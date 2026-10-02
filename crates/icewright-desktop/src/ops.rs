// 桌面端推进操作：全部复用 icewright-core 入口（与 CLI 同一状态机路径，历史自动入账）。
// S3 提取需要模型通道与会话交互，仍只走 CLI；这里覆盖 init/预检/渲染/闸门/生成/验证。
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use icewright_core::{
    config, delivery, design, generate, history, preflight, providers, secrets, state, verify,
    Workspace,
};

/// 同一时刻只允许一个推进操作，防止并发改写 state.json。
static BUSY: AtomicBool = AtomicBool::new(false);

pub struct Guard;

impl Guard {
    fn acquire() -> Result<Guard, String> {
        if BUSY.swap(true, Ordering::SeqCst) {
            return Err("已有操作进行中，请稍候…".to_string());
        }
        Ok(Guard)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

fn e2s(e: anyhow::Error) -> String {
    e.to_string()
}

/// 与 CLI next_run_id 同规则：pipeline/history/ 归档里找当日最大序号。
fn next_run_id(ws_dir: &std::path::Path, date: &str) -> String {
    let prefix = format!("run-{date}-");
    let mut max_seq = 0u32;
    if let Ok(entries) = std::fs::read_dir(ws_dir.join("pipeline").join("history")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(rest) = name.strip_prefix(&prefix) {
                let num = rest.strip_suffix(".json").unwrap_or(rest);
                if let Ok(num) = num.parse::<u32>() {
                    max_seq = max_seq.max(num);
                }
            }
        }
    }
    format!("{prefix}{:04}", max_seq + 1)
}

/// UI 文案语种跟随 workspace.locale（读不到就回退 zh）。
pub fn locale(ws_id: &str) -> String {
    open(ws_id)
        .and_then(|ws| ws.config().map_err(e2s))
        .map(|cfg| cfg.workspace.locale)
        .unwrap_or_else(|_| "zh".to_string())
}

pub fn dispatch(op: &str, ws_id: &str, note: Option<&str>) -> Result<String, String> {
    let _guard = Guard::acquire()?;
    // 引擎报错语种跟随 workspace.locale（读不到就保持默认 zh）
    if let Ok(cfg) = open(ws_id).and_then(|ws| ws.config().map_err(e2s)) {
        icewright_core::i18n::set_lang(&cfg.workspace.locale);
    }
    match op {
        "init" => ws_init(ws_id),
        "preflight" => run_preflight(ws_id),
        "design_render" => design_render(ws_id),
        "design_approve" => design_decide(ws_id, true, None),
        "design_reject" => design_decide(ws_id, false, note),
        "generate" => run_generate(ws_id),
        "verify" => run_verify(ws_id),
        "delivery_render" => delivery_render(ws_id),
        "delivery_approve" => delivery_decide(ws_id, true, None),
        "delivery_reject" => delivery_decide(ws_id, false, note),
        other => Err(format!("未知操作: {other}")),
    }
}

fn open(ws_id: &str) -> Result<Workspace, String> {
    Workspace::open(ws_id).map_err(e2s)
}

/// 界面新建工作区：与 CLI ws new 同一引擎入口；ID 合法性与重名由引擎报错（已双语）。
pub fn ws_create(ws_id: &str) -> Result<String, String> {
    let id = ws_id.trim();
    if id.is_empty() {
        return Err("empty_ws_id".to_string());
    }
    let ws = Workspace::create(id).map_err(e2s)?;
    Ok(ws.root.display().to_string())
}

/* ---------- 模型（AI 供应商）配置：结构化返回，文案由前端 i18n ---------- */

#[derive(serde::Serialize)]
pub struct ModelInfo {
    pub base_url: String,
    pub model: String,
    /// 密钥引用 URI；plain: 形态只回显掩码，明文不出引擎
    pub key_ref: String,
}

pub fn model_get(ws_id: &str) -> Result<ModelInfo, String> {
    let cfg = open(ws_id)?.config().map_err(e2s)?;
    let shown = if secrets::SecretRef::parse(&cfg.model.key_ref)
        .map(|r| r.is_plaintext())
        .unwrap_or(false)
    {
        "plain:****".to_string()
    } else {
        cfg.model.key_ref.clone()
    };
    Ok(ModelInfo {
        base_url: cfg.model.base_url,
        model: cfg.model.model,
        key_ref: shown,
    })
}

/// 校验错误返回稳定 code（前端映射双语文案）；引擎错误透传（已双语）。
pub fn model_set(ws_id: &str, base_url: &str, model: &str, key_ref: &str) -> Result<(), String> {
    let ws = open(ws_id)?;
    let base = base_url.trim().trim_end_matches('/');
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err("invalid_base_url".to_string());
    }
    if model.trim().is_empty() {
        return Err("invalid_model".to_string());
    }
    let key = key_ref.trim();
    if !key.is_empty() && !key.starts_with("plain:****") {
        secrets::SecretRef::parse(key).map_err(e2s)?;
    } else if key.starts_with("plain:****") {
        // 掩码回显不允许原样存回，避免把 **** 当密钥
        return Err("invalid_key_ref".to_string());
    }
    let path = ws.root.join("icewright.toml");
    config::set_and_save(&path, "model.base_url", base).map_err(e2s)?;
    config::set_and_save(&path, "model.model", model.trim()).map_err(e2s)?;
    if !key.is_empty() {
        config::set_and_save(&path, "model.key_ref", key).map_err(e2s)?;
    }
    Ok(())
}

