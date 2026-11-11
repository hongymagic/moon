//! Input callbacks only compare and enqueue. No keys leave this thread.

use crate::platform::{wide, window_id};
use moon_core::ids::WindowId;
use std::{
    cell::RefCell,
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            LibraryLoader::GetModuleHandleW, RemoteDesktop::*, Threading::GetCurrentThreadId,
        },
        UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{Error, PCWSTR, Result},
};

#[derive(Clone, Debug)]
pub struct HookBinding {
    pub chord: String,
    pub commands: Vec<String>,
}
#[derive(Clone, Debug)]
pub enum PlatformEvent {
    Window { kind: u32, id: WindowId },
    Commands { commands: Arc<[String]>, received: std::time::Instant },
    EmergencyRestore,
    HookError(String),
    System(SystemEvent),
    PointerMoved,
}

#[derive(Clone, Copy, Debug)]
pub enum SystemEvent {
    DisplayChanged,
    SettingsChanged,
    SessionLocked,
    SessionUnlocked,
    Suspended,
    Resumed,
    DesktopChanged,
    ShellRestarted,
    SessionEnding,
}

#[derive(Clone)]
struct Binding {
    key: u32,
    modifiers: u8,
    commands: Arc<[String]>,
}
struct State {
    bindings: Vec<Binding>,
    sender: Sender<PlatformEvent>,
    pressed: [bool; 256],
    swallowed: HashSet<u32>,
    taskbar_message: u32,
    pointer: Arc<PointerState>,
}
thread_local! { static STATE:RefCell<Option<State>> = const { RefCell::new(None) }; }
const UPDATE: u32 = WM_APP + 41;
const RENEW: u32 = WM_APP + 42;
const POINTER: u32 = WM_APP + 43;
// Documented WM_WTSSESSION_CHANGE value, absent from windows 0.62.2 metadata.
const SESSION_DESKTOP_READY: u32 = 0x0f;
#[derive(Default)]
struct PointerState {
    position: AtomicU64,
    pending: AtomicBool,
}
const EMERGENCY: i32 = 0x4d01;
#[cfg(test)]
static TEST_RENEWALS: AtomicU64 = AtomicU64::new(0);

fn modifier_snapshot(mut down: impl FnMut(VIRTUAL_KEY) -> bool) -> [bool; 256] {
    let mut pressed = [false; 256];
    for key in
        [VK_LMENU, VK_RMENU, VK_LSHIFT, VK_RSHIFT, VK_LCONTROL, VK_RCONTROL, VK_LWIN, VK_RWIN]
    {
        pressed[key.0 as usize] = down(key);
    }
    pressed
}

fn modifier_mask(pressed: &[bool; 256]) -> u8 {
    let down = |key: VIRTUAL_KEY| pressed[key.0 as usize];
    u8::from(down(VK_LMENU) || down(VK_RMENU))
        | (u8::from(down(VK_LSHIFT) || down(VK_RSHIFT)) << 1)
        | (u8::from(down(VK_LCONTROL) || down(VK_RCONTROL)) << 2)
        | (u8::from(down(VK_LWIN) || down(VK_RWIN)) << 3)
}

fn record_key(pressed: &mut [bool; 256], info: &KBDLLHOOKSTRUCT, up: bool) -> u32 {
    let key = if matches!(info.vkCode, 0x10..=0x12) {
        // Keep only a sided latch. Right Ctrl/Alt are extended keys even when
        // the current input layout maps their scan codes to Hangul/Hanja.
        let (left, right) = match info.vkCode {
            0x10 => (VK_LSHIFT, VK_RSHIFT),
            0x11 => (VK_LCONTROL, VK_RCONTROL),
            _ => (VK_LMENU, VK_RMENU),
        };
        let is_right = if info.vkCode == u32::from(VK_SHIFT.0) {
            (unsafe { MapVirtualKeyW(info.scanCode, MAPVK_VSC_TO_VK_EX) }) == u32::from(VK_RSHIFT.0)
        } else {
            info.flags.contains(LLKHF_EXTENDED)
        };
        u32::from(if is_right { right.0 } else { left.0 })
    } else {
        info.vkCode
    };
    pressed[key as usize] = !up;
    key
}

