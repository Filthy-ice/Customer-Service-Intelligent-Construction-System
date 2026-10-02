// 桌面端推进操作：全部复用 icewright-core 入口（与 CLI 同一状态机路径，历史自动入账）。
// 覆盖 init/预检/语料提取/渲染/闸门/生成/验证，客户端全流程闭环、不必回命令行。
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use icewright_core::{
    config, corpus, delivery, design, extract, generate, history, model, preflight, providers,
    secrets, state, verify, workspace, Workspace,
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
        "extract" => run_extract(ws_id),
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

/* ---------- 工作区文件读写：corpus/（S3 输入）与交付目录（S5 生成物）双向可编辑 ---------- */

/// 语料分类子目录，与引擎建目录/导入白名单同源，杜绝两处漂移。
pub const CORPUS_CATS: [&str; 6] = corpus::CATS;
const FILE_MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, serde::Serialize)]
pub struct WsFile {
    pub rel: String,
    pub bytes: u64,
}

#[derive(serde::Serialize)]
pub struct WorkspacePaths {
    pub root: String,
    pub corpus: String,
    pub output: String,
    /// 已设定的交付目录（空=未设定）
    pub delivery_dir: String,
    /// 交付目录是否已由客户确认（未确认时 S5 拒绝生成）
    pub delivery_confirmed: bool,
}

fn seg_ok(seg: &str) -> bool {
    !seg.is_empty()
        && seg != "."
        && seg != ".."
        && seg
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// 归一化任意相对路径：反斜杠转正斜杠，逐段校验安全字符，杜绝越出根目录。
fn norm_rel(rel: &str) -> Result<Vec<String>, String> {
    let r = rel.trim().replace('\\', "/");
    let segs: Vec<String> = r.split('/').map(|s| s.to_string()).collect();
    if segs.iter().any(|s| !seg_ok(s)) {
        return Err("invalid_rel".to_string());
    }
    Ok(segs)
}

/// corpus 只接受 `分类/文件名` 两段扁平路径，首段须在分类白名单内（便于 S3 归集）。
fn corpus_rel_checked(rel: &str) -> Result<String, String> {
    let segs = norm_rel(rel)?;
    if segs.len() != 2 || !CORPUS_CATS.contains(&segs[0].as_str()) {
        return Err("invalid_rel".to_string());
    }
    Ok(segs.join("/"))
}

/// output 允许任意层级的安全路径（含 src/main/java/…、app/…）。
fn output_rel_checked(rel: &str) -> Result<String, String> {
    let segs = norm_rel(rel)?;
    if segs.is_empty() {
        return Err("invalid_rel".to_string());
    }
    Ok(segs.join("/"))
}

/// 递归列出 subdir 下所有文件（rel 相对 subdir），按路径排序。skip_root_readme 时跳过顶层 README.md。
fn walk_files(dir: &std::path::Path, skip: &str) -> Result<Vec<WsFile>, String> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).map_err(|e| e.to_string())? {
            let p = entry.map_err(|e| e.to_string())?.path();
            if p.is_dir() {
                // 与引擎同一套构建垃圾判定（node_modules/target 等不进文件树视图）
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if workspace::is_build_junk(&name) {
                    continue;
                }
                stack.push(p);
            } else if p.is_file() {
                let rel = p.strip_prefix(dir).map_err(|e| e.to_string())?;
                let rel = rel.to_string_lossy().replace('\\', "/");
                if rel == skip {
                    continue;
                }
                let bytes = p.metadata().map(|m| m.len()).unwrap_or(0);
                files.push(WsFile { rel, bytes });
            }
        }
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(files)
}

pub fn ws_paths(ws_id: &str) -> Result<WorkspacePaths, String> {
    let ws = open(ws_id)?;
    let cfg = ws.config().map_err(e2s)?;
    Ok(WorkspacePaths {
        root: ws.root.display().to_string(),
        corpus: ws.root.join("corpus").display().to_string(),
        output: ws.artifact_browse_dir().display().to_string(),
        delivery_dir: cfg.workspace.delivery_dir.trim().to_string(),
        delivery_confirmed: cfg.workspace.delivery_customer_confirmed,
    })
}

pub fn corpus_list(ws_id: &str) -> Result<Vec<WsFile>, String> {
    let ws = open(ws_id)?;
    // 引擎自带的 corpus/README.md 是说明文件不是语料，不展示
    walk_files(&ws.root.join("corpus"), "README.md")
}

