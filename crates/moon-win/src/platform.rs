//! Checked Win32 effects. Coordinates describe the visible DWM frame in pixels.

use moon_core::{
    geometry::Rect,
    ids::{MonitorId, WindowId},
    world::{Monitor, WindowInfo},
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    marker::PhantomData,
    mem::{size_of, size_of_val},
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::{Com::*, RemoteDesktop::*, StationsAndDesktops::*, Threading::*},
        UI::{
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            Shell::{IVirtualDesktopManager, VirtualDesktopManager},
            WindowsAndMessaging::*,
        },
    },
    core::{GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR, Result},
};

pub fn hwnd(id: WindowId) -> HWND {
    HWND(id.0 as usize as *mut c_void)
}
pub fn window_id(handle: HWND) -> WindowId {
    WindowId(handle.0 as usize as u64)
}
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn rect(value: RECT) -> Rect {
    Rect::from_edges(value.left, value.top, value.right, value.bottom)
}

thread_local! {
    static APARTMENT_DEPTH: Cell<usize> = const { Cell::new(0) };
    static DESKTOP_MANAGER: RefCell<Option<IVirtualDesktopManager>> = const { RefCell::new(None) };
}

/// A balanced STA initialization that cannot move to another thread.
pub struct Apartment {
    _thread: PhantomData<Rc<()>>,
}
impl Apartment {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        APARTMENT_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Ok(Self { _thread: PhantomData })
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        let final_guard = APARTMENT_DEPTH.with(|depth| {
            let remaining = depth.get() - 1;
            depth.set(remaining);
            remaining == 0
        });
        if final_guard {
            // Release the proxy while COM is still initialized on its owner thread.
            invalidate_virtual_desktop_manager();
        }
        unsafe {
            CoUninitialize();
        }
    }
}

pub fn enable_dpi_awareness() -> Result<()> {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SessionState {
    pub session_id: u32,
    pub locked: bool,
    /// A logged-on user is actively connected (WTSActive), not merely reconnecting.
    pub connected: bool,
}

/// Read the current logon session, including startup before lock notifications arrive.
pub fn session_state() -> Result<SessionState> {
    let mut buffer = PWSTR::null();
    let mut bytes = 0;
    unsafe {
        WTSQuerySessionInformationW(
            None,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut bytes,
        )?;
    }
    let result = (|| {
        if buffer.is_null() || bytes < size_of::<WTSINFOEXW>() as u32 {
            return Err(windows::core::Error::new(
                E_UNEXPECTED,
                "Incomplete WTS session information",
            ));
        }
        let info = unsafe { buffer.0.cast::<WTSINFOEXW>().read_unaligned() };
        if info.Level != 1 {
            return Err(windows::core::Error::new(
                E_UNEXPECTED,
                "Unsupported WTS session information level",
            ));
        }
        let state = unsafe { info.Data.WTSInfoExLevel1 };
        // Only Windows 7/Server 2008 R2 reversed these documented flag meanings.
        let locked = match state.SessionFlags as u32 {
            WTS_SESSIONSTATE_LOCK => true,
            WTS_SESSIONSTATE_UNLOCK => false,
            _ => {
                return Err(windows::core::Error::new(
                    E_UNEXPECTED,
                    "Windows session lock state is unknown",
                ));
            }
        };
        Ok(SessionState {
            session_id: state.SessionId,
            locked,
            connected: state.SessionState == WTSActive,
        })
    })();
    if !buffer.is_null() {
        unsafe { WTSFreeMemory(buffer.0.cast()) };
    }
    result
}

pub fn session_locked() -> Result<bool> {
    session_state().map(|state| state.locked)
}

fn desktop_name(desktop: HDESK) -> Result<String> {
    let mut name = [0u16; 256];
    let mut bytes = 0;
    unsafe {
        GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            size_of_val(&name) as u32,
            Some(&mut bytes),
        )?;
    }
    if bytes < 2 || bytes as usize > size_of_val(&name) || !bytes.is_multiple_of(2) {
        return Err(windows::core::Error::new(E_UNEXPECTED, "Invalid desktop name information"));
    }
    let units = bytes as usize / 2;
    if name[units - 1] != 0 {
        return Err(windows::core::Error::new(E_UNEXPECTED, "Desktop name is not terminated"));
    }
    Ok(String::from_utf16_lossy(&name[..units - 1]))
}

/// True only when this thread's ordinary desktop is the desktop receiving input.
/// Access denied on the secure desktop is an expected unavailable result. This
/// does not establish RDP connectivity: OpenInputDesktop also works disconnected.
pub fn input_desktop_available() -> Result<bool> {
    let input =
        match unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) } {
            Ok(desktop) => desktop,
            Err(error) if error.code() == E_ACCESSDENIED => return Ok(false),
            Err(error) => return Err(error),
        };
    let result: Result<bool> = (|| {
        let input_name = desktop_name(input)?;
        if !input_name.eq_ignore_ascii_case("Default") {
            return Ok(false);
        }
        // GetThreadDesktop returns a borrowed handle; only the opened input
        // desktop belongs to this function and must be closed.
        let own = unsafe { GetThreadDesktop(GetCurrentThreadId())? };
        Ok(desktop_name(own)?.eq_ignore_ascii_case(&input_name))
    })();
    let closed = unsafe { CloseDesktop(input) };
    let available = result?;
    closed?;
    Ok(available)
}

