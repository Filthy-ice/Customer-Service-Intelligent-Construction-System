use crate::secrets::{self, SecretRef};
use crate::state::{self, PipelineState, StageId, StageStatus};
use crate::workspace::Workspace;
use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

pub const ALLOWED_STACKS: &[&str] = &["python", "java", "go"];

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

impl Check {
    fn new(name: &'static str, ok: bool, detail: String) -> Self {
        Self { name, ok, detail }
    }
}

/// 解析 OpenAI-compatible base_url：scheme://host[:port][/path] → (tls, host, port)。
pub fn split_base_url(base: &str) -> Result<(bool, String, u16), String> {
    let (tls, rest) = if let Some(r) = base.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = base.strip_prefix("http://") {
        (false, r)
    } else {
        return Err("base_url 必须以 http:// 或 https:// 开头".into());
    };
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() {
        return Err("base_url 缺少主机名".into());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() => {
            let port = p.parse::<u16>().map_err(|_| format!("端口非法: {p:?}"))?;
            (h.to_string(), port)
        }
        _ => (authority.to_string(), if tls { 443 } else { 80 }),
    };
    Ok((tls, host, port))
}

fn tcp_reachable(host: &str, port: u16, timeout: Duration) -> Result<(), String> {
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS 解析失败 {host}:{port}: {e}"))?;
    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(_) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }
    Err(match last_err {
        Some(e) => format!("无法连接 {host}:{port}: {e}"),
        None => format!("{host}:{port} 无解析地址"),
    })
}

fn corpus_has_files(ws: &Workspace) -> Result<bool> {
    let dir = ws.root.join("corpus");
    if !dir.exists() {
        return Ok(false);
    }
    Ok(std::fs::read_dir(&dir)?.next().is_some())
}

/// S2 环境预检：全部依赖外部条件（模型端点、密钥、行业包、需求语料）必须先绿。
pub fn run(ws: &Workspace, secrets_root: &Path) -> Result<Vec<Check>> {
    let cfg = ws.config()?;
    let mut checks = Vec::new();

    checks.push(Check::new(
        "corpus",
        corpus_has_files(ws).unwrap_or(false),
        "需求语料 corpus/ 至少需要一个文件".into(),
    ));

    let timeout = Duration::from_secs(3);
    let url_check = match split_base_url(cfg.model.base_url.trim()) {
        Ok((_, host, port)) => match tcp_reachable(&host, port, timeout) {
            Ok(()) => Check::new("model_endpoint", true, format!("{host}:{port} 可达")),
            Err(e) => Check::new("model_endpoint", false, e),
        },
        Err(e) => Check::new(
            "model_endpoint",
            false,
            format!("{e}（当前值 {:?}）", cfg.model.base_url),
        ),
    };
    checks.push(url_check);

    checks.push(Check::new(
        "model_name",
        !cfg.model.model.trim().is_empty(),
        if cfg.model.model.trim().is_empty() {
            "model.model 未填写".into()
        } else {
            cfg.model.model.clone()
        },
    ));

    let key_check = match SecretRef::parse(cfg.model.key_ref.trim()) {
        Err(e) => Check::new("model_key", false, format!("{e:#}")),
        Ok(r) => match secrets::resolve_at(secrets_root, &r) {
            Ok(_) => Check::new("model_key", true, format!("{} 可解析", r.to_uri())),
            Err(_) => Check::new(
                "model_key",
                false,
                format!(
                    "{} 引用了但本机无此密钥，用 `icewright secret set` 写入",
                    r.to_uri()
                ),
            ),
        },
    };
    checks.push(key_check);

    checks.push(Check::new(
        "stack",
        ALLOWED_STACKS.contains(&cfg.workspace.stack.as_str()),
        format!(
            "stack={:?}（允许: {}）",
            cfg.workspace.stack,
            ALLOWED_STACKS.join("/")
        ),
    ));

    checks.push(Check::new(
        "pack",
        !cfg.workspace.pack.trim().is_empty(),
        if cfg.workspace.pack.trim().is_empty() {
            "workspace.pack 未填写".into()
        } else {
            cfg.workspace.pack.clone()
        },
    ));

    checks.extend(datasource_checks(&cfg, secrets_root, timeout));

    Ok(checks)
}