const LATENCY_BUCKETS: usize = 4097;
static LATENCY: [std::sync::atomic::AtomicU64; LATENCY_BUCKETS] =
    [const { std::sync::atomic::AtomicU64::new(0) }; LATENCY_BUCKETS];
static LOCAL_LATENCY: [std::sync::atomic::AtomicU64; LATENCY_BUCKETS] =
    [const { std::sync::atomic::AtomicU64::new(0) }; LATENCY_BUCKETS];
static LOCAL_MAX_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static MAX_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[derive(serde::Serialize)]
pub struct HookStats {
    pub callbacks: u64,
    pub p99_us: u64,
    pub max_us: u64,
    /// Matching/enqueue work only, excluding downstream third-party hooks.
    pub own_p99_us: u64,
    pub own_max_us: u64,
}
pub fn stats() -> HookStats {
    use std::sync::atomic::Ordering::Relaxed;
    let counts = LATENCY.each_ref().map(|counter| counter.load(Relaxed));
    let local_counts = LOCAL_LATENCY.each_ref().map(|counter| counter.load(Relaxed));
    let callbacks: u64 = counts.iter().sum();
    let target = (callbacks * 99).div_ceil(100);
    let mut sum = 0;
    let p99_us = counts
        .iter()
        .position(|count| {
            sum += count;
            sum >= target
        })
        .unwrap_or(LATENCY_BUCKETS - 1) as u64;
    let mut local_sum = 0;
    let own_p99_us = local_counts
        .iter()
        .position(|count| {
            local_sum += count;
            local_sum >= target
        })
        .unwrap_or(LATENCY_BUCKETS - 1) as u64;
    HookStats {
        callbacks,
        p99_us,
        max_us: MAX_NS.load(Relaxed).div_ceil(1000),
        own_p99_us,
        own_max_us: LOCAL_MAX_NS.load(Relaxed).div_ceil(1000),
    }
}