pub fn output_list(ws_id: &str) -> Result<Vec<WsFile>, String> {
    let ws = open(ws_id)?;
    let dir = ws.artifact_browse_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    // 顶层说明文件仍可展示；仅生成清单本身不作为可编辑代码呈现
    walk_files(&dir, "")
}

fn read_file_under(base: &std::path::Path, rel: &str) -> Result<String, String> {
    let p = base.join(rel);
    std::fs::read_to_string(&p).map_err(|e| e.to_string())
}

fn write_file_under(base: &std::path::Path, rel: &str, content: &str) -> Result<String, String> {
    if content.len() > FILE_MAX_BYTES {
        return Err("file_too_large".to_string());
    }
    let p = base.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, content).map_err(|e| e.to_string())?;
    Ok(rel.to_string())
}

pub fn corpus_read(ws_id: &str, rel: &str) -> Result<String, String> {
    let rel = corpus_rel_checked(rel)?;
    read_file_under(&open(ws_id)?.root.join("corpus"), &rel)
}

pub fn corpus_save(ws_id: &str, rel: &str, content: &str) -> Result<String, String> {
    let rel = corpus_rel_checked(rel)?;
    write_file_under(&open(ws_id)?.root.join("corpus"), &rel, content)
}

/// 按路径导入客户语料的回报：分类落点 + 新增/相同/跳过计数（文案由前端 i18n 组装）。
#[derive(Debug, serde::Serialize)]
pub struct CorpusImportResult {
    pub cat: String,
    pub copied: Vec<String>,
    pub identical: usize,
    pub skipped_binary: usize,
}

/// 客户材料留在原处：文件/目录按路径拷进 corpus/<分类>/ 快照。
/// 桌面端没有"当前目录"概念：裸相对路径按用户主目录解析（绝对与 ~/ 原样交给引擎）。
pub fn corpus_import(ws_id: &str, path: &str, cat: &str) -> Result<CorpusImportResult, String> {
    let ws = open(ws_id)?;
    let raw = path.trim();
    if raw.is_empty() {
        return Err("empty_path".to_string());
    }
    let resolved = {
        let p = std::path::Path::new(raw);
        if p.is_absolute() || raw.starts_with('~') {
            raw.to_string()
        } else {
            let home = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|h| std::path::PathBuf::from(h).join(raw).display().to_string());
            home.unwrap_or_else(|| raw.to_string())
        }
    };
    let c = cat.trim();
    let rep =
        corpus::import(&ws, &resolved, if c.is_empty() { None } else { Some(c) }).map_err(e2s)?;
    Ok(CorpusImportResult {
        cat: if c.is_empty() {
            corpus::DEFAULT_CAT.to_string()
        } else {
            c.to_string()
        },
        copied: rep.copied,
        identical: rep.identical,
        skipped_binary: rep.skipped_binary,
    })
}

pub fn output_read(ws_id: &str, rel: &str) -> Result<String, String> {
    let rel = output_rel_checked(rel)?;
    read_file_under(&open(ws_id)?.artifact_browse_dir(), &rel)
}

/// 直接改写交付目录里的文件；S8 重生成冲突告警逻辑不变（前端已提示）。
pub fn output_save(ws_id: &str, rel: &str, content: &str) -> Result<String, String> {
    let rel = output_rel_checked(rel)?;
    write_file_under(&open(ws_id)?.artifact_browse_dir(), &rel, content)
}

/// 设定交付目录（生成前由客户规定去向；支持 ~ 开头，变更后须重新确认）。
/// 返回展开后的绝对路径。稳定错误码供前端双语映射。
pub fn delivery_set(ws_id: &str, dir: &str) -> Result<String, String> {
    let ws = open(ws_id)?;
    if dir.trim().is_empty() {
        return Err("empty_dest".to_string());
    }
    let d = workspace::expand_home(dir.trim()).map_err(|e| e.to_string())?;
    if !d.is_absolute() {
        return Err("need_abs".to_string());
    }
    let d = ws.set_delivery_dir(dir).map_err(e2s)?;
    Ok(d.display().to_string())
}

