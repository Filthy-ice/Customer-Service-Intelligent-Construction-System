// IceWright 桌面端：流程监控（S1–S8 进度实时视图）+ 生成历史回看 + 窗口内推进操作。
// 操作层直接复用 icewright-core 入口（与 CLI 同一状态机路径），不起服务。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ops;

use std::sync::atomic::{AtomicU8, Ordering};

use icewright_core::{history, state, Workspace};
use tauri::menu::{
    Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder,
};
use tauri::{Emitter, Manager};

/// 菜单栏文案语种：0=zh 1=en。前端切换语言时通过 set_menu_lang 同步。
static MENU_LANG: AtomicU8 = AtomicU8::new(0);

const GITHUB_URL: &str = "https://github.com/Filthy-ice/Ice-Wright";

fn menu_en() -> bool {
    MENU_LANG.load(Ordering::Relaxed) == 1
}

struct MenuText {
    file: &'static str,
    refresh: &'static str,
    reload: &'static str,
    quit: &'static str,
    view: &'static str,
    lang: &'static str,
    window: &'static str,
    help: &'static str,
    about: &'static str,
    github: &'static str,
}

const TEXTS_ZH: MenuText = MenuText {
    file: "文件",
    refresh: "刷新",
    reload: "重新加载",
    quit: "退出",
    view: "视图",
    lang: "语言",
    window: "窗口",
    help: "帮助",
    about: "关于 IceWright",
    github: "GitHub 主页",
};

const TEXTS_EN: MenuText = MenuText {
    file: "File",
    refresh: "Refresh",
    reload: "Reload",
    quit: "Quit",
    view: "View",
    lang: "Language",
    window: "Window",
    help: "Help",
    about: "About IceWright",
    github: "GitHub Homepage",
};

fn text() -> &'static MenuText {
    if menu_en() {
        &TEXTS_EN
    } else {
        &TEXTS_ZH
    }
}

fn item(
    app: &tauri::AppHandle,
    id: &str,
    label: &str,
    accel: Option<&str>,
) -> tauri::Result<MenuItem<tauri::Wry>> {
    let mut builder = MenuItemBuilder::new(label).id(id);
    if let Some(a) = accel {
        builder = builder.accelerator(a);
    }
    builder.build(app)
}

/// 用当前语种重建整条菜单栏；macOS 惯例把「关于/退出」放进应用菜单。
fn build_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let t = text();

    #[cfg(target_os = "macos")]
    let app_submenu = SubmenuBuilder::new(app, "IceWright")
        .item(&item(app, "about", t.about, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some("Hide IceWright"))?)
        .item(&PredefinedMenuItem::hide_others(app, Some("Hide Others"))?)
        .item(&PredefinedMenuItem::show_all(app, Some("Show All"))?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some(t.quit))?)
        .build()?;

    let mut file = SubmenuBuilder::new(app, t.file)
        .item(&item(app, "refresh", t.refresh, Some("F5"))?)
        .item(&item(app, "reload", t.reload, Some("CmdOrCtrl+R"))?)
        .separator();
    #[cfg(not(target_os = "macos"))]
    {
        file = file.item(&item(app, "quit", t.quit, Some("CmdOrCtrl+Q"))?);
    }
    let file = file.build()?;

    let lang_zh = item(app, "lang_zh", "中文", None)?;
    let lang_en = item(app, "lang_en", "English", None)?;
    let lang_submenu = SubmenuBuilder::new(app, t.lang)
        .item(&lang_zh)
        .item(&lang_en)
        .build()?;
    let view = SubmenuBuilder::new(app, t.view)
        .item(&lang_submenu)
        .build()?;

    let window = SubmenuBuilder::new(app, t.window)
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    let about = item(app, "about", t.about, None)?;
    let github = item(app, "github", t.github, None)?;
    let help = SubmenuBuilder::new(app, t.help)
        .item(&about)
        .item(&github)
        .build()?;

    #[allow(unused_mut)]
    let mut builder = MenuBuilder::new(app);
    #[cfg(target_os = "macos")]
    {
        builder = builder.item(&app_submenu);
    }
    let menu = builder
        .item(&file)
        .item(&view)
        .item(&window)
        .item(&help)
        .build()?;
    Ok(menu)
}

/// 安装/重建菜单栏到主窗口。
fn apply_menu(app: &tauri::AppHandle) -> Result<(), String> {
    let menu = build_menu(app).map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("main") {
        w.set_menu(menu).map_err(|e| e.to_string())?;
    }
    Ok(())
}

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

/// 前端切换界面语言后同步重建菜单栏语种。
#[tauri::command]
fn set_menu_lang(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    match lang.as_str() {
        "en" => MENU_LANG.store(1, Ordering::Relaxed),
        "zh" => MENU_LANG.store(0, Ordering::Relaxed),
        other => return Err(format!("未知语种: {other}")),
    }
    apply_menu(&app)
}

/// 窗口以 visible:false 启动，前端首帧渲染完成后调用本命令亮相，避免白屏闪烁。
#[tauri::command]
fn show_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let spawn = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let spawn = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let spawn = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
    };
    let _ = spawn;
}

/// 用系统默认浏览器打开项目主页（不引入 opener 插件，保持依赖精简）。
#[tauri::command]
fn open_github() {
    open_url(GITHUB_URL);
}

/// 菜单栏点击统一走这里：能本地处理的（退出/打开链接/切语种）直接处理，
/// 其余以 menu-action 事件转发给前端。
fn handle_menu_action(app: &tauri::AppHandle, id: &str) {
    match id {
        "quit" => app.exit(0),
        "github" => open_url(GITHUB_URL),
        "lang_zh" | "lang_en" => {
            MENU_LANG.store(if id == "lang_en" { 1 } else { 0 }, Ordering::Relaxed);
            let _ = apply_menu(app);
            let _ = app.emit("menu-action", format!("lang:{}", &id[5..]));
        }
        "refresh" | "reload" | "about" => {
            let _ = app.emit("menu-action", id);
        }
        _ => {}
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ws_list,
            pipeline_status,
            history_tail,
            app_version,
            ws_locale,
            run_op,
            set_menu_lang,
            show_window,
            open_github
        ])
        .setup(|app| {
            apply_menu(app.handle())?;
            // Linux 裸跑二进制没有 .desktop 关联时，GTK 回退齿轮图标；显式设置 _NET_WM_ICON。
            if let Some(w) = app.get_webview_window("main") {
                let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png"))?;
                w.set_icon(icon).map_err(|e| e.to_string())?;
            }
            Ok(())
        })
        .on_menu_event(|app, event| handle_menu_action(app, event.id().0.as_str()))
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
