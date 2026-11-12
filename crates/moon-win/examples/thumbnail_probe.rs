#[cfg(windows)]
mod common;

#[cfg(not(windows))]
fn main() {
    eprintln!("This probe runs on Windows.");
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}

#[cfg(windows)]
mod native {
    use moon_core::ids::WindowId;
    use moon_win::{platform, runtime};
    use serde_json::json;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU32, Ordering},
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Foundation::*,
            Graphics::{Dwm::*, Gdi::*},
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::*,
        },
        core::{BOOL, w},
    };
    static COLOR: AtomicU32 = AtomicU32::new(0x202020);
    static SOURCE: AtomicBool = AtomicBool::new(false);
    const COLOR_MESSAGE: u32 = WM_APP + 41;
    const CLOAK_MESSAGE: u32 = WM_APP + 42;

    unsafe extern "system" fn procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            COLOR_MESSAGE => {
                COLOR.store(wparam.0 as u32, Ordering::Relaxed);
                unsafe {
                    let _ = InvalidateRect(Some(window), None, false);
                    let _ = UpdateWindow(window);
                }
                LRESULT(0)
            }
            CLOAK_MESSAGE => {
                let cloak = BOOL::from(wparam.0 != 0);
                let result = unsafe {
                    DwmSetWindowAttribute(
                        window,
                        DWMWA_CLOAK,
                        &cloak as *const _ as _,
                        size_of::<BOOL>() as u32,
                    )
                };
                LRESULT(result.err().map_or(0, |error| error.code().0 as isize))
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                unsafe {
                    let dc = BeginPaint(window, &mut paint);
                    let mut rect = RECT::default();
                    let _ = GetClientRect(window, &mut rect);
                    let brush = CreateSolidBrush(COLORREF(COLOR.load(Ordering::Relaxed)));
                    FillRect(dc, &rect, brush);
                    let _ = DeleteObject(brush.into());
                    let _ = EndPaint(window, &paint);
                }
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_CLOSE => {
                unsafe {
                    let _ = DestroyWindow(window);
                }
                LRESULT(0)
            }
            WM_DESTROY if SOURCE.load(Ordering::Relaxed) => {
                unsafe {
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
        }
    }
    struct OwnedWindow(HWND);
    impl Drop for OwnedWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
    struct Thumbnail(isize);
    impl Drop for Thumbnail {
        fn drop(&mut self) {
            unsafe {
                let _ = DwmUnregisterThumbnail(self.0);
            }
        }
    }
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn create(
        name: &windows::core::PCWSTR,
        style: WINDOW_STYLE,
        extended: WINDOW_EX_STYLE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> windows::core::Result<OwnedWindow> {
        let instance = unsafe { GetModuleHandleW(None)? };
        Ok(OwnedWindow(unsafe {
            CreateWindowExW(
                extended,
                w!("MoonThumbnailProbe"),
                *name,
                style,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        }))
    }
    fn message(window: HWND, message: u32, value: usize) -> windows::core::Result<()> {
        let mut reply = 0;
        let sent = unsafe {
            SendMessageTimeoutW(
                window,
                message,
                WPARAM(value),
                LPARAM(0),
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                1000,
                Some(&mut reply),
            )
        };
        if sent.0 == 0 {
            return Err(windows::core::Error::from_thread());
        }
        if reply != 0 {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(reply as i32)));
        }
        Ok(())
    }
    fn settle() {
        let deadline = Instant::now() + Duration::from_millis(220);
        while Instant::now() < deadline {
            runtime::pump(10);
        }
        unsafe {
            let _ = DwmFlush();
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let _com = platform::Apartment::new()?;
        platform::enable_dpi_awareness()?;
        let arguments: Vec<_> = std::env::args().collect();
        let is_source = arguments.iter().any(|value| value == "--source");
        SOURCE.store(is_source, Ordering::Relaxed);
        let instance = unsafe { GetModuleHandleW(None)? };
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: w!("MoonThumbnailProbe"),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(windows::core::Error::from_thread().into());
        }
        let monitor = platform::monitors()?.into_iter().next().ok_or("no monitor")?;
        let work = monitor.work_area;
        if is_source {
            let window = create(
                &w!("Moon owned thumbnail source"),
                WS_OVERLAPPEDWINDOW,
                WS_EX_APPWINDOW,
                work.x + 80,
                work.y + 80,
                540,
                320,
            )?;
            unsafe {
                let _ = ShowWindow(window.0, SW_SHOWNOACTIVATE);
            }
            while runtime::pump(1000) {}
            return Ok(());
        }
        let output = PathBuf::from(arguments.get(1).ok_or("pass an owned evidence directory")?);
        std::fs::create_dir_all(&output)?;
        struct Foreground(HWND, u32);
        impl Drop for Foreground {
            fn drop(&mut self) {
                let mut owner = 0;
                unsafe {
                    GetWindowThreadProcessId(self.0, Some(&mut owner));
                    if owner == self.1 && IsWindow(Some(self.0)).as_bool() {
                        let _ = SetForegroundWindow(self.0);
                    }
                }
            }
        }
        let foreground = unsafe { GetForegroundWindow() };
        let mut foreground_owner = 0;
        unsafe {
            GetWindowThreadProcessId(foreground, Some(&mut foreground_owner));
        }
        let _foreground = Foreground(foreground, foreground_owner);
        let child = Child(runtime::internal_command()?.arg("--source").spawn()?);
        struct Search {
            process: u32,
            found: Option<HWND>,
        }
        unsafe extern "system" fn find(window: HWND, param: LPARAM) -> BOOL {
            let search = unsafe { &mut *(param.0 as *mut Search) };
            let mut process = 0;
            unsafe {
                GetWindowThreadProcessId(window, Some(&mut process));
            }
            if process == search.process && unsafe { IsWindowVisible(window) }.as_bool() {
                search.found = Some(window);
                return false.into();
            }
            true.into()
        }
        let mut search = Search { process: child.0.id(), found: None };
        let deadline = Instant::now() + Duration::from_secs(5);
        while search.found.is_none() && Instant::now() < deadline {
            unsafe {
                let _ = EnumWindows(Some(find), LPARAM(&mut search as *mut _ as isize));
            }
            runtime::pump(20);
        }
        let source = search.found.ok_or("owned source did not open")?;
        let source_id = WindowId(source.0 as usize as u64);
        let original = platform::capture_placement(source_id)?;
        struct Restore(WindowId, platform::OriginalPlacement);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = platform::restore_placement(self.0, &self.1);
            }
        }
        let _restore = Restore(source_id, original);
        let mut results = Vec::new();
        for (kind, extra) in [
            ("regular", WINDOW_EX_STYLE(0)),
            ("layered", WS_EX_LAYERED),
            ("no_redirection", WS_EX_NOREDIRECTIONBITMAP),
        ] {
            let destination = create(
                &w!("Moon owned thumbnail destination"),
                WS_POPUP | WS_DISABLED,
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | extra,
                work.x + work.width / 2,
                work.y + 120,
                360,
                260,
            )?;
            if extra == WS_EX_LAYERED {
                unsafe {
                    SetLayeredWindowAttributes(destination.0, COLORREF(0), 255, LWA_ALPHA)?;
                }
            }
            unsafe {
                let _ = ShowWindow(destination.0, SW_SHOWNOACTIVATE);
            }
            let thumbnail = match unsafe { DwmRegisterThumbnail(destination.0, source) } {
                Ok(value) => Thumbnail(value),
                Err(error) => {
                    results
                        .push(json!({"destination":kind,"registration_error":error.to_string()}));
                    continue;
                }
            };
            let properties = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_OPACITY
                    | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT { left: 20, top: 30, right: 340, bottom: 230 },
                opacity: 255,
                fVisible: true.into(),
                fSourceClientAreaOnly: true.into(),
                ..Default::default()
            };
            unsafe {
                DwmUpdateThumbnailProperties(thumbnail.0, &properties)?;
            }
            for (stage, color) in [
                ("visible_a", 0x3080d0u32),
                ("visible_b", 0x50c030),
                ("shell_cloaked", 0xd05040),
                ("uncloaked", 0x40b0e0),
                ("app_cloaked", 0x9020c0),
                ("app_uncloaked", 0x30b080),
            ] {
                let mut hiding = None;
                match stage {
                    "shell_cloaked" => {
                        hiding = Some(platform::set_hidden(source_id, true)?);
                    }
                    "uncloaked" => {
                        platform::set_hidden(source_id, false)?;
                        platform::restore_placement(source_id, &_restore.1)?;
                    }
                    "app_cloaked" => message(source, CLOAK_MESSAGE, 1)?,
                    "app_uncloaked" => message(source, CLOAK_MESSAGE, 0)?,
                    _ => {}
                }
                message(source, COLOR_MESSAGE, color as usize)?;
                settle();
                let mut rect = RECT::default();
                unsafe {
                    GetWindowRect(destination.0, &mut rect)?;
                }
                let image = output.join(format!("{kind}-{stage}.bmp"));
                let actual = crate::common::capture(rect, &image)?;
                results.push(json!({"destination":kind,"source_state":stage,"source_pid":child.0.id(),"source_hwnd":source_id.0,"cloak_bits":platform::cloaked(source_id)?,"hide_method":hiding,"expected_colorref":color,"actual_colorref":actual,"live_color_matches":actual==color,"image":image}));
            }
        }
        let report = json!({"recorded_unix_seconds":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),"source":"separate owned Win32/GDI process","results":results,"other_applications_modified":false});
        std::fs::write(output.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    }
}
