//! YSOB desktop shell. Starts the bundled `ysob` engine on a free loopback port with its data in the
//! app data dir and the UI from the bundle, then points the window at it with the engine's token.
//! The engine's stdin is a pipe we hold: when the app goes, even by crashing, the engine follows.
//! While it starts, the handshake floats on the desktop in a see-through window of its own; the
//! main window stays hidden until the app page says it has drawn, then it comes in and the
//! handshake fades away.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

mod update;

const BG: tauri::window::Color = tauri::window::Color(0x0b, 0x0d, 0x0a, 0xff);

#[derive(Default)]
struct Engine(Mutex<Option<(Child, ChildStdin)>>);

/// When the splash went up: it stays at least SPLASH_MIN so it never just flickers.
struct SplashSince(Instant);
/// Set once the main window has been brought in, so the safety timer never re-shows a window
/// someone has since hidden to the tray.
#[derive(Default)]
struct Revealed(AtomicBool);
const SPLASH_MIN: Duration = Duration::from_millis(1400);
/// If the app page never says it's ready, the window comes up anyway after this.
const SPLASH_MAX: Duration = Duration::from_secs(20);
/// The handshake at 3x (104 px art) with room round it for its shadow and the grip.
const SPLASH_SIZE: f64 = 380.0;
/// The engine is started again by itself after a crash, but not more than this many times in
/// RESTART_WINDOW: past that something is wrong that a restart won't fix, and the boot page says so.
const RESTARTS: usize = 3;
const RESTART_WINDOW: Duration = Duration::from_secs(300);

/// The tray's "needs you" line, kept so the page can change its words.
struct NeedsItem(MenuItem<tauri::Wry>);

/// One attempt at starting the engine. Returns the child, its stdin, and the URL with the token.
fn start(bin: &Path, data: &Path, ui: &Path) -> Result<(Child, ChildStdin, String), String> {
    std::fs::create_dir_all(data).map_err(|e| format!("Can't make {}: {e}", data.display()))?;
    let log_path = data.join("engine.log");
    let log = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).map_err(|e| format!("Can't open {}: {e}", log_path.display()))?;
    let mut cmd = Command::new(bin);
    // Port 0: the engine binds a free port itself and only then prints the URL, so the address
    // (and the token in it) never points at a server someone else got onto first.
    cmd.args(["serve", "--exit-with-stdin", "--port", "0", "--data"])
        .arg(data)
        .arg("--ui")
        .arg(ui)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(log.try_clone().map_err(|e| e.to_string())?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Couldn't start the engine at {}: {e}", bin.display()))?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;

    // The first line carries the URL and its token; it never goes to the log. Everything after does.
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut log = log;
        let mut tx = Some(tx);
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match (line.contains("#token=").then(|| line.split_once("http://")).flatten(), tx.take()) {
                (Some((_, url)), Some(t)) => {
                    let _ = t.send(format!("http://{}", url.trim()));
                }
                (_, t) => {
                    tx = t;
                    let _ = writeln!(log, "{line}");
                }
            }
        }
    });
    let fail = |mut child: Child, why: String| {
        let _ = child.kill();
        let _ = child.wait();
        Err(format!("{why} See {}", log_path.display()))
    };
    let url = match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(u) => u,
        Err(_) => return fail(child, "The engine didn't come up.".into()),
    };
    let port: Option<u16> = url.strip_prefix("http://127.0.0.1:").and_then(|r| r.split('/').next()).and_then(|p| p.parse().ok());
    let Some(port) = port else {
        return fail(child, "The engine gave an address that isn't on this machine.".into());
    };
    // The engine binds before it prints; make sure it is still the one listening.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return fail(child, format!("The engine stopped right after opening port {port}."));
        }
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        if Instant::now() > deadline {
            return fail(child, format!("The engine never answered on port {port}."));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok((child, stdin, url))
}

fn paths(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let here = std::env::current_exe().map_err(|e| e.to_string())?;
    let bin = here.with_file_name(format!("ysob{}", std::env::consts::EXE_SUFFIX));
    // Its own folder: the webview keeps its caches in the app data dir too.
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?.join("engine");
    let ui = app.path().resource_dir().map_err(|e| e.to_string())?.join("ui");
    Ok((bin, data, ui))
}

