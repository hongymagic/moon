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
    use moon_core::geometry::{Point, Rect};
    use moon_win::{
        carousel::{Carousel, PageWindow, Transition, WindowRef},
        platform, runtime,
    };
    use serde_json::json;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU32, Ordering},
        time::{Duration, Instant},
    };
    use windows::Win32::{
        Foundation::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    };
    const MARKER: usize = 0x4d43_4152;
    static DOWNS: AtomicU32 = AtomicU32::new(0);
    static UPS: AtomicU32 = AtomicU32::new(0);
    static BUTTON_HELD: AtomicBool = AtomicBool::new(false);
    unsafe extern "system" fn observer(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let event = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            if event.dwExtraInfo == MARKER {
                if wparam.0 as u32 == WM_LBUTTONDOWN {
                    DOWNS.fetch_add(1, Ordering::Relaxed);
                }
                if wparam.0 as u32 == WM_LBUTTONUP {
                    UPS.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
    fn button(flags: MOUSE_EVENT_FLAGS) -> Result<(), Box<dyn std::error::Error>> {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT { dwFlags: flags, dwExtraInfo: MARKER, ..Default::default() },
            },
        };
        if unsafe { SendInput(&[input], size_of::<INPUT>() as i32) } != 1 {
            return Err("owned mouse input was rejected".into());
        }
        BUTTON_HELD.store(flags == MOUSEEVENTF_LEFTDOWN, Ordering::Relaxed);
        Ok(())
    }
    fn pump(carousel: &mut Carousel, milliseconds: u64) -> windows::core::Result<()> {
        let until = Instant::now() + Duration::from_millis(milliseconds);
        while Instant::now() < until {
            runtime::pump(5);
            carousel.tick()?;
        }
        Ok(())
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let _com = platform::Apartment::new()?;
        platform::enable_dpi_awareness()?;
        let arguments: Vec<_> = std::env::args().collect();
        let helper = arguments.get(1).ok_or("pass testwin.exe then an owned evidence directory")?;
        let output = PathBuf::from(arguments.get(2).ok_or("missing evidence directory")?);
        std::fs::create_dir_all(&output)?;
        let original_foreground = unsafe { GetForegroundWindow() };
        let original_pointer = platform::cursor_position()?;
        struct Foreground(HWND, Point);
        impl Drop for Foreground {
            fn drop(&mut self) {
                let _ = platform::cursor_jump(self.1);
                unsafe {
                    let _ = SetForegroundWindow(self.0);
                }
            }
        }
        let _foreground = Foreground(original_foreground, original_pointer);
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let child = Child(std::process::Command::new(helper).arg("4").spawn()?);
        let until = Instant::now() + Duration::from_secs(5);
        let windows = loop {
            let mut windows: Vec<_> = platform::enumerate_windows()?
                .into_iter()
                .filter(|w| w.process_id == child.0.id() && w.class == "MoonTestWindow")
                .collect();
            windows.sort_by_key(|w| w.id);
            if windows.len() == 4 {
                break windows;
            }
            if Instant::now() > until {
                return Err("owned helpers did not open".into());
            }
            runtime::pump(20);
        };
        struct Restore(Vec<(moon_core::ids::WindowId, platform::OriginalPlacement)>);
        impl Drop for Restore {
            fn drop(&mut self) {
                for (id, placement) in &self.0 {
                    let _ = platform::restore_placement(*id, placement);
                }
            }
        }
        let _restore = Restore(
            windows
                .iter()
                .map(|w| platform::capture_placement(w.id).map(|p| (w.id, p)))
                .collect::<windows::core::Result<Vec<_>>>()?,
        );
        let monitor = platform::monitors()?.into_iter().next().ok_or("no monitor")?;
        let viewport = Rect::new(
            monitor.work_area.x + monitor.work_area.width / 2 - 420,
            monitor.work_area.y + 120,
            840,
            620,
        );
        let outgoing = [
            Rect::new(viewport.x + 20, viewport.y + 20, 390, 580),
            Rect::new(viewport.x + 430, viewport.y + 20, 390, 580),
        ];
        let incoming = [
            Rect::new(viewport.x + 20, viewport.y + 20, 800, 280),
            Rect::new(viewport.x + 20, viewport.y + 320, 800, 280),
        ];
        for (window, frame) in windows[..2].iter().zip(outgoing) {
            platform::place(window.id, frame)?;
        }
        // Hidden sources initially retain a four-tile size, as they do when
        // membership changes before the first visit to their workspace.
        for (index, window) in windows[2..].iter().enumerate() {
            platform::place(
                window.id,
                Rect::new(viewport.x + 20 + index as i32 * 410, viewport.y + 20, 390, 280),
            )?;
            platform::set_hidden(window.id, true)?;
        }
        platform::focus(windows[0].id)?;
        platform::cursor_jump(Point::new(viewport.x + 100, viewport.y + 140))?;
        runtime::pump(150);
        let pages = |start: usize, rects: [Rect; 2]| {
            windows[start..start + 2]
                .iter()
                .zip(rects)
                .map(|(window, rect)| PageWindow {
                    window: WindowRef { id: window.id, process_id: child.0.id() },
                    rect,
                })
                .collect()
        };
        let spec = Transition {
            from_workspace: moon_core::ids::WorkspaceId(0),
            to_workspace: moon_core::ids::WorkspaceId(1),
            from: pages(0, outgoing),
            to: pages(2, incoming),
            viewport,
            direction: 1,
            background_rgb: 0x142033,
        };
        let apply = |transition: &Transition| -> windows::core::Result<()> {
            for page in &transition.from {
                platform::set_hidden(page.window.id, true)?;
            }
            for page in &transition.to {
                platform::place(page.window.id, page.rect)?;
                platform::set_hidden(page.window.id, false)?;
            }
            if let Some(page) = transition.to.first() {
                platform::focus(page.window.id)?;
            }
            Ok(())
        };
        let mut carousel = Carousel::new()?;
        let mut results = Vec::new();
        let before = runtime::resource_stats()?;
        if !carousel.prepare(std::slice::from_ref(&spec))? {
            return Err("carousel prepare was suppressed; check system motion preference and source admission".into());
        }
        apply(&spec)?;
        carousel.animate()?;
        let start = Instant::now();
        let mut samples = 0;
        while carousel.is_active() {
            runtime::pump(10);
            carousel.tick()?;
            if carousel.is_active()
                && start.elapsed() >= Duration::from_millis(45 * (samples + 1))
                && samples < 3
            {
                let rect = RECT {
                    left: viewport.x,
                    top: viewport.y,
                    right: viewport.right(),
                    bottom: viewport.bottom(),
                };
                crate::common::capture(rect, &output.join(format!("frame-{samples}.bmp")))?;
                samples += 1;
            }
        }
        let after = runtime::resource_stats()?;
        let incoming_exact = spec.to.iter().all(|page| {
            platform::window_rect(page.window.id).is_ok_and(|frame| frame == page.rect)
                && platform::cloaked(page.window.id).is_ok_and(|bits| bits == 0)
        });
        let outgoing_hidden = spec
            .from
            .iter()
            .all(|page| platform::cloaked(page.window.id).is_ok_and(|bits| bits != 0));
        results.push(json!({"case":"two tiled workspaces translate together and incoming sources resize","passed":!carousel.is_active()&&incoming_exact&&outgoing_hidden,"outgoing_windows":spec.from.len(),"incoming_windows":spec.to.len(),"incoming_final_frames":spec.to.iter().map(|page|platform::window_rect(page.window.id).ok()).collect::<Vec<_>>(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"captured_frames":samples,"working_set_before":before.working_set_bytes,"working_set_after":after.working_set_bytes}));

        let reverse = Transition {
            from_workspace: spec.to_workspace,
            to_workspace: spec.from_workspace,
            from: spec.to.clone(),
            to: spec.from.clone(),
            direction: -1,
            ..spec.clone()
        };
        if !carousel.prepare(std::slice::from_ref(&reverse))? {
            return Err("reverse prepare failed".into());
        }
        apply(&reverse)?;
        carousel.animate()?;
        runtime::pump(40);
        carousel.tick()?;
        let repeated = carousel.prepare(std::slice::from_ref(&spec))?;
        apply(&spec)?;
        carousel.animate()?;
        runtime::pump(230);
        carousel.tick()?;
        let settled = spec
            .to
            .iter()
            .all(|page| platform::window_rect(page.window.id).is_ok_and(|rect| rect == page.rect));
        results.push(json!({"case":"rapid reversal retargets and settles without stale overlay","passed":repeated&&!carousel.is_active()&&settled,"retargeted":repeated}));
        apply(&reverse)?;

        let instance = unsafe { GetModuleHandleW(None)? };
        struct Hook(HHOOK);
        impl Drop for Hook {
            fn drop(&mut self) {
                unsafe {
                    let _ = UnhookWindowsHookEx(self.0);
                }
            }
        }
        let _observer = Hook(unsafe {
            SetWindowsHookExW(WH_MOUSE_LL, Some(observer), Some(instance.into()), 0)?
        });
        struct ReleaseButton;
        impl Drop for ReleaseButton {
            fn drop(&mut self) {
                if BUTTON_HELD.load(Ordering::Relaxed) {
                    let _ = button(MOUSEEVENTF_LEFTUP);
                    runtime::pump(0);
                }
            }
        }
        let _release_button = ReleaseButton;
        let delivered_ups = |window| unsafe {
            SendMessageW(platform::hwnd(window), WM_APP + 95, Some(WPARAM(0)), Some(LPARAM(0))).0
        };
        let ups_before = delivered_ups(spec.from[0].window.id);
        button(MOUSEEVENTF_LEFTDOWN)?;
        pump(&mut carousel, 20)?;
        let held_skipped = !carousel.prepare(std::slice::from_ref(&spec))? && !carousel.is_active();
        button(MOUSEEVENTF_LEFTUP)?;
        pump(&mut carousel, 20)?;
        results.push(json!({"case":"a preexisting held mouse press keeps its native release","passed":held_skipped&&delivered_ups(spec.from[0].window.id)==ups_before+1&&DOWNS.load(Ordering::Relaxed)==1&&UPS.load(Ordering::Relaxed)==1}));
        DOWNS.store(0, Ordering::Relaxed);
        UPS.store(0, Ordering::Relaxed);
        if !carousel.prepare(std::slice::from_ref(&spec))? {
            return Err("click dismissal prepare failed".into());
        }
        apply(&spec)?;
        carousel.animate()?;
        if unsafe { GetForegroundWindow() } != platform::hwnd(spec.to[0].window.id) {
            return Err("owned target did not retain foreground; no mouse event was sent".into());
        }
        let clicks_before =
            unsafe { GetWindowLongPtrW(platform::hwnd(spec.to[0].window.id), GWLP_USERDATA) };
        let ups_before = delivered_ups(spec.to[0].window.id);
        button(MOUSEEVENTF_LEFTDOWN)?;
        pump(&mut carousel, 30)?;
        let dismissed = !carousel.is_active();
        button(MOUSEEVENTF_LEFTUP)?;
        pump(&mut carousel, 30)?;
        let swallowed = DOWNS.load(Ordering::Relaxed) == 0 && UPS.load(Ordering::Relaxed) == 0;
        let clicks_after =
            unsafe { GetWindowLongPtrW(platform::hwnd(spec.to[0].window.id), GWLP_USERDATA) };
        results.push(json!({"case":"first pointer click dismisses with a swallowed pair","passed":dismissed&&swallowed&&clicks_before==clicks_after&&delivered_ups(spec.to[0].window.id)==ups_before,"forwarded_downs":DOWNS.load(Ordering::Relaxed),"forwarded_ups":UPS.load(Ordering::Relaxed)}));
        button(MOUSEEVENTF_LEFTDOWN)?;
        button(MOUSEEVENTF_LEFTUP)?;
        pump(&mut carousel, 30)?;
        let clicks_final =
            unsafe { GetWindowLongPtrW(platform::hwnd(spec.to[0].window.id), GWLP_USERDATA) };
        results.push(json!({"case":"next click reaches actual focused application","passed":DOWNS.load(Ordering::Relaxed)==1&&UPS.load(Ordering::Relaxed)==1&&clicks_final==clicks_after+1&&delivered_ups(spec.to[0].window.id)==ups_before+1}));

        if !carousel.prepare(std::slice::from_ref(&reverse))? {
            return Err("movement dismissal prepare failed".into());
        }
        apply(&reverse)?;
        carousel.animate()?;
        let point = platform::cursor_position()?;
        platform::cursor_jump(Point::new(point.x + 3, point.y))?;
        pump(&mut carousel, 30)?;
        results
            .push(json!({"case":"mouse movement cancels visuals","passed":!carousel.is_active()}));
        let mut invalid = spec.clone();
        invalid.from[0].window.process_id = u32::MAX;
        results.push(json!({"case":"stale process owner refuses presentation","passed":!carousel.prepare(&[invalid])?&&!carousel.is_active()}));

        let empty_incoming = Transition { to: Vec::new(), ..spec.clone() };
        if !carousel.prepare(std::slice::from_ref(&empty_incoming))? {
            return Err("empty incoming workspace prepare failed".into());
        }
        apply(&empty_incoming)?;
        carousel.animate()?;
        pump(&mut carousel, 240)?;
        results.push(json!({"case":"empty incoming workspace settles on its background","passed":!carousel.is_active()&&windows.iter().all(|window|platform::cloaked(window.id).is_ok_and(|bits|bits!=0))}));
        let empty_outgoing = Transition { from: Vec::new(), to: spec.from.clone(), ..spec.clone() };
        if !carousel.prepare(std::slice::from_ref(&empty_outgoing))? {
            return Err("empty outgoing workspace prepare failed".into());
        }
        apply(&empty_outgoing)?;
        carousel.animate()?;
        pump(&mut carousel, 240)?;
        results.push(json!({"case":"empty outgoing workspace reveals both tiled windows","passed":!carousel.is_active()&&empty_outgoing.to.iter().all(|page|platform::window_rect(page.window.id).is_ok_and(|frame|frame==page.rect)&&platform::cloaked(page.window.id).is_ok_and(|bits|bits==0))}));
        carousel.suspend();
        let passed = results.iter().all(|entry| entry["passed"] == true);
        let report = json!({"passed":passed,"checks":results,"source_process":child.0.id(),"other_applications_modified":false,"uac_requested":false});
        std::fs::write(output.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        if !passed {
            return Err("carousel native check failed; inspect its report".into());
        }
        Ok(())
    }
}