fn compile(bindings: Vec<HookBinding>) -> Result<Vec<Binding>> {
    bindings
        .into_iter()
        .map(|binding| {
            let mut modifiers = 0;
            let mut key = None;
            for part in binding.chord.to_lowercase().split('+') {
                match part.trim() {
                    "alt" => modifiers |= 1,
                    "shift" => modifiers |= 2,
                    "ctrl" | "control" => modifiers |= 4,
                    "win" | "super" => modifiers |= 8,
                    value => {
                        let parsed = match value {
                            "tab" => VK_TAB.0 as u32,
                            "space" => VK_SPACE.0 as u32,
                            "enter" | "return" => VK_RETURN.0 as u32,
                            "esc" | "escape" => VK_ESCAPE.0 as u32,
                            "left" => VK_LEFT.0 as u32,
                            "right" => VK_RIGHT.0 as u32,
                            "up" => VK_UP.0 as u32,
                            "down" => VK_DOWN.0 as u32,
                            "backspace" => VK_BACK.0 as u32,
                            "delete" => VK_DELETE.0 as u32,
                            "home" => VK_HOME.0 as u32,
                            "end" => VK_END.0 as u32,
                            "pageup" => VK_PRIOR.0 as u32,
                            "pagedown" => VK_NEXT.0 as u32,
                            "-" | "minus" | "oem_minus" => VK_OEM_MINUS.0 as u32,
                            "=" | "plus" | "oem_plus" => VK_OEM_PLUS.0 as u32,
                            "[" => VK_OEM_4.0 as u32,
                            "]" => VK_OEM_6.0 as u32,
                            ";" => VK_OEM_1.0 as u32,
                            "/" | "oem_question" => VK_OEM_2.0 as u32,
                            value
                                if value.len() == 1
                                    && value.as_bytes()[0].is_ascii_alphanumeric() =>
                            {
                                value.as_bytes()[0].to_ascii_uppercase() as u32
                            }
                            value if value.starts_with('f') => value[1..]
                                .parse::<u32>()
                                .ok()
                                .filter(|n| (1..=24).contains(n))
                                .map(|n| VK_F1.0 as u32 + n - 1)
                                .unwrap_or(0),
                            _ => 0,
                        };
                        if parsed == 0 || key.replace(parsed).is_some() {
                            return Err(Error::new(
                                E_INVALIDARG,
                                format!("Invalid hotkey: {}", binding.chord),
                            ));
                        }
                    }
                }
            }
            Ok(Binding {
                key: key.ok_or_else(|| Error::new(E_INVALIDARG, "Hotkey requires a key"))?,
                modifiers,
                commands: binding.commands.into(),
            })
        })
        .collect()
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    struct Measure {
        start: std::time::Instant,
        local_recorded: bool,
    }
    impl Measure {
        fn record_local(&mut self) {
            use std::sync::atomic::Ordering::Relaxed;
            if !self.local_recorded {
                let ns = self.start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                LOCAL_LATENCY[(ns.div_ceil(1000) as usize).min(LATENCY_BUCKETS - 1)]
                    .fetch_add(1, Relaxed);
                LOCAL_MAX_NS.fetch_max(ns, Relaxed);
                self.local_recorded = true;
            }
        }
    }
    impl Drop for Measure {
        fn drop(&mut self) {
            use std::sync::atomic::Ordering::Relaxed;
            self.record_local();
            let ns = self.start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            LATENCY[(ns.div_ceil(1000) as usize).min(LATENCY_BUCKETS - 1)].fetch_add(1, Relaxed);
            MAX_NS.fetch_max(ns, Relaxed);
        }
    }
    let mut measure = Measure { start: std::time::Instant::now(), local_recorded: false };
    if code >= 0 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if info.dwExtraInfo != 0x4d4f4f4e
            && info.flags & LLKHF_LOWER_IL_INJECTED == KBDLLHOOKSTRUCT_FLAGS(0)
        {
            let consume = STATE.with(|cell| {
                let mut value = cell.borrow_mut();
                let Some(state) = value.as_mut() else {
                    return false;
                };
                if info.vkCode >= 256 {
                    return false;
                }
                let up = matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);
                let key = record_key(&mut state.pressed, info, up);
                if up {
                    return state.swallowed.remove(&key);
                }
                let modifiers = modifier_mask(&state.pressed);
                if let Some(binding) = state
                    .bindings
                    .iter()
                    .find(|binding| binding.key == key && binding.modifiers == modifiers)
                {
                    let sent = state
                        .sender
                        .send(PlatformEvent::Commands {
                            commands: binding.commands.clone(),
                            received: std::time::Instant::now(),
                        })
                        .is_ok();
                    if sent {
                        state.swallowed.insert(key);
                    }
                    sent
                } else {
                    false
                }
            });
            if consume {
                return LRESULT(1);
            }
        }
    }
    measure.record_local();
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_MOUSEMOVE {
        let input = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if input.flags & LLMHF_LOWER_IL_INJECTED == 0 {
            STATE.with(|state| {
                if let Some(state) = state.borrow().as_ref() {
                    let packed = ((input.pt.x as u32 as u64) << 32) | (input.pt.y as u32 as u64);
                    state.pointer.position.store(packed, Ordering::Release);
                    if !state.pointer.pending.swap(true, Ordering::AcqRel)
                        && state.sender.send(PlatformEvent::PointerMoved).is_err()
                    {
                        state.pointer.pending.store(false, Ordering::Release);
                    }
                }
            });
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn win_event(
    _: HWINEVENTHOOK,
    event: u32,
    handle: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if event == EVENT_SYSTEM_DESKTOPSWITCH {
        send_system(SystemEvent::DesktopChanged);
        return;
    }
    if handle.is_invalid() || (object != 0 && object != OBJID_WINDOW.0) || child != 0 {
        return;
    }
    STATE.with(|cell| {
        if let Some(state) = cell.borrow().as_ref() {
            let _ = state.sender.send(PlatformEvent::Window { kind: event, id: window_id(handle) });
        }
    });
}

fn send_system(event: SystemEvent) {
    STATE.with(|state| {
        if let Some(state) = state.borrow().as_ref() {
            let _ = state.sender.send(PlatformEvent::System(event));
        }
    });
}

unsafe extern "system" fn notifications(
    handle: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let taskbar = STATE.with(|state| {
        state
            .borrow()
            .as_ref()
            .is_some_and(|state| state.taskbar_message != 0 && state.taskbar_message == message)
    });
    let event = if taskbar {
        Some(SystemEvent::ShellRestarted)
    } else {
        match message {
            WM_DISPLAYCHANGE => Some(SystemEvent::DisplayChanged),
            WM_SETTINGCHANGE => Some(SystemEvent::SettingsChanged),
            WM_POWERBROADCAST => match wparam.0 as u32 {
                PBT_APMSUSPEND => Some(SystemEvent::Suspended),
                PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => Some(SystemEvent::Resumed),
                _ => None,
            },
            WM_WTSSESSION_CHANGE => match wparam.0 as u32 {
                WTS_SESSION_LOCK => Some(SystemEvent::SessionLocked),
                WTS_SESSION_UNLOCK => Some(SystemEvent::SessionUnlocked),
                WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT => Some(SystemEvent::Suspended),
                WTS_CONSOLE_CONNECT
                | WTS_REMOTE_CONNECT
                | WTS_SESSION_LOGON
                | SESSION_DESKTOP_READY => Some(SystemEvent::Resumed),
                WTS_SESSION_LOGOFF => Some(SystemEvent::SessionEnding),
                _ => None,
            },
            WM_ENDSESSION if wparam.0 != 0 => Some(SystemEvent::SessionEnding),
            _ => None,
        }
    };
    if let Some(event) = event {
        send_system(event);
        if matches!(event, SystemEvent::Resumed | SystemEvent::SessionUnlocked) {
            let _ =
                unsafe { PostThreadMessageW(GetCurrentThreadId(), RENEW, WPARAM(0), LPARAM(0)) };
        }
        return LRESULT(1);
    }
    unsafe { DefWindowProcW(handle, message, wparam, lparam) }
}

struct NotificationWindow {
    handle: HWND,
    session_registered: bool,
}
impl NotificationWindow {
    fn new() -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let name = wide("MoonSystemNotifications");
            let class = WNDCLASSW {
                lpfnWndProc: Some(notifications),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(name.as_ptr()),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(Error::from_thread());
            }
            let handle = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                PCWSTR(name.as_ptr()),
                PCWSTR(name.as_ptr()),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            // The WTS service can still be starting during logon. Keep window,
            // display and power intake live and retry session registration later.
            let session_registered =
                WTSRegisterSessionNotification(handle, NOTIFY_FOR_THIS_SESSION).is_ok();
            Ok(Self { handle, session_registered })
        }
    }
}
impl NotificationWindow {
    fn retry_session_registration(&mut self) {
        if !self.session_registered {
            self.session_registered =
                unsafe { WTSRegisterSessionNotification(self.handle, NOTIFY_FOR_THIS_SESSION) }
                    .is_ok();
        }
    }
}
impl Drop for NotificationWindow {
    fn drop(&mut self) {
        unsafe {
            if self.session_registered {
                let _ = WTSUnRegisterSessionNotification(self.handle);
            }
            let _ = DestroyWindow(self.handle);
        }
    }
}

