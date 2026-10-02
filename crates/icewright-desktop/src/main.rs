// IceWright 桌面端：流程监控（S1–S8 进度实时视图）+ 生成历史回看 + 窗口内推进操作。
// 操作层直接复用 icewright-core 入口（与 CLI 同一状态机路径），不起服务。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ops;

use std::sync::atomic::{AtomicU8, Ordering};

use icewright_core::{history, state, Workspace};
use tauri::menu::{
    CheckMenuItemBuilder, Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem,
    SubmenuBuilder,
};
use tauri::{Emitter, Manager};

/// 菜单栏文案语种：0=zh 1=en。前端切换语言时通过 set_menu_lang 同步。
static MENU_LANG: AtomicU8 = AtomicU8::new(0);
/// 语言偏好勾选态：0=跟随工作区 1=中文 2=English（前端 sync_prefs 同步）。
static LANG_PREF: AtomicU8 = AtomicU8::new(0);
/// 主题勾选态：0=跟随系统 1=浅色 2=深色。
static THEME_PREF: AtomicU8 = AtomicU8::new(0);

const GITHUB_URL: &str = "https://github.com/Filthy-ice/Ice-Wright";

fn menu_en() -> bool {
    MENU_LANG.load(Ordering::Relaxed) == 1
}

struct MenuText {
    file: &'static str,
    refresh: &'static str,
    reload: &'static str,
    settings: &'static str,
    open_corpus: &'static str,
    open_output: &'static str,
    export: &'static str,
    quit: &'static str,
    settings_menu: &'static str,
    lang: &'static str,
    lang_auto: &'static str,
    lang_zh: &'static str,
    lang_en: &'static str,
    appearance: &'static str,
    theme_auto: &'static str,
    theme_light: &'static str,
    theme_dark: &'static str,
    window: &'static str,
    win_min: &'static str,
    win_max: &'static str,
    win_close: &'static str,
    help: &'static str,
    about: &'static str,
    github: &'static str,
}

const TEXTS_ZH: MenuText = MenuText {
    file: "文件",
    refresh: "刷新",
    reload: "重新加载",
    settings: "模型设置…",
    open_corpus: "打开语料目录",
    open_output: "打开生成目录",
    export: "交付目录…",
    quit: "退出",
    settings_menu: "设置",
    lang: "界面语言",
    lang_auto: "自动（跟随工作区语言）",
    lang_zh: "始终显示中文",
    lang_en: "始终显示 English",
    appearance: "主题",
    theme_auto: "跟随系统深浅色",
    theme_light: "始终浅色",
    theme_dark: "始终深色",
    window: "窗口",
    win_min: "最小化",
    win_max: "最大化 / 还原",
    win_close: "关闭窗口",
    help: "帮助",
    about: "关于 IceWright",
    github: "GitHub 主页",
};

