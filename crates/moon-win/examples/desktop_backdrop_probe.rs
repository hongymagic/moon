//! Presents only the documented shell desktop source in an owned no-activate
//! popup. Optional --carousel checks production motion using owned sources only.
//! It never moves, hides, focuses or changes a foreign window.
#[cfg(windows)]
mod common;

#[cfg(not(windows))]
fn main() {
    eprintln!("This probe runs on Windows.");
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_core::{
        geometry::Rect,
        ids::{WindowId, WorkspaceId},
    };
    use moon_win::{
        carousel::{Carousel, PageWindow, Transition, WindowRef},
        platform, runtime,
    };
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Foundation::*,
            Graphics::{Dwm::*, Gdi::*},
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::*,
        },
        core::w,
    };
    unsafe extern "system" fn procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_PAINT {
            let mut paint = PAINTSTRUCT::default();
            unsafe {
                let dc = BeginPaint(window, &mut paint);
                let color = GetWindowLongPtrW(window, GWLP_USERDATA) as u32;
                let brush = CreateSolidBrush(COLORREF(if color == 0 { 0x00ff00ff } else { color }));
                FillRect(dc, &paint.rcPaint, brush);
                let _ = DeleteObject(brush.into());
                let _ = EndPaint(window, &paint);
            }
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }
    struct OwnedWindow(HWND);
    impl Drop for OwnedWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
    struct OwnedThumbnail(isize);
    impl Drop for OwnedThumbnail {
        fn drop(&mut self) {
            unsafe {
                let _ = DwmUnregisterThumbnail(self.0);
            }
        }
    }
    let _apartment = platform::Apartment::new()?;
    platform::enable_dpi_awareness()?;
    let output = PathBuf::from(std::env::args().nth(1).ok_or("pass a fresh evidence directory")?);
    let exercise_carousel = std::env::args().nth(2).as_deref() == Some("--carousel");
    std::fs::create_dir(&output)?;
    let foreground = unsafe { GetForegroundWindow() };
    let source = unsafe { GetShellWindow() };
    if source.is_invalid() {
        return Err("no shell desktop".into());
    }
    let mut source_bounds = RECT::default();
    unsafe {
        GetWindowRect(source, &mut source_bounds)?;
    }
    let monitor = platform::monitors()?.into_iter().next().ok_or("no monitor")?;
    let width = 960.min(monitor.work_area.width);
    let height = 540.min(monitor.work_area.height);
    let left = monitor.work_area.x + (monitor.work_area.width - width) / 2;
    let top = monitor.work_area.y + (monitor.work_area.height - height) / 2;
    let instance = unsafe { GetModuleHandleW(None)? };
    let class = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance.into(),
        lpszClassName: w!("MoonDesktopBackdropProbe"),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(windows::core::Error::from_thread().into());
    }
    let window = OwnedWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.lpszClassName,
            w!("Moon desktop background inspection"),
            WS_POPUP | WS_DISABLED,
            left,
            top,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )?
    });
    let thumbnail = OwnedThumbnail(unsafe { DwmRegisterThumbnail(window.0, source)? });
    let dimensions = unsafe { DwmQueryThumbnailSourceSize(thumbnail.0)? };
    unsafe {
        DwmUpdateThumbnailProperties(
            thumbnail.0,
            &DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_OPACITY
                    | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT { left: 0, top: 0, right: width, bottom: height },
                opacity: 255,
                fVisible: true.into(),
                fSourceClientAreaOnly: false.into(),
                ..Default::default()
            },
        )?;
        let _ = ShowWindow(window.0, SW_SHOWNOACTIVATE);
        let _ = UpdateWindow(window.0);
        DwmFlush()?;
    }
    let until = Instant::now() + Duration::from_millis(300);
    while Instant::now() < until {
        runtime::pump(20);
    }
    let image = output.join("desktop-source.bmp");
    let center_color =
        common::capture(RECT { left, top, right: left + width, bottom: top + height }, &image)?;
    let foreground_unchanged = foreground == unsafe { GetForegroundWindow() };
    let mut report = serde_json::json!({
        "source_hwnd": source.0 as u64,
        "source_pid": platform::process_id(moon_core::ids::WindowId(source.0 as u64)),
        "source_rect": {"left":source_bounds.left,"top":source_bounds.top,
            "right":source_bounds.right,"bottom":source_bounds.bottom},
        "source_pixels": {"width":dimensions.cx,"height":dimensions.cy},
        "center_colorref": center_color,
        "fallback_colorref": 0x00ff00ffu32,
        "foreground_unchanged": foreground_unchanged,
        "other_windows_modified": false,
        "desktop_source_requires_visual_verification": true,
        "image": image,
    });
    if exercise_carousel {
        // This owned reference remains beneath the production overlay. It lets
        // us compare desktop pixels without hiding any of the user's apps.
        if dimensions.cx != source_bounds.right - source_bounds.left
            || dimensions.cy != source_bounds.bottom - source_bounds.top
        {
            return Err("probe requires a physical-pixel desktop source".into());
        }
        let viewport = Rect::new(left, top, width, height);
        let capture_rect = RECT { left, top, right: left + width, bottom: top + height };
        unsafe {
            DwmUpdateThumbnailProperties(
                thumbnail.0,
                &DWM_THUMBNAIL_PROPERTIES {
                    dwFlags: DWM_TNP_RECTSOURCE,
                    rcSource: RECT {
                        left: left - source_bounds.left,
                        top: top - source_bounds.top,
                        right: left + width - source_bounds.left,
                        bottom: top + height - source_bounds.top,
                    },
                    ..Default::default()
                },
            )?;
            DwmFlush()?;
        }
        let rect = Rect::new(left + 60, top + 70, width - 120, height - 140);
        let mut sources = Vec::new();
        for color in [0x003080d0, 0x0070b040] {
            let source = OwnedWindow(unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_APPWINDOW,
                    class.lpszClassName,
                    w!("Moon owned carousel source"),
                    WS_POPUP,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )?
            });
            unsafe {
                SetWindowLongPtrW(source.0, GWLP_USERDATA, color);
                let _ = ShowWindow(source.0, SW_SHOWNOACTIVATE);
                let _ = UpdateWindow(source.0);
            }
            sources.push(source);
        }
        let cloak = |window: HWND, hidden: bool| -> windows::core::Result<_> {
            let id = WindowId(window.0 as u64);
            if !hidden {
                platform::place(id, rect)?;
            }
            platform::set_hidden(id, hidden)
        };
        let until = Instant::now() + Duration::from_secs(2);
        while sources.iter().any(|source| {
            !platform::window_on_current_virtual_desktop(WindowId(source.0.0 as u64))
                .unwrap_or(false)
        }) {
            if Instant::now() >= until {
                return Err(
                    "owned no-activate sources were not admitted to the current native desktop"
                        .into(),
                );
            }
            runtime::pump(20);
        }
        let initial_hide = cloak(sources[1].0, true)?;
        let source_states = || {
            sources.iter().map(|source| {
                let id = WindowId(source.0.0 as u64);
                serde_json::json!({"id":id.0,"current_desktop":platform::window_on_current_virtual_desktop(id).map_err(|e|e.to_string()),
                    "cloaked":platform::cloaked(id).map_err(|e|e.to_string()),"rect":platform::window_rect(id).map_err(|e|e.to_string())})
            }).collect::<Vec<_>>()
        };
        let gap_pixels = |pixels: &[u8]| {
            [4, 24, 44]
                .into_iter()
                .flat_map(|y| (0..16).map(move |x| (x, y)))
                .map(|(x, y)| {
                    common::colorref(pixels, width as usize, (2 + x * (width - 4) / 16) as usize, y)
                })
                .collect::<Vec<_>>()
        };
        let cursor = || -> windows::core::Result<_> {
            let mut point = POINT::default();
            unsafe {
                GetCursorPos(&mut point)?;
            }
            Ok(serde_json::json!({"x":point.x,"y":point.y}))
        };
        let app_pixels_present = |pixels: &[u8]| {
            (0..width as usize).step_by(8).any(|x| {
                matches!(
                    common::colorref(pixels, width as usize, x, height as usize / 2),
                    0x003080d0 | 0x0070b040
                )
            })
        };
        unsafe {
            DwmFlush()?;
        }
        let baseline_pixels = common::read_pixels(capture_rect)?;
        let expected = gap_pixels(&baseline_pixels);
        common::write_bitmap(capture_rect, &baseline_pixels, &output.join("carousel-before.bmp"))?;
        let page = |index: usize| {
            vec![PageWindow {
                window: WindowRef {
                    id: WindowId(sources[index].0.0 as u64),
                    process_id: std::process::id(),
                },
                rect,
            }]
        };
        let mut carousel = Carousel::new()?;
        let mut cases = Vec::new();
        for (name, from_id, to_id, from, to) in [
            ("occupied-to-occupied", 0, 1, page(0), page(1)),
            ("occupied-to-empty", 1, 2, page(1), Vec::new()),
            ("empty-to-occupied", 2, 0, Vec::new(), page(0)),
        ] {
            let spec = Transition {
                from_workspace: WorkspaceId(from_id),
                to_workspace: WorkspaceId(to_id),
                from,
                to,
                viewport,
                direction: 1,
                background_rgb: 0xff00ff,
            };
            let phase_started = Instant::now();
            let cursor_before_prepare = cursor()?;
            if !carousel.prepare(std::slice::from_ref(&spec))? {
                return Err("production carousel refused the owned presentation".into());
            }
            let prepare_us = phase_started.elapsed().as_micros();
            let cursor_after_prepare = cursor()?;
            let apply_started = Instant::now();
            let mut hide_methods = Vec::new();
            for old in &spec.from {
                hide_methods.push(cloak(platform::hwnd(old.window.id), true)?);
            }
            for new in &spec.to {
                hide_methods.push(cloak(platform::hwnd(new.window.id), false)?);
            }
            let apply_us = apply_started.elapsed().as_micros();
            let state_after_apply = source_states();
            let cursor_after_apply = cursor()?;
            let animate_started = Instant::now();
            carousel.animate()?;
            let animate_us = animate_started.elapsed().as_micros();
            let active_after_animate = carousel.is_active();
            let cursor_after_animate = cursor()?;
            let start = Instant::now();
            let mut active_samples = 0;
            let mut app_samples = 0;
            let mut pixels_match = true;
            let mut captured = false;
            let mut frame_timings = Vec::new();
            let mut pointer_moved = cursor_before_prepare != cursor_after_prepare
                || cursor_after_prepare != cursor_after_apply
                || cursor_after_apply != cursor_after_animate;
            while carousel.is_active() {
                let frame_started = Instant::now();
                runtime::pump(8);
                let pump_us = frame_started.elapsed().as_micros();
                let cursor_after_pump = cursor()?;
                let tick_started = Instant::now();
                carousel.tick()?;
                let tick_us = tick_started.elapsed().as_micros();
                let active = carousel.is_active();
                let cursor_after_tick = cursor()?;
                pointer_moved |= cursor_after_pump != cursor_after_prepare
                    || cursor_after_tick != cursor_after_prepare;
                let sample_started = Instant::now();
                if carousel.is_active() {
                    unsafe {
                        DwmFlush()?;
                    }
                    let pixels = common::read_pixels(capture_rect)?;
                    pixels_match &= gap_pixels(&pixels) == expected;
                    app_samples += usize::from(app_pixels_present(&pixels));
                    active_samples += 1;
                    if !captured && start.elapsed() >= Duration::from_millis(70) {
                        common::write_bitmap(
                            capture_rect,
                            &pixels,
                            &output.join(format!("{name}-during.bmp")),
                        )?;
                        captured = true;
                    }
                }
                frame_timings.push(serde_json::json!({"pump_us":pump_us,"tick_us":tick_us,"active_after_tick":active,
                    "cursor_after_pump":cursor_after_pump,"cursor_after_tick":cursor_after_tick,
                    "sample_us":sample_started.elapsed().as_micros(),"animation_elapsed_us":animate_started.elapsed().as_micros()}));
            }
            let retired_elapsed_us = animate_started.elapsed().as_micros();
            let source_states_on_retirement = source_states();
            unsafe {
                DwmFlush()?;
            }
            let after_pixels = common::read_pixels(capture_rect)?;
            let after_matches = gap_pixels(&after_pixels) == expected;
            common::write_bitmap(
                capture_rect,
                &after_pixels,
                &output.join(format!("{name}-after.bmp")),
            )?;
            let passed = active_samples >= 3
                && app_samples > 0
                && pixels_match
                && after_matches
                && !pointer_moved
                && retired_elapsed_us >= 200_000;
            let outcome = if pointer_moved {
                "interrupted-pointer-movement"
            } else if passed {
                "passed"
            } else {
                "incomplete-or-mismatched-presentation"
            };
            cases.push(serde_json::json!({"case":name,"active_samples":active_samples,"moving_app_samples":app_samples,
                "prepare_us":prepare_us,"apply_us":apply_us,"animate_us":animate_us,"active_after_animate":active_after_animate,
                "retired_elapsed_us":retired_elapsed_us,"pointer_movement_observed":pointer_moved,
                "cursor_before_prepare":cursor_before_prepare,"cursor_after_prepare":cursor_after_prepare,
                "cursor_after_apply":cursor_after_apply,"cursor_after_animate":cursor_after_animate,
                "source_states_on_retirement":source_states_on_retirement,
                "source_states_after_apply":state_after_apply,"hide_methods":hide_methods,"frame_timings":frame_timings,
                "gap_pixels_per_sample":expected.len(),"during_pixels_exact":if active_samples>0 {Some(pixels_match)} else {None},
                "after_pixels_exact":after_matches,"outcome":outcome,"passed":passed}));
        }
        drop(carousel);
        drop(sources);
        report["carousel_cases"] = serde_json::json!(cases);
        report["initial_hide_method"] = serde_json::json!(initial_hide);
        report["carousel_passed"] =
            serde_json::json!(cases.iter().all(|case| case["passed"] == true));
    }
    report["foreground_unchanged"] =
        serde_json::json!(foreground == unsafe { GetForegroundWindow() });
    drop(thumbnail);
    drop(window);
    std::fs::write(output.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string(&report)?);
    if report["foreground_unchanged"] != true
        || (exercise_carousel && report["carousel_passed"] != true)
    {
        return Err("the owned presentation was interrupted, incomplete, or mismatched; see per-case evidence".into());
    }
    Ok(())
}