/// Set while an engine is being started, so Try again pressed twice (or a restart racing it)
/// can never start two.
static BOOTING: AtomicBool = AtomicBool::new(false);

pub(crate) fn boot(app: tauri::AppHandle) {
    if BOOTING.swap(true, Ordering::SeqCst) {
        return;
    }
    boot_once(&app);
    BOOTING.store(false, Ordering::SeqCst);
}

fn boot_once(app: &tauri::AppHandle) {
    let result = paths(app).and_then(|(bin, data, ui)| {
        // A port taken between picking it and the engine binding it is the likely failure; try again.
        let mut last = String::new();
        for _ in 0..3 {
            match start(&bin, &data, &ui) {
                Ok(ok) => return Ok(ok),
                Err(e) => last = e,
            }
        }
        Err(last)
    });
    let Some(win) = app.get_webview_window("main") else { return };
    match result {
        Ok((child, stdin, url)) => {
            *app.state::<Engine>().0.lock().unwrap() = Some((child, stdin));
            match url.parse() {
                Ok(u) => {
                    let _ = win.navigate(u);
                }
                Err(e) => eprintln!("bad engine url: {e}"),
            }
        }
        Err(e) => {
            eprintln!("{e}");
            // The boot page turns red, stops its status lines and shows its buttons when the
            // text starts "The engine".
            fail_page(app, format!("The engine wouldn't start.\n\n{e}\n\nTry again, or open the log to see why."));
            reveal(app);
        }
    }
}

/// Watch the engine. If it stops by itself (not because we stopped it, which takes it out of
/// state first), start it again; the window moves to the new address on its own. A crash loop
/// ends on the boot page with the reason, a Try again button and the log.
fn watch(app: tauri::AppHandle) {
    let mut recent: Vec<Instant> = vec![];
    loop {
        std::thread::sleep(Duration::from_secs(2));
        let died = {
            let state = app.state::<Engine>();
            let mut g = state.0.lock().unwrap();
            match g.as_mut().map(|(child, _)| child.try_wait()) {
                Some(Ok(Some(status))) => {
                    *g = None;
                    Some(status)
                }
                _ => None,
            }
        };
        let Some(status) = died else { continue };
        recent.retain(|t| t.elapsed() < RESTART_WINDOW);
        if let Ok((_, data, _)) = paths(&app) {
            if let Ok(mut log) = std::fs::OpenOptions::new().append(true).open(data.join("engine.log")) {
                let _ = writeln!(log, "[shell] the engine stopped by itself ({status}); {} restart(s) in the last 5 minutes", recent.len());
            }
        }
        if recent.len() >= RESTARTS {
            fail_page(&app, format!("The engine keeps stopping ({status}), {RESTARTS} times in five minutes, so YSOB has stopped restarting it.\n\nTry again, or open the log to see why."));
            continue;
        }
        recent.push(Instant::now());
        let again = app.clone();
        std::thread::spawn(move || boot(again));
    }
}

/// Put the boot page up with an error on it (its Try again and Open the log buttons show with it).
fn fail_page(app: &tauri::AppHandle, text: String) {
    let Some(win) = app.get_webview_window("main") else { return };
    let show = format!("document.getElementById('msg').textContent = {}", serde_json::Value::String(text));
    let on_boot = win.url().map(|u| u.scheme() == "tauri" || u.host_str() == Some("tauri.localhost")).unwrap_or(true);
    if on_boot {
        let _ = win.eval(show);
    } else if let Ok(boot) = (if cfg!(windows) { "http://tauri.localhost/index.html" } else { "tauri://localhost/index.html" }).parse::<tauri::Url>() {
        let _ = win.navigate(boot);
        let w = win.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(800));
            let _ = w.eval(show);
        });
    }
}

/// The boot page's Try again: start the engine once more.
#[tauri::command]
fn engine_retry(app: tauri::AppHandle) {
    if app.state::<Engine>().0.lock().unwrap().is_some() {
        return;
    }
    std::thread::spawn(move || boot(app));
}