/// Redis/MySQL 协议级探测；未配置 host 则非阻塞跳过。
fn datasource_checks(
    cfg: &crate::config::Config,
    secrets_root: &Path,
    timeout: std::time::Duration,
) -> Vec<Check> {
    let mut out = Vec::new();

    let redis = &cfg.datasource.redis;
    if redis.host.trim().is_empty() {
        out.push(Check::new(
            "datasource_redis",
            true,
            "未配置（跳过）".into(),
        ));
    } else {
        let pass = if redis.key_ref.trim().is_empty() {
            None
        } else {
            match SecretRef::parse(redis.key_ref.trim())
                .and_then(|r| secrets::resolve_at(secrets_root, &r))
            {
                Ok(p) => Some(p),
                Err(e) => {
                    out.push(Check::new(
                        "datasource_redis",
                        false,
                        format!("密码引用无法解析: {e:#}"),
                    ));
                    return out;
                }
            }
        };
        out.push(
            match crate::datasource::redis_probe(
                redis.host.trim(),
                redis.port,
                pass.as_deref(),
                timeout,
            ) {
                Ok(d) => Check::new("datasource_redis", true, d),
                Err(e) => Check::new("datasource_redis", false, e),
            },
        );
    }

    let mysql = &cfg.datasource.mysql;
    if mysql.host.trim().is_empty() {
        out.push(Check::new(
            "datasource_mysql",
            true,
            "未配置（跳过）".into(),
        ));
    } else {
        out.push(
            match crate::datasource::mysql_probe(mysql.host.trim(), mysql.port, timeout) {
                Ok(d) => Check::new("datasource_mysql", true, d),
                Err(e) => Check::new("datasource_mysql", false, e),
            },
        );
    }

    out
}

/// 跑预检并把结论写回 pipeline 状态（S1/S2 状态与失败明细）。返回检查清单。
pub fn run_and_record(ws: &Workspace, secrets_root: &Path) -> Result<(Vec<Check>, bool)> {
    let checks = run(ws, secrets_root)?;
    let all_ok = checks.iter().all(|c| c.ok);
    let path = ws.state_path();
    let mut st = match load_or_init(ws)? {
        Some(st) => st,
        None => anyhow::bail!("请先 `icewright pipeline init {}`", ws.id),
    };

    let now = Utc::now();
    let corpus_ok = checks.iter().any(|c| c.name == "corpus" && c.ok);
    let cfg_raw = std::fs::read(ws.root.join("icewright.toml"))?;
    for s in &mut st.stages {
        if s.id == StageId::S1
            && corpus_ok
            && matches!(s.status, StageStatus::Pending | StageStatus::Running)
        {
            s.status = StageStatus::Approved;
            s.ended_at = Some(now);
        }
        if s.id == StageId::S2 {
            s.input_hash = Some(state::input_hash(&[&cfg_raw]));
            s.started_at = Some(now);
            s.ended_at = Some(now);
            s.failures = checks
                .iter()
                .filter(|c| !c.ok)
                .map(|c| state::StageFailure {
                    kind: "preflight_failed".to_string(),
                    detail: Some(format!("{}: {}", c.name, c.detail)),
                    located_refs: vec![format!("preflight:{}", c.name)],
                })
                .collect();
            s.status = if all_ok {
                StageStatus::Approved
            } else {
                StageStatus::BlockedPreflight
            };
        }
    }
    if all_ok && st.current_stage == StageId::S1 {
        st.current_stage = StageId::S3;
    }
    st.updated_at = Some(now);
    state::save_state(&path, &st)?;
    Ok((checks, all_ok))
}

