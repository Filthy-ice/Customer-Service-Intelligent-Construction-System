// 桌面端推进操作：全部复用 icewright-core 入口（与 CLI 同一状态机路径，历史自动入账）。
// S3 提取需要模型通道与会话交互，仍只走 CLI；这里覆盖 init/预检/渲染/闸门/生成/验证。
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use icewright_core::{
    delivery, design, generate, history, preflight, secrets, state, verify, Workspace,
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

pub fn dispatch(op: &str, ws_id: &str, note: Option<&str>) -> Result<String, String> {
    let _guard = Guard::acquire()?;
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
    fn busy_guard_serializes_ops() {
        let g = Guard::acquire().unwrap();
        assert!(Guard::acquire().is_err(), "占用期间第二次 acquire 必须失败");
        drop(g);
        assert!(Guard::acquire().is_ok());
    }
}