/// Call at startup and on session/desktop transitions; errors mean unavailable.
pub fn session_interactive() -> Result<bool> {
    let state = session_state()?;
    if state.locked || !state.connected {
        return Ok(false);
    }
    input_desktop_available()
}

fn monitor_id(name: &[u16]) -> MonitorId {
    MonitorId(
        name.iter()
            .take_while(|&&c| c != 0)
            .fold(2166136261u32, |h, c| (h ^ u32::from(*c)).wrapping_mul(16777619)),
    )
}

pub fn monitors() -> Result<Vec<Monitor>> {
    unsafe extern "system" fn callback(
        handle: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> windows::core::BOOL {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(handle, &mut info.monitorInfo) }.as_bool() {
            let mut dpi_x = 96;
            let mut dpi_y = 96;
            let _ = unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
            let name = String::from_utf16_lossy(
                &info.szDevice
                    [..info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len())],
            );
            unsafe { &mut *(data.0 as *mut Vec<Monitor>) }.push(Monitor {
                id: monitor_id(&info.szDevice),
                name,
                bounds: rect(info.monitorInfo.rcMonitor),
                work_area: rect(info.monitorInfo.rcWork),
                scale: f64::from(dpi_x) / 96.0,
            });
        }
        true.into()
    }
    let mut result: Vec<Monitor> = Vec::new();
    unsafe {
        EnumDisplayMonitors(None, None, Some(callback), LPARAM(&mut result as *mut _ as isize))
            .ok()?;
    }
    result.sort_by_key(|m| (m.bounds.x, m.bounds.y));
    Ok(result)
}

pub fn process_name(pid: u32) -> Result<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(handle);
        result?;
        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        Ok(path.rsplit(['\\', '/']).next().unwrap_or(&path).trim_end_matches(".exe").to_owned())
    }
}

pub fn process_id(id: WindowId) -> u32 {
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd(id), Some(&mut pid));
    }
    pid
}

