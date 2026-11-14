use crate::{startup::Startup, store::Store};
use moon_core::settings::{Edit, Snapshot};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::windows::{IconExtWindows, WindowBuilderExtWindows, WindowExtWindows},
    window::{Icon, WindowBuilder},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Dwm::*,
        UI::{
            Controls::MARGINS,
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::PCWSTR,
};
use wry::{WebContext, WebViewBuilder, WebViewBuilderExtWindows, http::Response};

const ORIGIN: &str = "http://moon.localhost";
const MAX_MESSAGE: usize = 4 * 1024 * 1024;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}
#[derive(Debug)]
enum UserEvent {
    Call(Call),
    Reply(Value),
    MaterialChanged,
}

// The subclass sees sent preference broadcasts too. An event-loop message hook
// only sees queued messages, and ThemeChanged alone misses contrast changes.
struct MaterialObserver {
    hwnd: HWND,
    attached: Cell<bool>,
    pending: Cell<bool>,
    proxy: EventLoopProxy<UserEvent>,
}
impl MaterialObserver {
    fn new(hwnd: HWND, proxy: EventLoopProxy<UserEvent>) -> Result<Box<Self>, String> {
        let observer =
            Box::new(Self { hwnd, attached: Cell::new(false), pending: Cell::new(false), proxy });
        let installed = unsafe {
            SetWindowSubclass(
                hwnd,
                Some(material_subclass),
                1,
                (&*observer as *const Self) as usize,
            )
        };
        if !installed.as_bool() {
            return Err("Windows could not observe Settings appearance changes.".into());
        }
        observer.attached.set(true);
        Ok(observer)
    }
    fn request_refresh(&self) {
        if !self.pending.replace(true) && self.proxy.send_event(UserEvent::MaterialChanged).is_err()
        {
            self.pending.set(false);
        }
    }
}
impl Drop for MaterialObserver {
    fn drop(&mut self) {
        if self.attached.replace(false) {
            unsafe {
                let _ = RemoveWindowSubclass(self.hwnd, Some(material_subclass), 1);
            }
        }
    }
}
unsafe extern "system" fn material_subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    reference: usize,
) -> LRESULT {
    // Installed, called, and removed on the window thread; the boxed observer
    // remains owned by run's closure until after this callback is detached.
    let observer = unsafe { &*(reference as *const MaterialObserver) };
    if message == WM_NCDESTROY {
        observer.attached.set(false);
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(material_subclass), id);
        }
    } else if matches!(
        message,
        WM_SIZE
            | WM_DPICHANGED
            | WM_SETTINGCHANGE
            | WM_THEMECHANGED
            | WM_SYSCOLORCHANGE
            | WM_DWMCOMPOSITIONCHANGED
    ) {
        observer.request_refresh();
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
fn failure(message: impl ToString) -> Value {
    json!({"message":message.to_string()})
}
fn ipc_request(method: &str, extra: Value) -> Result<Value, String> {
    let mut message = json!({"version":1,"id":"settings","method":method});
    message.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    let reply = moon_win::ipc::request(&message).map_err(|e| e.to_string())?;
    if reply["ok"] != true {
        return Err(reply["error"]
            .as_str()
            .unwrap_or("Moon could not complete the request.")
            .into());
    }
    Ok(reply["result"].clone())
}
fn runtime() -> Value {
    match ipc_request("query", json!({"query":"stats"})) {
        Ok(stats) => {
            json!({"running":true,"message":"Your windows are being managed on this desktop.","stats":stats})
        }
        Err(_) => {
            json!({"running":false,"message":"You can change settings while Moon is stopped. They apply the next time it starts."})
        }
    }
}
fn data(snapshot: Snapshot, path: &std::path::Path, runtime: Value) -> Value {
    json!({"snapshot":snapshot,"path":path,"version":env!("CARGO_PKG_VERSION"),"runtime":runtime})
}
fn same_path(a: &str, b: &std::path::Path) -> bool {
    let a = fs::canonicalize(a).unwrap_or_else(|_| PathBuf::from(a));
    let b = fs::canonicalize(b).unwrap_or_else(|_| b.to_owned());
    a.to_string_lossy()
        .replace('/', "\\")
        .eq_ignore_ascii_case(&b.to_string_lossy().replace('/', "\\"))
}
fn handle(call: &Call, path: &std::path::Path) -> Result<Value, Value> {
    let store = Store::new(path.to_owned());
    match call.method.as_str() {
        "load" => Ok(data(store.load().map_err(|e| json!(e))?, path, runtime())),
        "status" => Ok(runtime()),
        "shortcuts" | "overview" => {
            ipc_request("query", json!({"query":call.method})).map_err(failure)
        }
        "application-status" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Check {
                expected_source: Option<String>,
            }
            let check: Check = serde_json::from_value(call.params.clone()).map_err(failure)?;
            let snapshot = store.load().map_err(|e| json!(e))?;
            if snapshot.source != check.expected_source {
                return Ok(
                    json!({"state":"changed","message":"The saved file changed in another editor. Reload to review it; your draft has not been overwritten."}),
                );
            }
            let state = runtime();
            if state["running"] != true {
                return Ok(
                    json!({"state":"unavailable","message":"Saved on this PC. Moon is not reachable; these settings will be read when it starts."}),
                );
            }
            if !state["stats"]["config_path"].as_str().is_some_and(|p| same_path(p, path)) {
                return Ok(
                    json!({"state":"other-config","message":"Saved on this PC. The running Moon is using a different configuration file."}),
                );
            }
            let expected = serde_json::to_value(&snapshot.config).map_err(failure)?;
            serde_json::to_value(crate::application::describe(
                &expected,
                ipc_request("query", json!({"query":"config"})),
                &state["stats"],
            ))
            .map_err(failure)
        }
        "save" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Save {
                expected_source: Option<String>,
                edits: Vec<Edit>,
            }
            let save: Save = serde_json::from_value(call.params.clone()).map_err(failure)?;
            if save.edits.len() > 512 {
                return Err(failure("Too many changes in one save."));
            }
            let snapshot =
                store.save(save.expected_source.as_deref(), &save.edits).map_err(|e| json!(e))?;
            let state = runtime();
            let message = if state["running"] == true {
                if state["stats"]["config_path"].as_str().is_some_and(|p| same_path(p, path)) {
                    match ipc_request("command", json!({"commands":["reload"]})) {
                        Ok(_) => {
                            let expected =
                                serde_json::to_value(&snapshot.config).map_err(failure)?;
                            if ipc_request("query", json!({"query":"config"}))
                                .is_ok_and(|actual| actual == expected)
                            {
                                "Saved and loaded by Moon.".to_owned()
                            } else {
                                "Saved. Moon is using different settings; refresh its status before continuing.".to_owned()
                            }
                        }
                        Err(error) => {
                            format!("Saved. Moon could not apply the changes yet: {error}")
                        }
                    }
                } else {
                    "Saved. The running Moon instance is using a different configuration.".into()
                }
            } else {
                "Saved. Your changes will apply when Moon starts.".into()
            };
            Ok(json!({"data":data(snapshot,path,state),"message":message}))
        }
        "open-config" => {
            if !path.exists() {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(failure)?;
                }
                use std::io::Write;
                match fs::OpenOptions::new().create_new(true).write(true).open(path) {
                    Ok(mut file) => file.write_all(b"config-version = 1\n").map_err(failure)?,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(failure(error)),
                }
            }
            moon_win::runtime::shell_execute("notepad.exe", &format!("\"{}\"", path.display()))
                .map_err(failure)?;
            Ok(Value::Null)
        }
        "open-reference" => {
            let folder = local_data()?.join("reference");
            fs::create_dir_all(&folder).map_err(failure)?;
            let reference = folder.join("configuration.md");
            fs::write(&reference, moon_core::config::reference_markdown()).map_err(failure)?;
            moon_win::runtime::shell_execute(
                "notepad.exe",
                &format!("\"{}\"", reference.display()),
            )
            .map_err(failure)?;
            Ok(Value::Null)
        }
        _ => Err(failure("This settings action is not supported.")),
    }
}
fn local_data() -> Result<PathBuf, Value> {
    std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("moon/settings"))
        .ok_or_else(|| failure("Windows did not provide a local application data folder."))
}
fn config_path(config: Option<PathBuf>) -> Result<PathBuf, String> {
    let path = match config {
        None => std::env::var_os("USERPROFILE")
            .map(|p| PathBuf::from(p).join(".config/moon/config.toml"))
            .ok_or("Windows did not provide a user profile folder.")?,
        Some(path) => path,
    };
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir().map_err(|e| e.to_string())?.join(path))
    }
}
fn trusted_url(url: &str) -> bool {
    url == ORIGIN || url == format!("{ORIGIN}/") || url == "moon://localhost/"
}