/// Open the engine's log in the system's text viewer.
#[tauri::command]
fn open_log(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let (_, data, _) = paths(&app)?;
    let log = data.join("engine.log");
    if !log.exists() {
        return Err("There is no log yet.".into());
    }
    app.opener().open_path(log.display().to_string(), None::<&str>).map_err(|e| e.to_string())
}

/// A squad member needs you. While the window is in front nothing happens (the page shows it);
/// otherwise the taskbar button flashes and a desktop notification says who and what.
#[tauri::command]
fn notify(app: tauri::AppHandle, title: String, body: String) {
    use tauri_plugin_notification::NotificationExt;
    let Some(w) = app.get_webview_window("main") else { return };
    let visible = w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false);
    if visible && w.is_focused().unwrap_or(false) {
        return;
    }
    if w.is_visible().unwrap_or(false) {
        let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));
    }
    let cut = |t: &str, n: usize| if t.chars().count() > n { format!("{}…", t.chars().take(n).collect::<String>()) } else { t.to_string() };
    let _ = app.notification().builder().title(cut(&title, 80)).body(cut(&body, 200)).show();
}

/// The page tells the tray how many things are waiting on you.
#[tauri::command]
fn tray_needs(app: tauri::AppHandle, count: u32) {
    let text = match count {
        0 => "Nothing needs you".to_string(),
        1 => "1 thing needs you".to_string(),
        n => format!("{n} things need you"),
    };
    if let Some(item) = app.try_state::<NeedsItem>() {
        let _ = item.0.set_text(&text);
        let _ = item.0.set_enabled(count > 0);
    }
    if let Some(tray) = app.tray_by_id("main") {
        let tip = if count == 0 { "You Son Of A Bitch".to_string() } else { format!("YSOB: {text}") };
        let _ = tray.set_tooltip(Some(tip));
    }
}

/// Bring the window up and hand the page a menu action (see App.svelte).
fn menu_action(app: &tauri::AppHandle, what: &str) {
    show(app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.eval(format!("window.dispatchEvent(new CustomEvent('ysob:menu', {{ detail: {} }}))", serde_json::Value::String(what.into())));
    }
}

/// Tell the app page its window is on screen, so it fades in now rather than unseen.
fn shown(w: &tauri::WebviewWindow) {
    let _ = w.eval("document.documentElement.classList.add('shown')");
}

/// Bring the main window in and fade the splash out over it. After the first time (a restart
/// of the engine, say) it only tells the page it is on screen.
fn reveal(app: &tauri::AppHandle) {
    if app.state::<Revealed>().0.swap(true, Ordering::SeqCst) {
        if let Some(w) = app.get_webview_window("main") {
            shown(&w);
        }
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(since) = app.try_state::<SplashSince>() {
            std::thread::sleep(SPLASH_MIN.saturating_sub(since.0.elapsed()));
        }
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.set_focus();
            shown(&w);
        }
        if let Some(s) = app.get_webview_window("splash") {
            let _ = s.eval("document.body.classList.add('out')");
            std::thread::sleep(Duration::from_millis(420));
            let _ = s.close();
        }
    });
}

/// Windows 11's Snap Layouts for our own maximise button. A frameless window has no native
/// button to hover, so the page asks for the same grid Win+Z opens, on the focused window.
#[tauri::command]
fn snap_layouts(window: tauri::WebviewWindow) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_LWIN};
        let _ = window.set_focus();
        let key = |vk: u16, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 } },
        };
        let keys = [key(VK_LWIN, false), key(0x5A, false), key(0x5A, true), key(VK_LWIN, true)];
        // SAFETY: a fixed array of fully initialised keyboard inputs, with its true length and size.
        unsafe { SendInput(keys.len() as u32, keys.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    }
    #[cfg(not(windows))]
    let _ = window;
}

/// The app page has drawn its first real screen: show it.
#[tauri::command]
fn app_ready(app: tauri::AppHandle) {
    reveal(&app);
}