/// 在线发现可用模型：优先用给定 key_ref 解出密钥，否则按目录里同接入点的
/// key_envs 候选逐个取环境变量（与 CLI discover 同策略）。
pub fn model_discover(base_url: &str, key_ref: Option<&str>) -> Result<Vec<String>, String> {
    let base = base_url.trim().trim_end_matches('/');
    let key = match key_ref.map(str::trim).filter(|s| !s.is_empty()) {
        Some(r) => {
            let sr = secrets::SecretRef::parse(r).map_err(e2s)?;
            secrets::resolve(&sr).map_err(e2s)?
        }
        None => {
            let mut cands: Vec<String> = providers::catalog()
                .map_err(e2s)?
                .into_iter()
                .filter(|p| p.base_url.trim().trim_end_matches('/') == base)
                .flat_map(|p| p.key_envs)
                .collect();
            if cands
                .iter()
                .all(|v| std::env::var(v).ok().filter(|s| !s.is_empty()).is_none())
            {
                cands.push("OPENAI_API_KEY".to_string());
            }
            cands
                .iter()
                .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
                .ok_or_else(|| "no_key_env".to_string())?
        }
    };
    providers::list_models(base, &key, std::time::Duration::from_secs(20)).map_err(e2s)
}

pub fn provider_catalog() -> Result<Vec<providers::Provider>, String> {
    providers::catalog().map_err(e2s)
}

fn ws_init(ws_id: &str) -> Result<String, String> {
    let ws = open(ws_id)?;
    let path = ws.state_path();
    if path.exists() {
        return Err("pipeline 已存在（重跑请先归档 state.json）".to_string());
    }
    let cfg = ws.config().map_err(e2s)?;
    let date = Utc::now().format("%Y%m%d").to_string();
    let run_id = next_run_id(&ws.root, &date);
    let pack = if cfg.workspace.pack.trim().is_empty() {
        None
    } else {
        Some(cfg.workspace.pack.as_str())
    };
    let st = state::PipelineState::new(ws_id, &run_id, pack);
    state::save_state(&path, &st).map_err(e2s)?;
    history::record(&ws, "S1", &format!("管线初始化 run={run_id}")).map_err(e2s)?;
    Ok(format!("已初始化 {run_id}"))
}

fn run_preflight(ws_id: &str) -> Result<String, String> {
    let ws = open(ws_id)?;
    if !ws.state_path().exists() {
        return Err("请先初始化 pipeline".to_string());
    }
    let sroot = secrets::secrets_root().map_err(e2s)?;
    let (checks, all_ok) = preflight::run_and_record(&ws, &sroot).map_err(e2s)?;
    let mut lines: Vec<String> = checks
        .iter()
        .map(|c| {
            format!(
                "{}  {:<16} {}",
                if c.ok { "PASS" } else { "FAIL" },
                c.name,
                c.detail
            )
        })
        .collect();
    lines.push(if all_ok {
        "预检通过".to_string()
    } else {
        "预检未通过：修复 FAIL 项后重试".to_string()
    });
    Ok(lines.join("\n"))
}

