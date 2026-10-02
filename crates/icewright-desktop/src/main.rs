// IceWright 桌面端：流程监控（S1–S8 进度实时视图）+ 生成历史回看 + 窗口内推进操作。
// 操作层直接复用 icewright-core 入口（与 CLI 同一状态机路径），不起服务。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ops;

use icewright_core::{history, state, Workspace};

#[tauri::command]
fn ws_list() -> Result<Vec<String>, String> {
    Workspace::list().map_err(|e| e.to_string())
}

/// 某 workspace 的完整 pipeline 状态；尚未 init 时返回 None（UI 显示引导文案）。
#[tauri::command]
fn pipeline_status(ws_id: String) -> Result<Option<state::PipelineState>, String> {
    let ws = Workspace::open(&ws_id).map_err(|e| e.to_string())?;
    let path = ws.state_path();
    if !path.exists() {
        return Ok(None);
    }
    state::load_state(&path)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn history_tail(ws_id: String, n: u32) -> Result<Vec<history::Event>, String> {
    let ws = Workspace::open(&ws_id).map_err(|e| e.to_string())?;
    history::tail(&ws, n as usize).map_err(|e| e.to_string())
}

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 界面文案语种（workspace.locale，读不到回退 zh）。
#[tauri::command]
fn ws_locale(ws_id: String) -> String {
    ops::locale(&ws_id)
}

/// 推进操作：dispatch 是阻塞的引擎调用，放到 spawn_blocking 里避免卡住 UI 线程。
#[tauri::command]
async fn run_op(op: String, ws_id: String, note: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || ops::dispatch(&op, &ws_id, note.as_deref()))
        .await
        .map_err(|e| format!("操作任务崩溃: {e}"))?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ws_list,
            pipeline_status,
            history_tail,
            app_version,
            ws_locale,
            run_op
        ])
        .run(tauri::generate_context!())
        .expect("IceWright 桌面端启动失败");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_command_roundtrips_fresh_workspace() {
        let base = std::env::temp_dir().join(format!("iw-dsk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = Workspace::create_at(&base, "dsk-a").unwrap();
        let st = state::PipelineState::new("dsk-a", "run-20261002-0001", Some("p@0.1.0"));
        state::save_state(&ws.state_path(), &st).unwrap();

        let loaded = state::load_state(&ws.state_path()).unwrap();
        assert_eq!(loaded.run_id, "run-20261002-0001");
        assert_eq!(loaded.stages.len(), state::StageId::ALL.len());

        history::record(&ws, "S1", "管线初始化").unwrap();
        let events = history::tail(&ws, 10).unwrap();
        assert_eq!(events.len(), 1);
        let json = serde_json::to_string(&events[0]).unwrap();
        assert!(json.contains("\"stage\":\"S1\""), "{json}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