pub fn window_info(id: WindowId) -> Result<WindowInfo> {
    unsafe {
        let handle = hwnd(id);
        let mut pid = 0;
        GetWindowThreadProcessId(handle, Some(&mut pid));
        let mut title = vec![0u16; 4096];
        let len = GetWindowTextW(handle, &mut title);
        let mut class = vec![0u16; 512];
        let class_len = GetClassNameW(handle, &mut class);
        let monitor = MonitorFromWindow(handle, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        GetMonitorInfoW(monitor, &mut info.monitorInfo).ok()?;
        Ok(WindowInfo {
            id,
            process_id: pid,
            process: process_name(pid)?,
            class: String::from_utf16_lossy(&class[..class_len.max(0) as usize]),
            title: String::from_utf16_lossy(&title[..len.max(0) as usize]),
            rect: window_rect(id)?,
            monitor: monitor_id(&info.szDevice),
        })
    }
}

pub fn is_manageable(id: WindowId) -> bool {
    unsafe {
        let handle = hwnd(id);
        if !IsWindow(Some(handle)).as_bool()
            || !IsWindowVisible(handle).as_bool()
            || IsIconic(handle).as_bool()
        {
            return false;
        }
        if process_id(id) == GetCurrentProcessId() {
            return false;
        }
        let style = GetWindowLongPtrW(handle, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(handle, GWL_EXSTYLE) as u32;
        if style & WS_CHILD.0 != 0 || ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0 {
            return false;
        }
        let mut class = [0u16; 256];
        let count = GetClassNameW(handle, &mut class);
        let class = String::from_utf16_lossy(&class[..count.max(0) as usize]);
        // Standard owned dialogs have explicit built-in float rules. Other
        // owned/tool windows stay with their application's normal behavior.
        if GetWindow(handle, GW_OWNER).is_ok_and(|owner| !owner.is_invalid())
            && ex & WS_EX_APPWINDOW.0 == 0
            && !matches!(class.as_str(), "#32770" | "SunAwtDialog")
        {
            return false;
        }
        if matches!(
            class.as_str(),
            "Progman"
                | "WorkerW"
                | "Shell_TrayWnd"
                | "Shell_SecondaryTrayWnd"
                | "Windows.UI.Core.CoreWindow"
                | "XamlExplorerHostIslandWindow"
        ) {
            return false;
        }
        window_rect(id).is_ok_and(|r| r.width > 0 && r.height > 0)
    }
}

pub fn enumerate_windows() -> Result<Vec<WindowInfo>> {
    unsafe extern "system" fn callback(handle: HWND, data: LPARAM) -> windows::core::BOOL {
        let id = window_id(handle);
        if is_manageable(id)
            && let Ok(info) = window_info(id)
        {
            unsafe { &mut *(data.0 as *mut Vec<WindowInfo>) }.push(info);
        }
        true.into()
    }
    let mut result = Vec::new();
    unsafe {
        EnumWindows(Some(callback), LPARAM(&mut result as *mut _ as isize))?;
    }
    Ok(result)
}

pub fn window_rect(id: WindowId) -> Result<Rect> {
    unsafe {
        let mut frame = RECT::default();
        if DwmGetWindowAttribute(
            hwnd(id),
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut _ as *mut c_void,
            size_of::<RECT>() as u32,
        )
        .is_err()
        {
            GetWindowRect(hwnd(id), &mut frame)?;
        }
        Ok(rect(frame))
    }
}

pub fn place(id: WindowId, target: Rect) -> Result<()> {
    unsafe {
        let handle = hwnd(id);
        if IsZoomed(handle).as_bool() {
            let _ = ShowWindowAsync(handle, SW_RESTORE);
        }
        let frame = window_rect(id)?;
        let mut outer = RECT::default();
        GetWindowRect(handle, &mut outer)?;
        let left = frame.x - outer.left;
        let top = frame.y - outer.top;
        let right = outer.right - frame.right();
        let bottom = outer.bottom - frame.bottom();
        SetWindowPos(
            handle,
            None,
            target.x - left,
            target.y - top,
            target.width + left + right,
            target.height + top + bottom,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
        )
    }
}

pub fn foreground() -> Option<WindowId> {
    let h = unsafe { GetForegroundWindow() };
    (!h.is_invalid()).then(|| window_id(h))
}

pub fn cursor_position() -> Result<moon_core::geometry::Point> {
    let mut position = POINT::default();
    unsafe {
        GetCursorPos(&mut position)?;
    }
    Ok(moon_core::geometry::Point::new(position.x, position.y))
}

pub fn cursor_jump(position: moon_core::geometry::Point) -> Result<()> {
    unsafe { SetCursorPos(position.x, position.y) }
}

pub fn window_at_point(position: moon_core::geometry::Point) -> Option<WindowId> {
    unsafe {
        let child = WindowFromPoint(POINT { x: position.x, y: position.y });
        if child.is_invalid() {
            return None;
        }
        let root = GetAncestor(child, GA_ROOT);
        (!root.is_invalid()).then(|| window_id(root))
    }
}

pub fn native_fullscreen() -> Result<Option<(WindowId, MonitorId)>> {
    let Some(id) = foreground() else {
        return Ok(None);
    };
    unsafe {
        let handle = hwnd(id);
        // Explorer can foreground its new desktop during a shell restart. Its
        // borderless monitor-sized surface is not a fullscreen application.
        if handle == GetShellWindow() || handle == GetDesktopWindow() {
            return Ok(None);
        }
        if GetWindowLongPtrW(handle, GWL_STYLE) as u32 & WS_CAPTION.0 != 0
            || IsIconic(handle).as_bool()
        {
            return Ok(None);
        }
        let frame = window_rect(id)?;
        let monitor = MonitorFromWindow(handle, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        GetMonitorInfoW(monitor, &mut info.monitorInfo).ok()?;
        let bounds = rect(info.monitorInfo.rcMonitor);
        let matches = (frame.x - bounds.x).abs() <= 1
            && (frame.y - bounds.y).abs() <= 1
            && (frame.right() - bounds.right()).abs() <= 1
            && (frame.bottom() - bounds.bottom()).abs() <= 1;
        Ok(matches.then_some((id, monitor_id(&info.szDevice))))
    }
}

pub fn window_on_current_virtual_desktop(id: WindowId) -> Result<bool> {
    let manager = virtual_desktop_manager()?;
    match unsafe { manager.IsWindowOnCurrentVirtualDesktop(hwnd(id)) } {
        Ok(current) => Ok(current.as_bool()),
        Err(error) if disconnected_shell(error.code()) => {
            drop(manager);
            invalidate_virtual_desktop_manager();
            // Explorer can replace the shell endpoint. Rebind once; ordinary
            // invalid-window errors do not discard a healthy cached interface.
            let manager = virtual_desktop_manager()?;
            unsafe {
                manager.IsWindowOnCurrentVirtualDesktop(hwnd(id)).map(|current| current.as_bool())
            }
        }
        Err(error) => Err(error),
    }
}

/// Call on the daemon's apartment after Explorer's TaskbarCreated notification.
pub fn invalidate_virtual_desktop_manager() {
    // Drop outside the RefCell borrow: releasing a COM proxy can dispatch messages.
    let previous = DESKTOP_MANAGER.with(RefCell::take);
    drop(previous);
}

fn virtual_desktop_manager() -> Result<IVirtualDesktopManager> {
    let cache = APARTMENT_DEPTH.with(|depth| depth.get() != 0);
    if cache && let Some(manager) = DESKTOP_MANAGER.with(|slot| slot.borrow().clone()) {
        return Ok(manager);
    }
    let manager: IVirtualDesktopManager =
        unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_INPROC_SERVER)? };
    if cache {
        let previous = DESKTOP_MANAGER.with(|slot| slot.replace(Some(manager.clone())));
        drop(previous);
    }
    // External COM initializers retain the previous, uncached behavior because
    // this module cannot know when their apartment will be uninitialized.
    Ok(manager)
}

fn disconnected_shell(code: HRESULT) -> bool {
    matches!(
        code,
        RPC_E_DISCONNECTED
            | RPC_E_SERVER_DIED
            | RPC_E_SERVER_DIED_DNE
            | CO_E_OBJNOTCONNECTED
            | CO_E_SERVER_STOPPING
    ) || code == HRESULT(0x800706ba_u32 as i32) // HRESULT_FROM_WIN32(RPC_S_SERVER_UNAVAILABLE)
}

#[cfg(test)]
mod apartment_tests {
    use super::*;

    #[test]
    fn desktop_manager_reuses_its_apartment_and_drops_with_the_final_guard() {
        let outer = Apartment::new().unwrap();
        let first = virtual_desktop_manager().unwrap();
        let address = Interface::as_raw(&first);
        drop(first);
        let inner = Apartment::new().unwrap();
        let second = virtual_desktop_manager().unwrap();
        assert_eq!(Interface::as_raw(&second), address);
        drop(second);
        drop(inner);
        assert!(DESKTOP_MANAGER.with(|slot| slot.borrow().is_some()));
        drop(outer);
        assert!(DESKTOP_MANAGER.with(|slot| slot.borrow().is_none()));

        let restarted = Apartment::new().unwrap();
        drop(virtual_desktop_manager().unwrap());
        invalidate_virtual_desktop_manager();
        assert!(DESKTOP_MANAGER.with(|slot| slot.borrow().is_none()));
        drop(virtual_desktop_manager().unwrap());
        drop(restarted);
        assert!(DESKTOP_MANAGER.with(|slot| slot.borrow().is_none()));
    }
}

fn await_foreground(handle: HWND) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(10);
    loop {
        if unsafe { GetForegroundWindow() } == handle {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::yield_now();
    }
}

pub fn focus(id: WindowId) -> Result<()> {
    unsafe {
        let handle = hwnd(id);
        if IsIconic(handle).as_bool() {
            let _ = ShowWindowAsync(handle, SW_RESTORE);
        }
        if (SetForegroundWindow(handle).as_bool() && await_foreground(handle))
            || GetForegroundWindow() == handle
        {
            return Ok(());
        }
        // A zero-motion input establishes a foreground interaction without typing.
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT { dwFlags: MOUSEEVENTF_MOVE, ..Default::default() },
            },
        };
        SendInput(&[input], size_of::<INPUT>() as i32);
        if (SetForegroundWindow(handle).as_bool() && await_foreground(handle))
            || GetForegroundWindow() == handle
        {
            return Ok(());
        }
        let current = GetCurrentThreadId();
        let target = GetWindowThreadProcessId(GetForegroundWindow(), None);
        if target != 0
            && target != current
            && !IsHungAppWindow(GetForegroundWindow()).as_bool()
            && AttachThreadInput(current, target, true).as_bool()
        {
            let applied = SetForegroundWindow(handle).as_bool();
            let _ = AttachThreadInput(current, target, false);
            if (applied && await_foreground(handle)) || GetForegroundWindow() == handle {
                return Ok(());
            }
        }
        Err(windows::core::Error::new(E_ACCESSDENIED, "Windows refused the foreground change"))
    }
}