fn design_render(ws_id: &str) -> Result<String, String> {
    let ws = open(ws_id)?;
    let (path, changed) = design::publish(&ws).map_err(e2s)?;
    Ok(if changed {
        format!("设计文档已更新，等待闸门A 确认：{path}")
    } else {
        format!("设计文档无变化（幂等重渲染）：{path}")
    })
}

fn design_decide(ws_id: &str, approve: bool, note: Option<&str>) -> Result<String, String> {
    let ws = open(ws_id)?;
    let decision = if approve {
        state::GateDecision::Approved
    } else {
        state::GateDecision::Rejected
    };
    design::decide(&ws, decision, "desktop", None, note).map_err(e2s)?;
    Ok(if approve {
        "闸门A 已批准（绑定当前产物哈希）".to_string()
    } else {
        "闸门A 已驳回，请修订后重新渲染".to_string()
    })
}

fn run_generate(ws_id: &str) -> Result<String, String> {
    let ws = open(ws_id)?;
    let out_dir = ws.root.join("output");
    let report = generate::generate(&ws, &out_dir).map_err(e2s)?;
    Ok(format!(
        "S5 生成完成：{} 个文件写入 · {} 个定制文件保留\n增量对比：新增 {} · 更新 {} · 未变 {} · 移除 {} · 冲突 {}\n输出目录：{}",
        report.written.len(),
        report.preserved.len(),
        report.diff.created.len(),
        report.diff.modified.len(),
        report.diff.unchanged.len(),
        report.diff.removed.len(),
        report.diff.conflicts.len(),
        report.out_dir.display()
    ))
}

fn run_verify(ws_id: &str) -> Result<String, String> {
    let ws = open(ws_id)?;
    let out_dir = ws.root.join("output");
    let checks = verify::verify(&ws, &out_dir, &verify::default_python()).map_err(e2s)?;
    let mut lines: Vec<String> = checks
        .iter()
        .map(|c| {
            format!(
                "{}  {:<10} {}",
                if c.ok { "PASS" } else { "FAIL" },
                c.name,
                c.detail
            )
        })
        .collect();
    lines.push(if checks.iter().all(|c| c.ok) {
        "S6 通过：可进入 S7 验收交付".to_string()
    } else {
        "S6 未通过：修复后重新验证".to_string()
    });
    Ok(lines.join("\n"))
}

fn delivery_render(ws_id: &str) -> Result<String, String> {
    let ws = open(ws_id)?;
    let (path, changed) = delivery::publish(&ws).map_err(e2s)?;
    Ok(if changed {
        format!("交付报告已更新，等待闸门B 验收：{path}")
    } else {
        format!("交付报告无变化（幂等重渲染）：{path}")
    })
}