const TEXTS_EN: MenuText = MenuText {
    file: "File",
    refresh: "Refresh",
    reload: "Reload",
    settings: "Model Settings…",
    open_corpus: "Open Corpus Folder",
    open_output: "Open Output Folder",
    export: "Delivery Folder…",
    quit: "Quit",
    settings_menu: "Settings",
    lang: "UI Language",
    lang_auto: "Auto (follow workspace locale)",
    lang_zh: "Always Chinese",
    lang_en: "Always English",
    appearance: "Theme",
    theme_auto: "Follow System",
    theme_light: "Always Light",
    theme_dark: "Always Dark",
    window: "Window",
    win_min: "Minimize",
    win_max: "Maximize / Restore",
    win_close: "Close Window",
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

/// 带勾选态的菜单项：当前生效的语言/主题在菜单里直接可见。
fn check_item(
    app: &tauri::AppHandle,
    id: &str,
    label: &str,
    on: bool,
) -> tauri::Result<tauri::menu::CheckMenuItem<tauri::Wry>> {
    CheckMenuItemBuilder::new(label)
        .id(id)
        .checked(on)
        .build(app)
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

    // macOS 惯例：退出放应用菜单；Win/Linux 收进文件菜单。设置统一独立成顶级菜单。
    #[cfg(target_os = "macos")]
    let file = SubmenuBuilder::new(app, t.file)
        .item(&item(app, "refresh", t.refresh, Some("F5"))?)
        .item(&item(app, "reload", t.reload, Some("CmdOrCtrl+R"))?)
        .separator()
        .item(&item(app, "open_corpus", t.open_corpus, None)?)
        .item(&item(app, "open_output", t.open_output, None)?)
        .item(&item(app, "delivery", t.export, None)?)
        .build()?;
    #[cfg(not(target_os = "macos"))]
    let file = SubmenuBuilder::new(app, t.file)
        .item(&item(app, "refresh", t.refresh, Some("F5"))?)
        .item(&item(app, "reload", t.reload, Some("CmdOrCtrl+R"))?)
        .separator()
        .item(&item(app, "open_corpus", t.open_corpus, None)?)
        .item(&item(app, "open_output", t.open_output, None)?)
        .item(&item(app, "delivery", t.export, None)?)
        .separator()
        .item(&item(app, "quit", t.quit, Some("CmdOrCtrl+Q"))?)
        .build()?;

    // 语言/主题直接做成带勾选的菜单项（选项自带描述文案，不再藏进设置模态）。
    let lang_sel = LANG_PREF.load(Ordering::Relaxed);
    let lang_submenu = SubmenuBuilder::new(app, t.lang)
        .item(&check_item(app, "lang_auto", t.lang_auto, lang_sel == 0)?)
        .item(&check_item(app, "lang_zh", t.lang_zh, lang_sel == 1)?)
        .item(&check_item(app, "lang_en", t.lang_en, lang_sel == 2)?)
        .build()?;
    let theme_sel = THEME_PREF.load(Ordering::Relaxed);
    let theme_submenu = SubmenuBuilder::new(app, t.appearance)
        .item(&check_item(
            app,
            "theme_auto",
            t.theme_auto,
            theme_sel == 0,
        )?)
        .item(&check_item(
            app,
            "theme_light",
            t.theme_light,
            theme_sel == 1,
        )?)
        .item(&check_item(
            app,
            "theme_dark",
            t.theme_dark,
            theme_sel == 2,
        )?)
        .build()?;
    let settings_menu = SubmenuBuilder::new(app, t.settings_menu)
        .item(&item(app, "settings", t.settings, Some("CmdOrCtrl+,"))?)
        .separator()
        .item(&lang_submenu)
        .item(&theme_submenu)
        .build()?;

    // GTK 环境下 PredefinedMenuItem 的窗口项会渲染为置灰不可点，改为自实现项，
    // 点击后在 handle_menu_action 里直接调 WebviewWindow 方法（三平台一致可用）。
    let window = SubmenuBuilder::new(app, t.window)
        .item(&item(app, "win_min", t.win_min, Some("CmdOrCtrl+M"))?)
        .item(&item(app, "win_max", t.win_max, None)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&item(app, "win_close", t.win_close, Some("CmdOrCtrl+W"))?)
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
        .item(&settings_menu)
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

/// 界面上直接创建工作区（与 CLI ws new 同一引擎入口，ID 校验/重名报错由引擎负责）。
#[tauri::command]
fn ws_create(ws_id: String) -> Result<String, String> {
    ops::ws_create(&ws_id)
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

/* ---------- 需求语料与生成物（corpus/ 与 output/）：GUI 内查看编辑，无需回命令行 ---------- */
#[tauri::command]
fn ws_paths(ws_id: String) -> Result<ops::WorkspacePaths, String> {
    ops::ws_paths(&ws_id)
}

#[tauri::command]
fn corpus_list(ws_id: String) -> Result<Vec<ops::WsFile>, String> {
    ops::corpus_list(&ws_id)
}

#[tauri::command]
fn corpus_read(ws_id: String, rel: String) -> Result<String, String> {
    ops::corpus_read(&ws_id, &rel)
}

#[tauri::command]
fn corpus_save(ws_id: String, rel: String, content: String) -> Result<String, String> {
    ops::corpus_save(&ws_id, &rel, &content)
}

#[tauri::command]
fn corpus_cats() -> Vec<&'static str> {
    ops::CORPUS_CATS.to_vec()
}

/// 语料按路径导入：客户材料留在原处，拷贝进 corpus/<分类>/ 快照（目录拷贝放阻塞线程池）。
#[tauri::command]
async fn corpus_import(
    ws_id: String,
    path: String,
    cat: String,
    update: bool,
) -> Result<ops::CorpusImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || ops::corpus_import(&ws_id, &path, &cat, update))
        .await
        .map_err(|e| format!("导入任务崩溃: {e}"))?
}