pub struct Hooks {
    id: u32,
    pending: Arc<Mutex<Option<Vec<Binding>>>>,
    pointer: Arc<PointerState>,
    thread: Option<JoinHandle<()>>,
}
impl Hooks {
    pub fn start(
        bindings: Vec<HookBinding>,
        sender: Sender<PlatformEvent>,
        keyboard_enabled: bool,
    ) -> Result<Self> {
        let bindings = compile(bindings)?;
        let pending: Arc<Mutex<Option<Vec<Binding>>>> = Arc::new(Mutex::new(None));
        let changes = pending.clone();
        let pointer = Arc::new(PointerState::default());
        let pointer_thread = pointer.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("moon-intake".into())
            .spawn(move || unsafe {
                let id = GetCurrentThreadId();
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                STATE.with(|state| {
                    *state.borrow_mut() = Some(State {
                        bindings,
                        sender: sender.clone(),
                        pressed: modifier_snapshot(|key| GetAsyncKeyState(key.0 as i32) < 0),
                        swallowed: HashSet::with_capacity(16),
                        pointer: pointer_thread,
                        taskbar_message: RegisterWindowMessageW(PCWSTR(
                            wide("TaskbarCreated").as_ptr(),
                        )),
                    })
                });
                let mut notifications = match NotificationWindow::new() {
                    Ok(window) => window,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                let mut event_hooks = Vec::new();
                for (first, last) in [
                    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
                    (EVENT_SYSTEM_DESKTOPSWITCH, EVENT_SYSTEM_DESKTOPSWITCH),
                    (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
                    (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
                    (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
                    (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE),
                    (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
                ] {
                    let hook = SetWinEventHook(
                        first,
                        last,
                        None,
                        Some(win_event),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                    );
                    if hook.is_invalid() {
                        let _ = ready_tx.send(Err(Error::from_thread()));
                        return;
                    }
                    event_hooks.push(hook);
                }
                let mut hook = if keyboard_enabled {
                    match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0) {
                        Ok(value) => Some(value),
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                            return;
                        }
                    }
                } else {
                    None
                };
                if let Err(error) = RegisterHotKey(
                    None,
                    EMERGENCY,
                    MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                    VK_F12.0 as u32,
                ) {
                    for hook in event_hooks {
                        let _ = UnhookWinEvent(hook);
                    }
                    if let Some(hook) = hook {
                        let _ = UnhookWindowsHookEx(hook);
                    }
                    let _ = ready_tx.send(Err(error));
                    return;
                }
                // Windows exposes no hook-is-installed API. Renew it periodically so a
                // timeout or secure-desktop transition cannot disable bindings forever.
                let timer = SetTimer(None, 0, 30_000, None);
                let mut mouse_enabled = false;
                let mut mouse_hook = None;
                let _ = ready_tx.send(Ok(id));
                while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                    if message.message == UPDATE {
                        if let Some(bindings) =
                            changes.lock().unwrap_or_else(|e| e.into_inner()).take()
                        {
                            STATE.with(|state| {
                                if let Some(state) = state.borrow_mut().as_mut() {
                                    state.bindings = bindings;
                                }
                            });
                        }
                    } else if message.message == POINTER {
                        mouse_enabled = message.wParam.0 != 0;
                        if let Some(old) = mouse_hook.take() {
                            let _ = UnhookWindowsHookEx(old);
                        }
                        if mouse_enabled {
                            match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0) {
                                Ok(hook) => mouse_hook = Some(hook),
                                Err(error) => {
                                    let _ =
                                        sender.send(PlatformEvent::HookError(error.to_string()));
                                }
                            }
                        }
                    } else if message.message == WM_HOTKEY && message.wParam.0 == EMERGENCY as usize
                    {
                        let _ = sender.send(PlatformEvent::EmergencyRestore);
                    } else if matches!(message.message, WM_TIMER | RENEW) {
                        notifications.retry_session_registration();
                        if mouse_enabled {
                            if let Some(old) = mouse_hook.take() {
                                let _ = UnhookWindowsHookEx(old);
                            }
                            match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0) {
                                Ok(hook) => mouse_hook = Some(hook),
                                Err(error) => {
                                    let _ =
                                        sender.send(PlatformEvent::HookError(error.to_string()));
                                }
                            }
                        }
                        if !keyboard_enabled {
                            continue;
                        }
                        if let Some(old) = hook.take() {
                            let _ = UnhookWindowsHookEx(old);
                        }
                        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0) {
                            Ok(new) => hook = Some(new),
                            Err(error) => {
                                let _ = sender.send(PlatformEvent::HookError(error.to_string()));
                            }
                        }
                        STATE.with(|state| {
                            if let Some(state) = state.borrow_mut().as_mut() {
                                state.pressed =
                                    modifier_snapshot(|key| GetAsyncKeyState(key.0 as i32) < 0);
                            }
                        });
                        #[cfg(test)]
                        TEST_RENEWALS.fetch_add(1, Ordering::Release);
                    } else {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                if let Some(hook) = mouse_hook {
                    let _ = UnhookWindowsHookEx(hook);
                }
                let _ = UnregisterHotKey(None, EMERGENCY);
                if timer != 0 {
                    let _ = KillTimer(None, timer);
                }
                drop(notifications);
                if let Some(hook) = hook {
                    let _ = UnhookWindowsHookEx(hook);
                }
                for hook in event_hooks {
                    let _ = UnhookWinEvent(hook);
                }
                STATE.with(|state| *state.borrow_mut() = None);
            })
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        let id = ready_rx
            .recv()
            .map_err(|_| Error::new(E_FAIL, "Input thread stopped before initialization"))??;
        Ok(Self { id, pending, pointer, thread: Some(thread) })
    }
    pub fn set_pointer_tracking(&self, enabled: bool) -> Result<()> {
        unsafe { PostThreadMessageW(self.id, POINTER, WPARAM(usize::from(enabled)), LPARAM(0)) }
    }
    pub fn take_pointer(&self) -> moon_core::geometry::Point {
        self.pointer.pending.store(false, Ordering::Release);
        let value = self.pointer.position.load(Ordering::Acquire);
        moon_core::geometry::Point::new((value >> 32) as u32 as i32, value as u32 as i32)
    }
    pub fn update_bindings(&self, bindings: Vec<HookBinding>) -> Result<()> {
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some(compile(bindings)?);
        unsafe { PostThreadMessageW(self.id, UPDATE, WPARAM(0), LPARAM(0)) }
    }
}
impl Drop for Hooks {
    fn drop(&mut self) {
        let _ = unsafe { PostThreadMessageW(self.id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Called by the main thread before executing an Alt binding. This prevents
/// Windows from treating a consumed Alt chord as a bare menu activation.
pub fn suppress_alt_menu() {
    unsafe {
        if GetAsyncKeyState(VK_MENU.0 as i32) < 0 {
            let mut down = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_F24, ..Default::default() } },
            };
            down.Anonymous.ki.dwExtraInfo = 0x4d4f4f4e;
            let mut up = down;
            up.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
            SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an isolated interactive desktop and MOON_TESTWIN_PATH"]
    fn held_alt_shift_renewal_preserves_hkl_and_plain_trigger()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::platform;
        use std::time::{Duration, Instant};
        fn inject(keys: &[(VIRTUAL_KEY, bool)]) -> bool {
            let inputs: Vec<_> = keys
                .iter()
                .map(|&(key, up)| INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: key,
                            dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                            ..Default::default()
                        },
                    },
                })
                .collect();
            unsafe {
                SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) == inputs.len() as u32
            }
        }
        struct Fixture {
            child: std::process::Child,
            foreground: Option<WindowId>,
            helper: Option<(WindowId, HKL)>,
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                inject(&[(VK_LSHIFT, true), (VK_LMENU, true)]);
                if let Some((id, layout)) = self.helper {
                    unsafe {
                        let _ = PostMessageW(
                            Some(platform::hwnd(id)),
                            WM_INPUTLANGCHANGEREQUEST,
                            WPARAM(0),
                            LPARAM(layout.0 as isize),
                        );
                    }
                    thread::sleep(Duration::from_millis(30));
                }
                let _ = self.child.kill();
                let _ = self.child.wait();
                if let Some(id) = self.foreground.filter(|id| platform::is_window(*id)) {
                    let _ = platform::focus(id);
                }
            }
        }
        if platform::running_processes()?.iter().any(|(_, name)| {
            name.eq_ignore_ascii_case("moon.exe") || name.eq_ignore_ascii_case("testwin.exe")
        }) {
            return Err("stop existing moon/testwin processes before this interactive test".into());
        }
        if modifier_mask(&modifier_snapshot(|key| unsafe { GetAsyncKeyState(key.0 as i32) < 0 }))
            != 0
        {
            return Err("physical modifiers must be released before this test".into());
        }
        let _apartment = platform::Apartment::new()?;
        let foreground = platform::foreground();
        let mut fixture = Fixture {
            child: std::process::Command::new(std::env::var("MOON_TESTWIN_PATH")?)
                .arg("1")
                .spawn()?,
            foreground,
            helper: None,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        let helper = loop {
            if let Some(window) = platform::enumerate_windows()?
                .into_iter()
                .find(|w| w.process_id == fixture.child.id() && w.class == "MoonTestWindow")
            {
                break window.id;
            }
            if Instant::now() >= deadline {
                return Err("owned helper did not appear".into());
            }
            thread::sleep(Duration::from_millis(20));
        };
        platform::focus(helper)?;
        let helper_thread = unsafe { GetWindowThreadProcessId(platform::hwnd(helper), None) };
        let initial_layout = unsafe { GetKeyboardLayout(helper_thread) };
        fixture.helper = Some((helper, initial_layout));
        let available_layouts = unsafe { GetKeyboardLayoutList(None) };
        assert!(inject(&[(VK_LMENU, false), (VK_LSHIFT, false)]));
        let (sender, receiver) = mpsc::channel();
        let hook = Hooks::start(
            vec![
                HookBinding {
                    chord: "alt+shift+oem_minus".into(),
                    commands: vec!["modified".into()],
                },
                HookBinding { chord: "oem_minus".into(), commands: vec!["plain".into()] },
            ],
            sender,
            true,
        )?;
        let expect = |expected: &str| -> std::result::Result<(), Box<dyn std::error::Error>> {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if Instant::now() >= deadline {
                    return Err(format!("expected {expected} trigger was not received").into());
                }
                if let Ok(PlatformEvent::Commands { commands, .. }) =
                    receiver.recv_timeout(Duration::from_millis(10))
                {
                    if commands.first().is_none_or(|actual| actual != expected) {
                        return Err(format!(
                            "modifier mismatch: expected {expected}, received {commands:?}"
                        )
                        .into());
                    }
                    return Ok(());
                }
            }
        };
        for trial in 0..11 {
            if trial > 0 {
                assert!(inject(&[(VK_LMENU, false), (VK_LSHIFT, false)]));
                let before = TEST_RENEWALS.load(Ordering::Acquire);
                unsafe {
                    PostThreadMessageW(hook.id, RENEW, WPARAM(0), LPARAM(0))?;
                }
                let deadline = Instant::now() + Duration::from_secs(2);
                while TEST_RENEWALS.load(Ordering::Acquire) == before {
                    if Instant::now() >= deadline {
                        return Err("hook renewal did not complete".into());
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
            if platform::foreground() != Some(helper) {
                return Err("foreground left the owned helper".into());
            }
            assert!(inject(&[(VK_OEM_MINUS, false), (VK_OEM_MINUS, true)]));
            expect("modified")?;
            suppress_alt_menu();
            assert!(inject(&[(VK_LSHIFT, true), (VK_LMENU, true)]));
            thread::sleep(Duration::from_millis(40));
            assert_eq!(
                unsafe { GetKeyboardLayout(helper_thread) },
                initial_layout,
                "Alt+Shift changed HKL in trial {trial}"
            );
            assert!(inject(&[(VK_OEM_MINUS, false), (VK_OEM_MINUS, true)]));
            expect("plain")?;
        }
        println!(
            "initially held modifiers +10 renewals:22/22 triggers; HKL={:x}, available_layouts={available_layouts}",
            initial_layout.0 as usize
        );
        drop(hook);
        Ok(())
    }

    #[test]
    fn renewed_modifiers_clear_on_generic_and_sided_release() {
        // Documented hardware scan codes, independent of the active layout's
        // reverse mapping (Korean layouts have no VK_RCONTROL/VK_RMENU mapping).
        for (generic, side, expected_mask, scan) in [
            (VK_SHIFT, VK_LSHIFT, 2, 0x2a),
            (VK_SHIFT, VK_RSHIFT, 2, 0x36),
            (VK_CONTROL, VK_LCONTROL, 4, 0x1d),
            (VK_CONTROL, VK_RCONTROL, 4, 0xe01d),
            (VK_MENU, VK_LMENU, 1, 0x38),
            (VK_MENU, VK_RMENU, 1, 0xe038),
        ] {
            for event_key in [generic, side] {
                // Windows reports both the generic and the actual side as held
                // during renewal. Only the side may survive in our snapshot.
                let mut pressed = modifier_snapshot(|key| key == generic || key == side);
                assert_eq!(modifier_mask(&pressed), expected_mask);
                assert!(!pressed[generic.0 as usize]);
                let event = KBDLLHOOKSTRUCT {
                    vkCode: u32::from(event_key.0),
                    scanCode: scan & 0xff,
                    flags: if scan & 0xff00 != 0 {
                        LLKHF_EXTENDED
                    } else {
                        KBDLLHOOKSTRUCT_FLAGS(0)
                    },
                    ..Default::default()
                };
                assert_eq!(record_key(&mut pressed, &event, true), u32::from(side.0));
                assert_eq!(modifier_mask(&pressed), 0);
            }
        }
    }

    #[test]
    fn releasing_one_modifier_side_preserves_the_other_side() {
        let mut pressed = modifier_snapshot(|key| key == VK_LMENU || key == VK_RMENU);
        for (side, expected_mask) in [(VK_LMENU, 1), (VK_RMENU, 0)] {
            record_key(
                &mut pressed,
                &KBDLLHOOKSTRUCT { vkCode: u32::from(side.0), ..Default::default() },
                true,
            );
            assert_eq!(modifier_mask(&pressed), expected_mask);
        }
    }

    #[test]
    fn every_built_in_binding_has_a_native_key_mapping() {
        let config = moon_core::config::Config::default();
        let count = config.bindings.len();
        let bindings = compile(
            config
                .bindings
                .into_iter()
                .map(|(chord, commands)| HookBinding { chord, commands })
                .collect(),
        )
        .unwrap();
        assert_eq!(bindings.len(), count);
        assert!(
            bindings
                .iter()
                .any(|binding| binding.key == VK_OEM_2.0 as u32 && binding.modifiers == 1)
        );
    }
    #[test]
    fn malformed_keybindings_are_rejected_before_installing_a_hook() {
        for chord in ["alt", "alt+banana", "alt+h+j", "ctrl+f25"] {
            assert!(
                compile(vec![HookBinding { chord: chord.into(), commands: vec!["close".into()] }])
                    .is_err()
            );
        }
    }
}