/// 客户确认交付目录（未确认 S5 拒绝生成）。返回生效路径。
pub fn delivery_confirm(ws_id: &str) -> Result<String, String> {
    let ws = open(ws_id)?;
    let d = ws.confirm_delivery().map_err(e2s)?;
    Ok(d.display().to_string())
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

/// S3 语料提取：与 CLI 同一条 core 路径（五类产物按依赖顺序），模型通道走 workspace 配置。
/// 长耗时调用，靠前端 busy + 这里的 Guard 串行化；期间其他推进操作会被挡下。
fn run_extract(ws_id: &str) -> Result<String, String> {
    let ws = open(ws_id)?;
    let path = ws.state_path();
    if !path.exists() {
        return Err("need_init".to_string());
    }
    let st_before = state::load_state(&path).map_err(e2s)?;
    if st_before.stage(state::StageId::S2).unwrap().status != state::StageStatus::Approved {
        return Err("s2_not_approved".to_string());
    }
    let cfg = ws.require_configured().map_err(e2s)?;
    let key = secrets::resolve(&secrets::SecretRef::parse(&cfg.model.key_ref).map_err(e2s)?)
        .map_err(e2s)?;
    let st = extract::run(&ws, &extract::Kind::ORDER, |msgs| {
        let o = model::chat(
            &cfg.model,
            &key,
            msgs,
            false,
            std::time::Duration::from_secs(120),
        )?;
        Ok(extract::CallOutcome {
            content: o.content,
            model: Some(o.model),
            tokens_in: o.tokens_in,
            tokens_out: o.tokens_out,
        })
    })
    .map_err(e2s)?;
    Ok(format!(
        "S3 提取完成：五类产物已入账（apis→flows→dictionary→rules→skills）\n当前阶段：{:?}",
        st.current_stage
    ))
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
    let out_dir = ws.require_delivery_dir().map_err(e2s)?;
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
    let out_dir = ws.require_delivery_dir().map_err(e2s)?;
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
    fn corpus_crud_roundtrip_and_path_guard() {
        let _serial = HOME_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("iw-dsk-corpus-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        ws_create("corp-ws").unwrap();
        // 引擎自带的说明文件不当作语料展示
        assert!(corpus_list("corp-ws").unwrap().is_empty());

        assert_eq!(
            corpus_save("corp-ws", "rules/req.md", "# 需求").unwrap(),
            "rules/req.md"
        );
        assert_eq!(corpus_read("corp-ws", "rules/req.md").unwrap(), "# 需求");
        let list = corpus_list("corp-ws").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].rel, "rules/req.md");
        assert_eq!(list[0].bytes, "# 需求".len() as u64);

        // 越界与非法路径全部拒绝，写不到 corpus/ 之外
        for bad in [
            "../icewright.toml",
            "rules/../apis/x.md",
            "/abs/x.md",
            "notes/x.md",
            "rules/",
            "rules/a/b.md",
        ] {
            assert!(corpus_save("corp-ws", bad, "x").is_err(), "应拒绝 {bad}");
            assert!(corpus_read("corp-ws", bad).is_err(), "应拒绝读取 {bad}");
        }
        // README 不许被语料路径伪装覆盖（corpus 根不在分类目录下，天然被 cat 白名单挡住）

        let paths = ws_paths("corp-ws").unwrap();
        assert!(paths.corpus.starts_with(&paths.root));
        assert!(std::path::Path::new(&paths.corpus).is_dir());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn corpus_import_from_customer_paths() {
        let _serial = HOME_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("iw-dsk-cimp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        ws_create("cimp-ws").unwrap();
        assert_eq!(
            corpus_import("cimp-ws", "  ", "rules").unwrap_err(),
            "empty_path"
        );
        // 不存在的源：引擎双语报错（非稳定 code），前端原样透出
        assert!(corpus_import("cimp-ws", "~/没有这个目录", "rules").is_err());

        // 客户材料留在原处：绝对路径目录、递归收集、剔除构建垃圾
        let inbox = home.join("客户材料");
        std::fs::create_dir_all(inbox.join("附件/node_modules")).unwrap();
        std::fs::write(inbox.join("理赔规则.md"), "48 小时").unwrap();
        std::fs::write(inbox.join("附件/接口清单.txt"), "POST /claim").unwrap();
        std::fs::write(inbox.join("附件/node_modules/j.js"), "junk").unwrap();
        let r = corpus_import("cimp-ws", inbox.to_str().unwrap(), "rules").unwrap();
        assert_eq!(r.cat, "rules");
        assert_eq!(r.copied, vec!["rules/理赔规则.md", "rules/接口清单.txt"]);
        assert!(inbox.join("理赔规则.md").is_file(), "原件不许被动");
        // 导入结果立刻进入面板模型（可点开编辑）
        let list = corpus_list("cimp-ws").unwrap();
        assert!(list.iter().any(|f| f.rel == "rules/理赔规则.md"));

        // 幂等重导入：只记 identical
        let r2 = corpus_import("cimp-ws", inbox.to_str().unwrap(), "rules").unwrap();
        assert!(r2.copied.is_empty() && r2.identical == 2, "{r2:?}");

        // 桌面端无 cwd 概念：裸相对路径按主目录解析；分类留空默认 other
        std::fs::write(home.join("话术.md"), "您好").unwrap();
        let r3 = corpus_import("cimp-ws", "话术.md", "").unwrap();
        assert_eq!(r3.copied, vec!["other/话术.md"]);
        assert_eq!(r3.cat, "other");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn output_crud_allows_nested_generated_paths() {
        let _serial = HOME_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("iw-dsk-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        ws_create("out-ws").unwrap();
        // 未生成时 output/ 不存在，列表返回空而不是报错
        assert!(output_list("out-ws").unwrap().is_empty());

        // 生成的项目可任意层级查看/修改
        let rel = "src/main/java/com/x/App.java";
        assert_eq!(output_save("out-ws", rel, "class App {}").unwrap(), rel);
        assert_eq!(output_read("out-ws", rel).unwrap(), "class App {}");
        let list = output_list("out-ws").unwrap();
        assert!(list.iter().any(|f| f.rel == rel), "{list:?}");

        // 但仍禁止越出 output/ 目录
        for bad in ["../icewright.toml", "../../etc/passwd", "/abs"] {
            assert!(output_save("out-ws", bad, "x").is_err(), "应拒绝 {bad}");
            assert!(output_read("out-ws", bad).is_err(), "应拒绝读取 {bad}");
        }
        let paths = ws_paths("out-ws").unwrap();
        assert!(paths.output.starts_with(&paths.root));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn delivery_set_confirm_gates_generation_and_browse_follows() {
        let _serial = HOME_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("iw-dsk-deliv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        ws_create("del-ws").unwrap();
        assert_eq!(delivery_set("del-ws", "").unwrap_err(), "empty_dest");
        assert_eq!(
            delivery_set("del-ws", "relative/dir").unwrap_err(),
            "need_abs"
        );
        // 未设定交付目录：生成被硬闸拒绝，报错自带 set 指引
        let err = dispatch("generate", "del-ws", None).unwrap_err();
        assert!(err.contains("delivery set"), "{err}");

        let dir = delivery_set("del-ws", "~/客服交付").unwrap();
        assert_eq!(dir, home.join("客服交付").display().to_string());
        let paths = ws_paths("del-ws").unwrap();
        assert_eq!(paths.delivery_dir, dir);
        assert!(!paths.delivery_confirmed);
        assert_eq!(paths.output, dir, "浏览面板应跟随已设定的交付目录");
        // 已设定未确认：仍拒绝，报错自带 confirm 指引
        let err = dispatch("generate", "del-ws", None).unwrap_err();
        assert!(err.contains("delivery confirm"), "{err}");

        assert_eq!(delivery_confirm("del-ws").unwrap(), dir);
        assert!(ws_paths("del-ws").unwrap().delivery_confirmed);

        // 文件直接写进交付目录；构建垃圾不进视图；工作区不留副本
        output_save("del-ws", "app/main.py", "print(1)").unwrap();
        std::fs::create_dir_all(format!("{dir}/node_modules/dep")).unwrap();
        std::fs::write(format!("{dir}/node_modules/dep/i.js"), "junk").unwrap();
        let list = output_list("del-ws").unwrap();
        assert!(list.iter().any(|f| f.rel == "app/main.py"), "{list:?}");
        assert!(list.iter().all(|f| !f.rel.contains("node_modules")));
        assert!(home.join("客服交付/app/main.py").is_file());
        assert!(!home
            .join(".icewright/workspaces/del-ws/output/app/main.py")
            .exists());

        // 改目录自动作废确认
        delivery_set("del-ws", "~/deliver2").unwrap();
        assert!(!ws_paths("del-ws").unwrap().delivery_confirmed);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn extract_requires_pipeline_and_preflight_first() {
        let _serial = HOME_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("iw-dsk-ext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);

        ws_create("ext-ws").unwrap();
        // 未初始化 → need_init；已初始化但 S2 未批 → s2_not_approved（都不碰模型）
        assert_eq!(
            dispatch("extract", "ext-ws", None).unwrap_err(),
            "need_init"
        );
        dispatch("init", "ext-ws", None).unwrap();
        assert_eq!(
            dispatch("extract", "ext-ws", None).unwrap_err(),
            "s2_not_approved"
        );
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