#[tauri::command]
fn output_list(ws_id: String) -> Result<Vec<ops::WsFile>, String> {
    ops::output_list(&ws_id)
}

#[tauri::command]
fn output_read(ws_id: String, rel: String) -> Result<String, String> {
    ops::output_read(&ws_id, &rel)
}

#[tauri::command]
fn output_save(ws_id: String, rel: String, content: String) -> Result<String, String> {
    ops::output_save(&ws_id, &rel, &content)
}

/// 在系统文件管理器中打开生成物目录（失败回传前端明示，不静默）。
#[tauri::command]
fn open_output_dir(ws_id: String) -> Result<(), String> {
    let paths = ops::ws_paths(&ws_id)?;
    open_url(&paths.output)
}

/// 在系统文件管理器中打开语料目录。
#[tauri::command]
fn open_corpus_dir(ws_id: String) -> Result<(), String> {
    let paths = ops::ws_paths(&ws_id)?;
    open_url(&paths.corpus)
}

/// 交付目录设定：生成前由客户规定去向（支持 ~），改目录会作废确认。返回展开后绝对路径。
#[tauri::command]
async fn delivery_set(ws_id: String, dir: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || ops::delivery_set(&ws_id, &dir))
        .await
        .map_err(|e| format!("交付目录任务崩溃: {e}"))?
}

/// 客户确认交付目录（未确认 S5 拒绝生成）。返回生效路径。
#[tauri::command]
async fn delivery_confirm(ws_id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || ops::delivery_confirm(&ws_id))
        .await
        .map_err(|e| format!("交付目录任务崩溃: {e}"))?
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