fn apply_material(window: &tao::window::Window) -> bool {
    let preferences = moon_win::surfaces::system_accessibility();
    let backdrop = if preferences.transparency_allowed { DWMSBT_MAINWINDOW } else { DWMSBT_NONE };
    let corner = DWMWCP_ROUND;
    let hwnd = HWND(window.hwnd() as *mut _);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&corner as *const DWM_WINDOW_CORNER_PREFERENCE).cast(),
            std::mem::size_of_val(&corner) as u32,
        );
        if DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&backdrop as *const DWM_SYSTEMBACKDROP_TYPE).cast(),
            std::mem::size_of_val(&backdrop) as u32,
        )
        .is_err()
        {
            return false;
        }
        let margin = if preferences.transparency_allowed { -1 } else { 0 };
        let margins = MARGINS {
            cxLeftWidth: margin,
            cxRightWidth: margin,
            cyTopHeight: margin,
            cyBottomHeight: margin,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins).is_ok() && preferences.transparency_allowed
    }
}

pub fn run() -> Result<(), String> {
    // Web content never inherits the window manager's UIAccess or an elevated token.
    let token =
        moon_win::platform::token_capabilities(std::process::id()).map_err(|e| e.to_string())?;
    if token.ui_access || token.integrity_rid >= 0x3000 {
        return Err("Open Moon Settings normally from the Start menu. Settings does not run as administrator.".into());
    }
    // Appearance reads use UISettings before WebView2 initializes its thread.
    let _apartment = moon_win::platform::Apartment::new().map_err(|e| e.to_string())?;
    let startup = Startup::parse(std::env::args_os().skip(1))?;
    let page = serde_json::to_string(startup.page.name()).map_err(|e| e.to_string())?;
    let path = config_path(startup.config)?;
    let loop_ = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("Moon Settings")
        // Reveal after the local page renders its initial settings or error.
        .with_visible(false)
        .with_transparent(false)
        // DWM's extended frame needs zero-valued pixels underneath it,
        // including newly exposed client pixels after a resize.
        .with_background_color((0, 0, 0, 0))
        .with_inner_size(LogicalSize::new(1000., 800.))
        .with_min_inner_size(LogicalSize::new(740., 540.))
        .with_window_icon(Icon::from_resource(101, None).ok())
        .with_taskbar_icon(Icon::from_resource(101, None).ok())
        .build(&loop_)
        .map_err(|e| e.to_string())?;
    let mica = apply_material(&window);
    let proxy = loop_.create_proxy();
    let material_observer = MaterialObserver::new(HWND(window.hwnd() as *mut _), proxy.clone())?;
    let reply_proxy = proxy.clone();
    let (sender, receiver) = mpsc::sync_channel::<Call>(8);
    thread::Builder::new()
        .name("moon-settings-files".into())
        .spawn(move || {
            let apartment = moon_win::platform::Apartment::new();
            for call in receiver {
                let result = match &apartment {
                    Ok(_) => handle(&call, &path),
                    Err(error) => Err(failure(error)),
                };
                let reply = match result {
                    Ok(value) => json!({"id":call.id,"ok":true,"result":value}),
                    Err(error) => json!({"id":call.id,"ok":false,"error":error}),
                };
                if reply_proxy.send_event(UserEvent::Reply(reply)).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let directory = local_data().map_err(|e| e.to_string())?.join("WebView2");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut context = WebContext::new(Some(directory));
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_background_color((0,0,0,0))
        .with_devtools(false)
        .with_browser_accelerator_keys(false)
        .with_initialization_script(format!("window.moonStartPage={page};document.addEventListener('DOMContentLoaded',()=>document.documentElement.classList.toggle('mica',{mica}));"))
        .with_navigation_handler(|url| trusted_url(&url))
        .with_new_window_req_handler(|_,_| wry::NewWindowResponse::Deny)
        .with_download_started_handler(|_,_| false)
        .with_drag_drop_handler(|_| true)
        .with_custom_protocol("moon".into(), |_, request| {
            let (mime, body): (&str, &[u8]) = match request.uri().path() {
                "/" => ("text/html; charset=utf-8",include_bytes!("../../../apps/settings/dist/index.html")),
                "/styles.css" => ("text/css; charset=utf-8",include_bytes!("../../../apps/settings/dist/styles.css")),
                "/app.js" => ("text/javascript; charset=utf-8",include_bytes!("../../../apps/settings/dist/app.js")),
                "/moon.svg" => ("image/svg+xml",include_bytes!("../../moon-win/assets/icons/moon.svg")),
                _ => return Response::builder().status(404).body(Vec::new().into()).unwrap(),
            };
            Response::builder().header("Content-Type",mime)
                .header("Content-Security-Policy","default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'")
                .header("X-Content-Type-Options","nosniff").body(body.to_vec().into()).unwrap()
        })
        .with_ipc_handler(move |request| {
            if !trusted_url(&request.uri().to_string()) || request.body().len() > MAX_MESSAGE { return; }
            if let Ok(call) = serde_json::from_str::<Call>(request.body()) { let _ = proxy.send_event(UserEvent::Call(call)); }
        })
        .with_url("moon://localhost/")
        .build(&window).map_err(|e|format!("Moon Settings could not open its interface. Check that Microsoft Edge WebView2 Runtime is installed.\n\n{e}\n\nRuntime help: https://developer.microsoft.com/microsoft-edge/webview2/"))?;
    let mut initialized = false;
    let mut shown = false;
    let startup_deadline = Instant::now() + Duration::from_secs(30);
    loop_.run(move |event, _, flow| {
        // A failed script or WebView process must not leave an invisible host.
        // The frontend's bounded load request renders its own error before this.
        if !shown && matches!(event, Event::NewEvents(_)) && Instant::now() >= startup_deadline {
            show_error("Moon Settings could not finish opening. Close this message and try opening Settings again. Your saved settings have not changed.");
            *flow = ControlFlow::Exit;
            return;
        }
        *flow = if shown { ControlFlow::Wait } else { ControlFlow::WaitUntil(startup_deadline) };
        match event {
            Event::UserEvent(UserEvent::MaterialChanged) => {
                material_observer.pending.set(false);
                let mica = apply_material(&window);
                let _ = webview.evaluate_script(&format!("document.documentElement.classList.toggle('mica',{mica})"));
            }
            Event::WindowEvent { event: WindowEvent::Focused(true), .. } => {
                let mica = apply_material(&window);
                let _ = webview.evaluate_script(&format!("document.documentElement.classList.toggle('mica',{mica});window.moonActivated?.()"));
            }
            Event::WindowEvent { event: WindowEvent::ThemeChanged(_), .. } => {
                let mica = apply_material(&window);
                let _ = webview.evaluate_script(&format!("document.documentElement.classList.toggle('mica',{mica})"));
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                if initialized { let _ = webview.evaluate_script("window.moonCloseRequested()"); }
                else { *flow = ControlFlow::Exit; }
            }
            Event::UserEvent(UserEvent::Call(call)) => {
                initialized = true;
                if call.method == "ready" {
                    if !shown {
                        shown = true;
                        *flow = ControlFlow::Wait;
                        window.set_visible(true);
                        window.set_focus();
                    }
                }
                else if call.method == "close" { *flow = ControlFlow::Exit; }
                else if let Err(error) = sender.try_send(call) {
                    let (mpsc::TrySendError::Full(call) | mpsc::TrySendError::Disconnected(call)) = error;
                    let reply = json!({"id":call.id,"ok":false,"error":{"message":"Moon is busy. Wait a moment and try again."}});
                    let _ = webview.evaluate_script(&format!("window.moonReceive({reply})"));
                }
            }
            Event::UserEvent(UserEvent::Reply(reply)) => { let _ = webview.evaluate_script(&format!("window.moonReceive({reply})")); }
            _ => {}
        }
    });
}
pub fn show_error(error: &str) {
    let title = moon_win::platform::wide("Moon Settings");
    let text = moon_win::platform::wide(error);
    unsafe {
        MessageBoxW(None, PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR);
    }
}