/// The system's folder picker, for choosing a mission's project folder. The chosen path, or
/// nothing if it was cancelled.
#[tauri::command]
async fn pick_folder(window: tauri::WebviewWindow) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let picked = window.dialog().file().set_title("Choose the project folder").set_parent(&window).blocking_pick_folder()?;
        picked.into_path().ok().map(|p| p.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

/// Open a link from a reply in the system's browser. Web links only: an agent's words can't
/// make the app open a file, a program or any other kind of address.
#[tauri::command]
fn open_link(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let parsed = tauri::Url::parse(&url).map_err(|_| "That isn't a link.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("Only web links can be opened.".into());
    }
    app.opener().open_url(parsed.as_str(), None::<&str>).map_err(|e| e.to_string())
}

/// Close the engine's stdin so it shuts down by itself, then make sure.
pub(crate) fn stop(app: &tauri::AppHandle) {
    let Some((mut child, stdin)) = app.state::<Engine>().0.lock().unwrap().take() else { return };
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn show(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn toggle(app: &tauri::AppHandle) {
    match app.get_webview_window("main") {
        Some(w) if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) => {
            let _ = w.hide();
        }
        _ => show(app),
    }
}

/// Tray: show/hide the window, what's waiting on you, updates, the engine's log, and quit
/// (which stops the engine). Linux trays only open the menu.
fn tray(app: &tauri::App) -> tauri::Result<()> {
    let toggle_item = MenuItem::with_id(app, "toggle", "Show / hide", true, None::<&str>)?;
    let needs = MenuItem::with_id(app, "needs", "Nothing needs you", false, None::<&str>)?;
    let updates = MenuItem::with_id(app, "updates", "Check for updates…", true, None::<&str>)?;
    let log = MenuItem::with_id(app, "log", "Open the engine log", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit YSOB", true, None::<&str>)?;
    let gap = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(app, &[&toggle_item, &needs, &gap()?, &updates, &log, &gap()?, &quit])?;
    app.manage(NeedsItem(needs));
    let mut b = TrayIconBuilder::with_id("main")
        .tooltip("You Son Of A Bitch")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "toggle" => toggle(app),
            "needs" => menu_action(app, "needs"),
            "updates" => menu_action(app, "updates"),
            "log" => {
                let _ = open_log(app.clone());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                toggle(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show(app)))
        .plugin(tauri_plugin_updater::Builder::new().pubkey(update::pubkey().unwrap_or_default()).build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Engine::default())
        .manage(Revealed::default())
        .manage(update::Progress::default())
        .manage(update::Failed::default())
        .invoke_handler(tauri::generate_handler![update::app_version, update::update_check, update::update_progress, update::update_install, pick_folder, open_link, app_ready, snap_layouts, notify, tray_needs, engine_retry, open_log])
        .setup(|app| {
            // First the handshake on its own: no frame, no background, no taskbar button. A see-
            // through window needs Apple's private API on macOS, so there the window just opens.
            #[cfg(target_os = "macos")]
            let splash: Result<(), ()> = Err(());
            #[cfg(not(target_os = "macos"))]
            let splash = WebviewWindowBuilder::new(app, "splash", WebviewUrl::App("splash.html".into()))
                .title("You Son Of A Bitch")
                .inner_size(SPLASH_SIZE, SPLASH_SIZE)
                .resizable(false)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .focused(false)
                .center()
                .build();
            if splash.is_ok() {
                app.manage(SplashSince(Instant::now()));
            }
            // The main window draws its own top bar (ui/src/WindowBar.svelte), except on macOS
            // where the native one is left alone. It loads hidden on the boot page (which shows if the engine won't
            // start), moves to the engine when it is up, and shows when the app page is ready.
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("You Son Of A Bitch")
                .inner_size(1360.0, 860.0)
                .min_inner_size(980.0, 640.0)
                .background_color(BG)
                .decorations(cfg!(target_os = "macos"))
                .visible(splash.is_err())
                .center()
                .build()?;
            let late = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(SPLASH_MAX);
                if !late.state::<Revealed>().0.load(Ordering::SeqCst) {
                    reveal(&late);
                }
            });
            tray(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || boot(handle));
            let watcher = app.handle().clone();
            std::thread::spawn(move || watch(watcher));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building YSOB")
        .run(|app, ev| {
            if let RunEvent::Exit = ev {
                stop(app);
            }
        });
}