fn delivery_decide(ws_id: &str, approve: bool, note: Option<&str>) -> Result<String, String> {
    let ws = open(ws_id)?;
    let decision = if approve {
        state::GateDecision::Approved
    } else {
        state::GateDecision::Rejected
    };
    delivery::decide(&ws, decision, "desktop", None, note).map_err(e2s)?;
    Ok(if approve {
        "闸门B 已验收，进入 S8 变更/重生成态".to_string()
    } else {
        "闸门B 已驳回，请修订后重新渲染交付报告".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // HOME 重定向影响全局搜索路径，串行执行。
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn locale_follows_workspace_config() {
        let _serial = HOME_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("iw-dsk-loc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        Workspace::create("loc-ws").unwrap();
        assert_eq!(locale("loc-ws"), "zh");
        let cfg_path = Workspace::open("loc-ws")
            .unwrap()
            .root
            .join("icewright.toml");
        icewright_core::config::set_and_save(&cfg_path, "workspace.locale", "en").unwrap();
        assert_eq!(locale("loc-ws"), "en");
        // 工作区不存在时回退 zh 而不是报错
        assert_eq!(locale("no-such-ws"), "zh");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn dispatch_init_and_preflight_end_to_end_under_temp_home() {
        let _serial = HOME_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("iw-dsk-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        // 新 workspace：init 前 dispatch 报「不存在」
        assert!(dispatch("init", "live-e2e", None).is_err());
        Workspace::create("live-e2e").unwrap();
        let out = dispatch("init", "live-e2e", None).unwrap();
        assert!(out.starts_with("已初始化 run-"), "{out}");
        // 重复 init 被状态机拒绝
        assert!(dispatch("init", "live-e2e", None)
            .unwrap_err()
            .contains("已存在"));
        // 预检走完整引擎路径：即使环境不全也返回 PASS/FAIL 清单
        let out = dispatch("preflight", "live-e2e", None);
        let report = match out {
            Ok(r) => r,
            Err(e) => panic!("preflight dispatch 应返回清单而非中断: {e}"),
        };
        assert!(
            report.contains("PASS") || report.contains("FAIL"),
            "{report}"
        );
        // S1/S2 事件已入账（与 CLI 同一历史账本）
        let ws = Workspace::open("live-e2e").unwrap();
        let events = history::tail(&ws, 10).unwrap();
        assert!(events.iter().any(|e| e.stage == "S1"), "{events:?}");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn init_then_preflight_gate_flow_on_temp_workspace() {
        let base = std::env::temp_dir().join(format!("iw-dsk-op-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        // Workspace::open 走全局 base_dir，这里直接测纯函数层：run_id + 状态落盘
        let ws = Workspace::create_at(&base, "dsk-op").unwrap();
        let date = "20261002";
        assert_eq!(next_run_id(&ws.root, date), "run-20261002-0001");
        let st = state::PipelineState::new("dsk-op", "run-20261002-0001", None);
        state::save_state(&ws.state_path(), &st).unwrap();
        assert!(ws.state_path().exists());
        // 闸门决策在未等待时必失败（错误信息透传，不 panic）
        let err = design_decide("dsk-op", true, None).unwrap_err();
        assert!(!err.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn ws_create_makes_workspace_and_rejects_duplicate() {
        let _serial = HOME_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("iw-dsk-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        assert_eq!(ws_create("  ").unwrap_err(), "empty_ws_id");
        let root = ws_create("ui-ws").unwrap();
        assert!(std::path::Path::new(&root).join("icewright.toml").exists());
        // 重名由引擎报错（双语），列表可见
        assert!(ws_create("ui-ws").unwrap_err().contains("ui-ws"));
        assert!(Workspace::list().unwrap().contains(&"ui-ws".to_string()));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn busy_guard_serializes_ops() {
        let g = Guard::acquire().unwrap();
        assert!(Guard::acquire().is_err(), "占用期间第二次 acquire 必须失败");
        drop(g);
        assert!(Guard::acquire().is_ok());
    }

    #[test]
    fn model_config_roundtrip_validation_and_masking() {
        let _serial = HOME_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("iw-dsk-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        Workspace::create("mod-ws").unwrap();
        let info = model_get("mod-ws").unwrap();
        assert!(info.base_url.is_empty() && info.model.is_empty());

        assert_eq!(
            model_set("mod-ws", "ftp://x", "m", "").unwrap_err(),
            "invalid_base_url"
        );
        assert_eq!(
            model_set("mod-ws", "https://api.example.com/v1", "  ", "").unwrap_err(),
            "invalid_model"
        );
        // 非法 key_ref 由引擎报错（双语），不是稳定 code
        assert!(model_set("mod-ws", "https://api.example.com/v1", "m", "bogus-ref").is_err());

        model_set(
            "mod-ws",
            "https://api.example.com/v1/",
            "deepseek-chat",
            "env://DEEPSEEK_API_KEY",
        )
        .unwrap();
        let info = model_get("mod-ws").unwrap();
        assert_eq!(
            info.base_url, "https://api.example.com/v1",
            "尾部斜杠应去除"
        );
        assert_eq!(info.model, "deepseek-chat");
        assert_eq!(info.key_ref, "env://DEEPSEEK_API_KEY");

        // plain 密钥回显必须掩码，且掩码值不许原样存回
        model_set(
            "mod-ws",
            "https://api.example.com/v1",
            "m",
            "plain:secret123",
        )
        .unwrap();
        assert_eq!(model_get("mod-ws").unwrap().key_ref, "plain:****");
        assert_eq!(
            model_set("mod-ws", "https://api.example.com/v1", "m", "plain:****").unwrap_err(),
            "invalid_key_ref"
        );

        assert!(!provider_catalog().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }
}