fn load_or_init(ws: &Workspace) -> Result<Option<PipelineState>> {
    let path = ws.state_path();
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(state::load_state(&path)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn split_base_url_cases() {
        assert_eq!(
            split_base_url("https://api.example.com/v1").unwrap(),
            (true, "api.example.com".into(), 443)
        );
        assert_eq!(
            split_base_url("http://127.0.0.1:11434").unwrap(),
            (false, "127.0.0.1".into(), 11434)
        );
        assert!(split_base_url("api.example.com").is_err());
        assert!(split_base_url("https://").is_err());
        assert!(split_base_url("http://host:abc").is_err());
    }

    fn make_ws(base: &Path) -> Workspace {
        let ws = Workspace::create_at(base, "pf-test").unwrap();
        let mut cfg = std::fs::read_to_string(ws.root.join("icewright.toml")).unwrap();
        cfg = cfg.replace("base_url = \"\"", "base_url = \"http://127.0.0.1:1\"");
        cfg = cfg.replace("model = \"\"", "model = \"test-model\"");
        cfg = cfg.replace("key_ref = \"\"", "key_ref = \"keyring://pf/test\"");
        cfg = cfg.replace("pack = \"\"", "pack = \"insurance/auto-claim@0.1.0\"");
        std::fs::write(ws.root.join("icewright.toml"), cfg).unwrap();
        std::fs::write(ws.root.join("corpus/需求.md"), "车险理赔规则").unwrap();
        ws
    }

    #[test]
    fn run_reports_each_check_independently() {
        let base = std::env::temp_dir().join(format!("iw-pf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let sroot = base.join("secrets");
        let ws = make_ws(&base);
        secrets::store_at(
            &sroot,
            &SecretRef::parse("keyring://pf/test").unwrap(),
            "sk",
        )
        .unwrap();

        let checks = run(&ws, &sroot).unwrap();
        let get = |n: &str| checks.iter().find(|c| c.name == n).unwrap().ok;
        assert!(get("corpus"));
        assert!(get("model_name"));
        assert!(get("model_key"));
        assert!(get("stack"));
        assert!(get("pack"));
        // port 1 不可达 → 预检整体不放行
        assert!(!get("model_endpoint"));
        let st0 = state::PipelineState::new(
            "pf-test",
            "run-20261001-0001",
            Some("insurance/auto-claim@0.1.0"),
        );
        state::save_state(&ws.state_path(), &st0).unwrap();
        let (_, all_ok) = run_and_record(&ws, &sroot).unwrap();
        assert!(!all_ok);
        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(
            st.stage(StageId::S2).unwrap().status,
            StageStatus::BlockedPreflight
        );
        assert_eq!(st.stage(StageId::S2).unwrap().failures.len(), 1);

        // 指向本机监听端口后可全绿
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1];
            let _ = c.read(&mut buf);
        });
        icewright_core_set_stack_url(&ws, &format!("http://{addr}"));
        let (checks, all_ok) = run_and_record(&ws, &sroot).unwrap();
        assert!(all_ok, "failed checks: {checks:?}");
        let st = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(st.stage(StageId::S2).unwrap().status, StageStatus::Approved);
        assert_eq!(st.stage(StageId::S1).unwrap().status, StageStatus::Approved);
        assert_eq!(st.current_stage, StageId::S3);
        assert!(st.stage(StageId::S2).unwrap().input_hash.is_some());
        let _ = std::fs::remove_dir_all(&base);
    }

    fn icewright_core_set_stack_url(ws: &Workspace, url: &str) {
        let raw = std::fs::read_to_string(ws.root.join("icewright.toml")).unwrap();
        let raw = raw.replace(
            "base_url = \"http://127.0.0.1:1\"",
            &format!("base_url = \"{url}\""),
        );
        std::fs::write(ws.root.join("icewright.toml"), raw).unwrap();
    }
}