/// 同步 语言/主题 偏好（0/1/2），刷新菜单栏勾选态。
#[tauri::command]
fn sync_prefs(app: tauri::AppHandle, lang: u8, theme: u8) -> Result<(), String> {
    if lang > 2 {
        return Err(format!("未知语言偏好: {lang}"));
    }
    if theme > 2 {
        return Err(format!("未知主题偏好: {theme}"));
    }
    LANG_PREF.store(lang, Ordering::Relaxed);
    THEME_PREF.store(theme, Ordering::Relaxed);
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

/// 由模板生成 .desktop 内容：Exec 指向当前二进制，Icon 指向解出的 PNG。
fn desktop_entry(exe: &std::path::Path, icon: &std::path::Path) -> String {
    include_str!("../packaging/icewright-desktop.desktop.in")
        .replace("%EXEC%", &exe.display().to_string())
        .replace("%ICON%", &icon.display().to_string())
}

/// Linux 裸二进制（含 AppImage/dev 直跑）没有 .desktop 关联时，Ubuntu 任务栏按
/// WM_CLASS 找不到启动条目就回退齿轮。这里幂等写一份用户级启动条目；
/// deb/rpm 装的是系统级条目，互不影响。
#[cfg(target_os = "linux")]
fn ensure_desktop_entry() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME 未设置".to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let icon_dir = std::path::Path::new(&home).join(".local/share/icons");
    std::fs::create_dir_all(&icon_dir).map_err(|e| e.to_string())?;
    let icon_path = icon_dir.join("icewright-desktop.png");
    if !icon_path.is_file() {
        std::fs::write(
            &icon_path,
            include_bytes!("../icons/128x128.png").as_slice(),
        )
        .map_err(|e| e.to_string())?;
    }
    let entry = desktop_entry(&exe, &icon_path);
    let apps = std::path::Path::new(&home).join(".local/share/applications");
    std::fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
    let path = apps.join("icewright-desktop.desktop");
    // 仅在内容变化（如移动了二进制）时重写，避免每次启动都落盘。
    if std::fs::read_to_string(&path)
        .map(|s| s != entry)
        .unwrap_or(true)
    {
        std::fs::write(&path, entry).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn ensure_desktop_entry() -> Result<(), String> {
    Ok(())
}

fn open_url(url: &str) -> Result<(), String> {
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
    spawn.map(|_| ()).map_err(|e| e.to_string())
}

/// 用系统默认浏览器打开项目主页（不引入 opener 插件，保持依赖精简）。
#[tauri::command]
fn open_github() -> Result<(), String> {
    open_url(GITHUB_URL)
}

#[tauri::command]
fn model_providers() -> Result<Vec<icewright_core::providers::Provider>, String> {
    ops::provider_catalog()
}

#[tauri::command]
fn model_get(ws_id: String) -> Result<ops::ModelInfo, String> {
    ops::model_get(&ws_id)
}

#[tauri::command]
fn model_set(
    ws_id: String,
    base_url: String,
    model: String,
    key_ref: String,
) -> Result<(), String> {
    ops::model_set(&ws_id, &base_url, &model, &key_ref)
}

/// 在线发现是阻塞 HTTP 调用，放 spawn_blocking 避免卡 UI 线程。
#[tauri::command]
async fn model_discover(base_url: String, key_ref: Option<String>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || ops::model_discover(&base_url, key_ref.as_deref()))
        .await
        .map_err(|e| format!("发现任务崩溃: {e}"))?
}

/// 菜单栏点击统一走这里：能本地处理的（退出/打开链接/切语种）直接处理，
/// 其余以 menu-action 事件转发给前端。
fn handle_menu_action(app: &tauri::AppHandle, id: &str) {
    match id {
        "quit" => app.exit(0),
        "github" => {
            // 打开失败（如系统未关联默认浏览器）不能让菜单项"点了没动静"：回传前端明示手动地址
            if let Err(e) = open_url(GITHUB_URL) {
                eprintln!("open_url failed: {e}");
                let _ = app.emit("menu-action", "github_failed");
            }
        }
        "lang_zh" | "lang_en" => {
            let en = id == "lang_en";
            MENU_LANG.store(if en { 1 } else { 0 }, Ordering::Relaxed);
            LANG_PREF.store(if en { 2 } else { 1 }, Ordering::Relaxed);
            let _ = apply_menu(app);
            let _ = app.emit("menu-action", format!("lang:{}", &id[5..]));
        }
        "lang_auto" => {
            LANG_PREF.store(0, Ordering::Relaxed);
            let _ = apply_menu(app);
            let _ = app.emit("menu-action", "lang:auto");
        }
        "theme_auto" | "theme_light" | "theme_dark" => {
            THEME_PREF.store(
                match id {
                    "theme_light" => 1,
                    "theme_dark" => 2,
                    _ => 0,
                },
                Ordering::Relaxed,
            );
            let _ = apply_menu(app);
            let _ = app.emit("menu-action", format!("theme:{}", &id[6..]));
        }
        "refresh" | "reload" | "about" | "settings" | "open_corpus" | "open_output"
        | "delivery" => {
            let _ = app.emit("menu-action", id);
        }
        "win_min" | "win_max" | "win_close" => {
            if let Some(w) = app.get_webview_window("main") {
                let r = match id {
                    "win_min" => w.minimize(),
                    "win_max" => {
                        if w.is_maximized().unwrap_or(false) {
                            w.unmaximize()
                        } else {
                            w.maximize()
                        }
                    }
                    _ => w.close(),
                };
                if let Err(e) = r {
                    eprintln!("window action {id} failed: {e}");
                }
            }
        }
        _ => {}
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ws_list,
            ws_create,
            pipeline_status,
            ws_paths,
            corpus_list,
            corpus_read,
            corpus_save,
            corpus_cats,
            corpus_import,
            output_list,
            output_read,
            output_save,
            open_output_dir,
            open_corpus_dir,
            delivery_set,
            delivery_confirm,
            history_tail,
            app_version,
            ws_locale,
            run_op,
            set_menu_lang,
            sync_prefs,
            show_window,
            open_github,
            model_providers,
            model_get,
            model_set,
            model_discover
        ])
        .setup(|app| {
            apply_menu(app.handle())?;
            // 任务栏图标依赖 .desktop 条目：注册失败只告警，不影响启动。
            if let Err(e) = ensure_desktop_entry() {
                eprintln!("desktop entry not registered: {e}");
            }
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
    fn desktop_entry_fills_exec_and_icon() {
        let text = desktop_entry(
            std::path::Path::new("/opt/icewright-desktop"),
            std::path::Path::new("/home/u/.local/share/icons/icewright-desktop.png"),
        );
        assert!(text.contains("Exec=/opt/icewright-desktop"), "{text}");
        assert!(
            text.contains("Icon=/home/u/.local/share/icons/icewright-desktop.png"),
            "{text}"
        );
        assert!(
            !text.contains("%EXEC%") && !text.contains("%ICON%"),
            "{text}"
        );
        // 任务栏按 WM_CLASS 匹配：StartupWMClass 必须与 tao 实际注册的类名一致
        assert!(text.contains("StartupWMClass=icewright-desktop"), "{text}");
    }

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