pub fn close(id: WindowId) -> Result<()> {
    unsafe { PostMessageW(Some(hwnd(id)), WM_CLOSE, WPARAM(0), LPARAM(0)) }
}
pub fn is_window(id: WindowId) -> bool {
    unsafe { IsWindow(Some(hwnd(id))).as_bool() }
}

pub fn is_minimized(id: WindowId) -> bool {
    unsafe { IsIconic(hwnd(id)).as_bool() }
}

/// Apps can hide their top-level window or cloak it while they finish closing.
/// Moon's workspace cloak is set by the shell and leaves WS_VISIBLE intact.
pub fn is_hidden_by_app(id: WindowId) -> bool {
    let visible = unsafe { IsWindowVisible(hwnd(id)).as_bool() };
    !visible || cloaked(id).is_ok_and(|flags| flags & DWM_CLOAKED_APP != 0)
}

pub fn cloaked(id: WindowId) -> Result<u32> {
    let mut value = 0u32;
    unsafe {
        DwmGetWindowAttribute(hwnd(id), DWMWA_CLOAKED, &mut value as *mut _ as *mut c_void, 4)?;
    }
    Ok(value)
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    use windows::core::{BOOL, w};

    #[test]
    fn detects_hide_before_destroy_and_recovers_on_show_or_uncloak() -> Result<()> {
        unsafe {
            let handle = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("Moon visibility test"),
                WS_POPUP,
                -32000,
                -32000,
                32,
                32,
                None,
                None,
                None,
                None,
            )?;
            let id = window_id(handle);
            let _ = ShowWindow(handle, SW_SHOWNOACTIVATE);
            assert!(!is_hidden_by_app(id));
            let _ = ShowWindow(handle, SW_HIDE);
            assert!(is_window(id) && is_hidden_by_app(id));
            let _ = ShowWindow(handle, SW_SHOWNOACTIVATE);
            assert!(!is_hidden_by_app(id));
            for hidden in [true, false] {
                let cloak = BOOL::from(hidden);
                DwmSetWindowAttribute(
                    handle,
                    DWMWA_CLOAK,
                    (&cloak as *const BOOL).cast(),
                    size_of::<BOOL>() as u32,
                )?;
                DwmFlush()?;
                assert_eq!(is_hidden_by_app(id), hidden);
            }
            DestroyWindow(handle)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HideMethod {
    ShellCloak,
    Parked,
}

// Undocumented shell ABI is isolated here. QueryService requests the exact interface,
// and every effect is verified using the documented DWM attribute before use.
fn shell_cloak(id: WindowId, hidden: bool) -> Result<()> {
    const SHELL: GUID = GUID::from_u128(0xc2f03a33_21f5_47fa_b4bb_156362a2f239);
    const COLLECTION: GUID = GUID::from_u128(0x1841c6d7_4f9d_42c0_af41_8747538f10e5);
    unsafe {
        let service: IServiceProvider = CoCreateInstance(&SHELL, None, CLSCTX_LOCAL_SERVER)?;
        let mut collection = std::ptr::null_mut();
        (Interface::vtable(&service).QueryService)(
            Interface::as_raw(&service),
            &COLLECTION,
            &COLLECTION,
            &mut collection,
        )
        .ok()?;
        if collection.is_null() {
            return Err(windows::core::Error::new(E_POINTER, "Shell returned no collection"));
        }
        let collection = IUnknown::from_raw(collection);
        let table = *(Interface::as_raw(&collection) as *const *const *const c_void);
        let get_view: unsafe extern "system" fn(*mut c_void, HWND, *mut *mut c_void) -> HRESULT =
            std::mem::transmute(*table.add(6));
        let mut view = std::ptr::null_mut();
        get_view(Interface::as_raw(&collection), hwnd(id), &mut view).ok()?;
        if view.is_null() {
            return Err(windows::core::Error::new(E_POINTER, "Shell returned no view"));
        }
        let view = IUnknown::from_raw(view);
        let table = *(Interface::as_raw(&view) as *const *const *const c_void);
        let set_cloak: unsafe extern "system" fn(*mut c_void, u32, i32) -> HRESULT =
            std::mem::transmute(*table.add(12));
        set_cloak(Interface::as_raw(&view), 1, if hidden { 2 } else { 0 }).ok()?;
        for _ in 0..10 {
            if (cloaked(id)? != 0) == hidden {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Err(windows::core::Error::new(E_FAIL, "Shell cloak did not reach its requested state"))
    }
}

pub fn set_hidden(id: WindowId, hidden: bool) -> Result<HideMethod> {
    if shell_cloak(id, hidden).is_ok() {
        return Ok(HideMethod::ShellCloak);
    }
    if hidden {
        let current = window_rect(id)?;
        place(id, Rect::new(-32000, -32000, current.width, current.height))?;
    } else if cloaked(id).is_ok_and(|state| state != 0) {
        return Err(windows::core::Error::new(
            E_FAIL,
            "Unable to uncloak window; Explorer may need to restart",
        ));
    }
    Ok(HideMethod::Parked)
}

pub fn restore(id: WindowId, original: Rect) -> Result<()> {
    if !is_window(id) {
        return Ok(());
    }
    set_hidden(id, false)?;
    place(id, original)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OriginalPlacement {
    pub flags: u32,
    pub show_command: u32,
    pub minimum: (i32, i32),
    pub maximum: (i32, i32),
    pub normal: Rect,
    pub border_color: Option<u32>,
}

pub fn capture_placement(id: WindowId) -> Result<OriginalPlacement> {
    unsafe {
        let mut placement =
            WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        GetWindowPlacement(hwnd(id), &mut placement)?;
        let mut color = 0u32;
        let border_color = DwmGetWindowAttribute(
            hwnd(id),
            DWMWA_BORDER_COLOR,
            &mut color as *mut _ as *mut c_void,
            4,
        )
        .ok()
        .map(|()| color);
        Ok(OriginalPlacement {
            flags: placement.flags.0,
            show_command: placement.showCmd,
            minimum: (placement.ptMinPosition.x, placement.ptMinPosition.y),
            maximum: (placement.ptMaxPosition.x, placement.ptMaxPosition.y),
            normal: rect(placement.rcNormalPosition),
            border_color,
        })
    }
}

pub fn restore_placement(id: WindowId, original: &OriginalPlacement) -> Result<()> {
    if !is_window(id) {
        return Ok(());
    }
    set_hidden(id, false)?;
    let normal = original.normal;
    let placement = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        flags: WINDOWPLACEMENT_FLAGS(original.flags),
        showCmd: original.show_command,
        ptMinPosition: POINT { x: original.minimum.0, y: original.minimum.1 },
        ptMaxPosition: POINT { x: original.maximum.0, y: original.maximum.1 },
        rcNormalPosition: RECT {
            left: normal.x,
            top: normal.y,
            right: normal.right(),
            bottom: normal.bottom(),
        },
    };
    unsafe {
        SetWindowPlacement(hwnd(id), &placement)?;
    }
    // BORDER_COLOR is documented for setting, not querying. When Windows does
    // not expose the original, clear moon's tint back to the documented default.
    set_border_color(id, original.border_color)?;
    Ok(())
}

pub fn process_alive(pid: u32) -> bool {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let alive = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(handle);
        alive
    }
}

pub fn process_session(pid: u32) -> Result<u32> {
    let mut session = 0;
    unsafe {
        windows::Win32::System::RemoteDesktop::ProcessIdToSessionId(pid, &mut session)?;
    }
    Ok(session)
}

pub fn running_processes() -> Result<Vec<(u32, String)>> {
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        let mut entry =
            PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut values = Vec::new();
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let end =
                    entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                values
                    .push((entry.th32ProcessID, String::from_utf16_lossy(&entry.szExeFile[..end])));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        Ok(values)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenCapabilities {
    pub process_id: u32,
    pub session_id: u32,
    pub integrity_rid: u32,
    pub integrity: String,
    pub ui_access: bool,
    pub elevated: bool,
    pub app_container: bool,
}

/// Check before adopting a window, so ordinary builds never record effects they
/// cannot safely reverse across an integrity boundary. Unknown tokens stay out.
pub fn can_manage_process(pid: u32) -> bool {
    let Ok(caller) = token_capabilities(std::process::id()) else { return false };
    let Ok(target) = token_capabilities(pid) else { return false };
    token_allows_management(&caller, &target)
}

fn token_allows_management(caller: &TokenCapabilities, target: &TokenCapabilities) -> bool {
    const SYSTEM_INTEGRITY_RID: u32 = 0x4000;
    caller.session_id == target.session_id
        && target.integrity_rid < SYSTEM_INTEGRITY_RID
        && (caller.ui_access || target.integrity_rid <= caller.integrity_rid)
}

#[cfg(test)]
mod process_management_tests {
    use super::*;

    // Well-known integrity SID RIDs from Windows' SECURITY_MANDATORY_* values.
    const LOW: u32 = 0x1000;
    const MEDIUM: u32 = 0x2000;
    const MEDIUM_PLUS: u32 = 0x2100;
    const HIGH: u32 = 0x3000;
    const SYSTEM: u32 = 0x4000;

    fn token(integrity_rid: u32) -> TokenCapabilities {
        TokenCapabilities {
            process_id: 1,
            session_id: 1,
            integrity_rid,
            integrity: String::new(),
            ui_access: false,
            elevated: false,
            app_container: false,
        }
    }

    #[test]
    fn admission_respects_actual_integrity_and_uiaccess() {
        let mut ordinary = token(MEDIUM);
        let low = token(LOW);
        let medium_plus = token(MEDIUM_PLUS);
        let high = token(HIGH);
        assert!(token_allows_management(&ordinary, &ordinary));
        assert!(token_allows_management(&ordinary, &low));
        assert!(!token_allows_management(&ordinary, &medium_plus));
        assert!(!token_allows_management(&ordinary, &high));
        // An elevation flag or administrative account is not a substitute for
        // the actual integrity label used by Windows access checks.
        ordinary.elevated = true;
        assert!(!token_allows_management(&ordinary, &high));
        ordinary.ui_access = true;
        assert!(token_allows_management(&ordinary, &high));
        assert!(!token_allows_management(&ordinary, &token(SYSTEM)));
        let mut other_session = token(LOW);
        other_session.session_id += 1;
        assert!(!token_allows_management(&ordinary, &other_session));
        assert!(token_allows_management(&high, &high));
    }

    #[test]
    fn native_admission_accepts_self_and_fails_closed_for_unqueryable_processes() {
        assert!(can_manage_process(std::process::id()));
        assert!(!can_manage_process(0));
        assert!(!can_manage_process(u32::MAX));
        let caller = token_capabilities(std::process::id()).unwrap();
        // Read an already running elevated app when this host has one. This
        // opens query-only handles and never creates or changes a window.
        for (pid, name) in running_processes().unwrap() {
            if name.eq_ignore_ascii_case("PowerToys.exe")
                && let Ok(target) = token_capabilities(pid)
                && target.integrity_rid > caller.integrity_rid
                && !caller.ui_access
            {
                assert!(!can_manage_process(pid));
                eprintln!(
                    "read-only elevated process exclusion: pid={pid}, caller_rid={}, target_rid={}",
                    caller.integrity_rid, target.integrity_rid
                );
            }
        }
    }
}

pub fn token_capabilities(pid: u32) -> Result<TokenCapabilities> {
    use windows::Win32::Security::*;
    struct Owned(HANDLE);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
    unsafe {
        let process = Owned(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?);
        let mut token = HANDLE::default();
        OpenProcessToken(process.0, TOKEN_QUERY, &mut token)?;
        let token = Owned(token);
        fn word(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<u32> {
            let mut value = 0u32;
            let mut length = 0;
            unsafe {
                GetTokenInformation(
                    token,
                    class,
                    Some(&mut value as *mut _ as *mut c_void),
                    4,
                    &mut length,
                )?;
            }
            Ok(value)
        }
        let mut length = 0;
        let _ = GetTokenInformation(token.0, TokenIntegrityLevel, None, 0, &mut length);
        let mut data = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        GetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            Some(data.as_mut_ptr().cast()),
            length,
            &mut length,
        )?;
        let label = &*(data.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        if count == 0 {
            return Err(windows::core::Error::new(E_FAIL, "Token has an empty integrity SID"));
        }
        let integrity_rid = *GetSidSubAuthority(label.Label.Sid, u32::from(count - 1));
        let integrity = match integrity_rid {
            0..4096 => "untrusted",
            4096..8192 => "low",
            8192..8448 => "medium",
            8448..12288 => "medium-plus",
            12288..16384 => "high",
            16384.. => "system",
        }
        .to_owned();
        Ok(TokenCapabilities {
            process_id: pid,
            session_id: word(token.0, TokenSessionId)?,
            integrity_rid,
            integrity,
            ui_access: word(token.0, TokenUIAccess)? != 0,
            elevated: word(token.0, TokenElevation)? != 0,
            app_container: word(token.0, TokenIsAppContainer)? != 0,
        })
    }
}

pub fn pump_messages() -> bool {
    unsafe {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            if message.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    true
}

pub fn set_border_color(id: WindowId, rgb: Option<u32>) -> Result<()> {
    let color = rgb.unwrap_or(0xffffffff);
    unsafe {
        DwmSetWindowAttribute(hwnd(id), DWMWA_BORDER_COLOR, &color as *const _ as *const c_void, 4)
    }
}

/// Owned test-window diagnostic properties: sequence, post-default QPC/rectangle,
/// then callback-entry QPC/rectangle. V2 pairs both samples from one callback.
/// Coordinates use their u32 bit pattern plus one, so NULL always means missing.
pub const TESTWIN_PLACEMENT_PROPERTIES: [PCWSTR; 11] = [
    windows::core::w!("Moon.TestPlacement.v2.Sequence"),
    windows::core::w!("Moon.TestPlacement.v2.Qpc"),
    windows::core::w!("Moon.TestPlacement.v2.Left"),
    windows::core::w!("Moon.TestPlacement.v2.Top"),
    windows::core::w!("Moon.TestPlacement.v2.Right"),
    windows::core::w!("Moon.TestPlacement.v2.Bottom"),
    windows::core::w!("Moon.TestPlacement.v2.EntryQpc"),
    windows::core::w!("Moon.TestPlacement.v2.EntryLeft"),
    windows::core::w!("Moon.TestPlacement.v2.EntryTop"),
    windows::core::w!("Moon.TestPlacement.v2.EntryRight"),
    windows::core::w!("Moon.TestPlacement.v2.EntryBottom"),
];

pub fn run_test_windows(count: usize) -> Result<()> {
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
    static WINDOWS: AtomicUsize = AtomicUsize::new(0);
    static KEY_DOWNS: AtomicU32 = AtomicU32::new(0);
    static KEY_UPS: AtomicU32 = AtomicU32::new(0);
    static CHARACTER_EVENTS: AtomicU32 = AtomicU32::new(0);
    static MOUSE_UPS: AtomicU32 = AtomicU32::new(0);
    static PLACEMENT_TIMING: AtomicBool = AtomicBool::new(false);
    const KEY_COUNTS_MESSAGE: u32 = WM_APP + 93;
    const CHARACTER_COUNT_MESSAGE: u32 = WM_APP + 94;
    const MOUSE_UP_COUNT_MESSAGE: u32 = WM_APP + 95;
    unsafe fn placement_entry(handle: HWND) -> Result<(i64, RECT)> {
        unsafe {
            let mut counter = 0;
            windows::Win32::System::Performance::QueryPerformanceCounter(&mut counter)?;
            let mut outer = RECT::default();
            GetWindowRect(handle, &mut outer)?;
            Ok((counter, outer))
        }
    }
    unsafe fn publish_placement(handle: HWND, entry: (i64, RECT)) -> Result<()> {
        unsafe {
            let mut outer = RECT::default();
            GetWindowRect(handle, &mut outer)?;
            let mut counter = 0;
            windows::Win32::System::Performance::QueryPerformanceCounter(&mut counter)?;
            let previous = GetPropW(handle, TESTWIN_PLACEMENT_PROPERTIES[0]).0 as usize;
            let sequence = (previous | 1).checked_add(1).ok_or_else(|| {
                windows::core::Error::new(E_FAIL, "test-window placement sequence overflow")
            })?;
            SetPropW(handle, TESTWIN_PLACEMENT_PROPERTIES[0], Some(HANDLE((sequence - 1) as _)))?;
            let values = [
                counter as usize,
                outer.left as u32 as usize + 1,
                outer.top as u32 as usize + 1,
                outer.right as u32 as usize + 1,
                outer.bottom as u32 as usize + 1,
                entry.0 as usize,
                entry.1.left as u32 as usize + 1,
                entry.1.top as u32 as usize + 1,
                entry.1.right as u32 as usize + 1,
                entry.1.bottom as u32 as usize + 1,
            ];
            for (name, value) in TESTWIN_PLACEMENT_PROPERTIES[1..].iter().zip(values) {
                SetPropW(handle, *name, Some(HANDLE(value as _)))?;
            }
            SetPropW(handle, TESTWIN_PLACEMENT_PROPERTIES[0], Some(HANDLE(sequence as _)))
        }
    }
    unsafe extern "system" fn procedure(
        handle: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_WINDOWPOSCHANGED if PLACEMENT_TIMING.load(Ordering::Relaxed) => {
                // The entry timestamp precedes its rectangle read; the existing
                // post-default timestamp follows its read. This bracket includes
                // both reads, normal WM_SIZE/WM_MOVE handling and scheduling.
                // Geometry is observed at each read, not at the QPC instant.
                let entry = unsafe { placement_entry(handle) };
                let result = unsafe { DefWindowProcW(handle, message, wparam, lparam) };
                if entry.and_then(|sample| unsafe { publish_placement(handle, sample) }).is_err() {
                    // Never leave an old complete sample after publication fails.
                    let _ = unsafe { RemovePropW(handle, TESTWIN_PLACEMENT_PROPERTIES[0]) };
                }
                result
            }
            WM_NCDESTROY if PLACEMENT_TIMING.load(Ordering::Relaxed) => {
                for name in TESTWIN_PLACEMENT_PROPERTIES {
                    let _ = unsafe { RemovePropW(handle, name) };
                }
                unsafe { DefWindowProcW(handle, message, wparam, lparam) }
            }
            WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP
                if wparam.0 == usize::from(VK_OEM_MINUS.0) =>
            {
                if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN) {
                    KEY_DOWNS.fetch_add(1, Ordering::Relaxed);
                } else {
                    KEY_UPS.fetch_add(1, Ordering::Relaxed);
                }
                unsafe { DefWindowProcW(handle, message, wparam, lparam) }
            }
            KEY_COUNTS_MESSAGE => LRESULT(
                (u64::from(KEY_DOWNS.load(Ordering::Relaxed))
                    | (u64::from(KEY_UPS.load(Ordering::Relaxed)) << 32)) as isize,
            ),
            WM_CHAR | WM_SYSCHAR | WM_DEADCHAR | WM_SYSDEADCHAR | WM_UNICHAR => {
                CHARACTER_EVENTS.fetch_add(1, Ordering::Relaxed);
                unsafe { DefWindowProcW(handle, message, wparam, lparam) }
            }
            CHARACTER_COUNT_MESSAGE => LRESULT(CHARACTER_EVENTS.load(Ordering::Relaxed) as isize),
            MOUSE_UP_COUNT_MESSAGE => LRESULT(MOUSE_UPS.load(Ordering::Relaxed) as isize),
            WM_LBUTTONUP => {
                MOUSE_UPS.fetch_add(1, Ordering::Relaxed);
                unsafe { DefWindowProcW(handle, message, wparam, lparam) }
            }
            WM_LBUTTONDOWN => {
                unsafe {
                    SetWindowLongPtrW(
                        handle,
                        GWLP_USERDATA,
                        GetWindowLongPtrW(handle, GWLP_USERDATA) + 1,
                    );
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                if WINDOWS.fetch_sub(1, Ordering::SeqCst) == 1 {
                    unsafe {
                        PostQuitMessage(0);
                    }
                }
                LRESULT(0)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = unsafe { BeginPaint(handle, &mut paint) };
                let text = wide("moon integration window — move, focus, resize and close me");
                unsafe {
                    let _ = TextOutW(dc, 24, 24, &text[..text.len() - 1]);
                    let _ = EndPaint(handle, &paint);
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(handle, message, wparam, lparam) },
        }
    }
    if !(1..=128).contains(&count) {
        return Err(windows::core::Error::new(
            E_INVALIDARG,
            "testwin count must be between 1 and 128",
        ));
    }
    KEY_DOWNS.store(0, Ordering::Relaxed);
    KEY_UPS.store(0, Ordering::Relaxed);
    CHARACTER_EVENTS.store(0, Ordering::Relaxed);
    MOUSE_UPS.store(0, Ordering::Relaxed);
    PLACEMENT_TIMING.store(
        std::env::var("MOON_TESTWIN_PLACEMENT_TIMING").is_ok_and(|value| value == "1"),
        Ordering::Relaxed,
    );
    let _ = enable_dpi_awareness();
    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let name = wide("MoonTestWindow");
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(name.as_ptr()),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut c_void),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        for index in 0..count {
            let title = wide(&format!("moon test window {}", index + 1));
            let handle = CreateWindowExW(
                WS_EX_APPWINDOW,
                PCWSTR(name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW,
                100 + (index as i32 % 8) * 60,
                100 + (index as i32 % 8) * 40,
                640,
                480,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            WINDOWS.fetch_add(1, Ordering::SeqCst);
            let _ = ShowWindow(handle, SW_SHOW);
        }
        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0);
            if result.0 == -1 {
                return Err(windows::core::Error::from_thread());
            }
            if result.0 == 0 {
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}
