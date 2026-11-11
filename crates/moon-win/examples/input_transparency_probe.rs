#[cfg(not(windows))]
fn main() {
    eprintln!("This probe runs on Windows.");
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_core::{
        config::Config,
        world::{Event, World, reduce},
    };
    use moon_win::{platform, runtime, surfaces::Surfaces};
    use std::{
        collections::BTreeMap,
        time::{Duration, Instant},
    };
    use windows::Win32::{
        Foundation::*,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    };
    let _com = platform::Apartment::new()?;
    let _ = platform::enable_dpi_awareness();
    let executable = std::env::args().nth(1).ok_or("pass the path to testwin.exe")?;
    let child = std::process::Command::new(executable).arg("1").spawn()?;
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let child = Child(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    let info = loop {
        if let Some(info) = platform::enumerate_windows()?
            .into_iter()
            .find(|w| w.process_id == child.0.id() && w.class == "MoonTestWindow")
        {
            break info;
        }
        if Instant::now() >= deadline {
            return Err("test receiver did not open".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let target = platform::hwnd(info.id);
    let original_pointer = platform::cursor_position()?;
    struct Pointer(moon_core::geometry::Point);
    impl Drop for Pointer {
        fn drop(&mut self) {
            let _ = platform::cursor_jump(self.0);
        }
    }
    let _pointer = Pointer(original_pointer);
    let mut config = Config::default();
    config.bar.enabled = false;
    let world =
        reduce(&World::new(config, platform::monitors()?)?, Event::WindowAdded(info))?.world;
    let placement = world.plan().placements.into_iter().next().ok_or("no placement")?;
    platform::place(placement.id, placement.rect)?;
    let mut surfaces = Surfaces::new()?;
    surfaces.update(&world, &BTreeMap::new(), None)?;
    runtime::pump(100);
    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid == std::process::id() && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            let mut class = [0u16; 64];
            let count = unsafe { GetClassNameW(hwnd, &mut class) };
            if String::from_utf16_lossy(&class[..count.max(0) as usize]) == "moon.surface" {
                unsafe {
                    (*(param.0 as *mut Vec<HWND>)).push(hwnd);
                }
            }
        }
        true.into()
    }
    let mut borders = Vec::<HWND>::new();
    unsafe {
        EnumWindows(Some(collect), LPARAM(&mut borders as *mut _ as isize))?;
    }
    if borders.is_empty() {
        return Err("no border surface found".into());
    }
    // Cover the ring's physical pixels with the receiver, then deliberately put
    // the underlay above it to prove input routing, independent of normal z-order.
    platform::place(placement.id, placement.rect.outset(64))?;
    platform::focus(placement.id)?;
    runtime::pump(100);
    let rect = placement.rect;
    let points = [
        ("left", moon_core::geometry::Point::new(rect.x - 2, rect.y + rect.height / 2)),
        ("right", moon_core::geometry::Point::new(rect.right() + 2, rect.y + rect.height / 2)),
        ("top", moon_core::geometry::Point::new(rect.x + rect.width / 2, rect.y - 2)),
        ("bottom", moon_core::geometry::Point::new(rect.x + rect.width / 2, rect.bottom() + 2)),
    ];
    let mut results = Vec::new();
    for (edge, point) in points {
        let border = borders
            .iter()
            .copied()
            .find(|&hwnd| unsafe {
                let mut bounds = RECT::default();
                GetWindowRect(hwnd, &mut bounds).is_ok()
                    && point.x >= bounds.left
                    && point.x < bounds.right
                    && point.y >= bounds.top
                    && point.y < bounds.bottom
            })
            .ok_or_else(|| format!("no rendered strip covers {edge} edge"))?;
        unsafe {
            let style = GetWindowLongPtrW(border, GWL_EXSTYLE) as u32;
            if style & (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0)
                != WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0
            {
                return Err("border is missing cross-process input transparency styles".into());
            }
            SetWindowPos(
                target,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )?;
            SetWindowPos(
                border,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )?;
        }
        runtime::pump(100);
        let before = unsafe { GetWindowLongPtrW(target, GWLP_USERDATA) };
        platform::cursor_jump(point)?;
        let before_hit = unsafe { WindowFromPoint(POINT { x: point.x, y: point.y }) };
        let mut hit_class = [0u16; 128];
        let hit_length = unsafe { GetClassNameW(before_hit, &mut hit_class) };
        let hit_class = String::from_utf16_lossy(&hit_class[..hit_length.max(0) as usize]);
        if before_hit != target && before_hit != border {
            return Err(format!("probe aborted before input: edge={edge}, hit_class={hit_class}, hit={before_hit:?}, target={target:?}, border={border:?}, disabled={}, receiver_rect={:?}, point={point:?}", !unsafe { IsWindowEnabled(border) }.as_bool(), platform::window_rect(placement.id)?).into());
        }
        let inputs = [MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP].map(|flags| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: flags, ..Default::default() } },
        });
        if unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) } != 2 {
            return Err("SendInput did not inject both mouse events".into());
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let after = loop {
            runtime::pump(10);
            let after = unsafe { GetWindowLongPtrW(target, GWLP_USERDATA) };
            if after > before || Instant::now() >= deadline {
                break after;
            }
        };
        let hit = unsafe { WindowFromPoint(POINT { x: point.x, y: point.y }) };
        results.push(serde_json::json!({"edge":edge,"receiver_clicked":after == before + 1,"window_from_point_is_receiver":hit == target,"before_hit_class":hit_class,"underlay_enabled":unsafe{IsWindowEnabled(border)}.as_bool(),"underlay_style":unsafe{GetWindowLongPtrW(border,GWL_STYLE)},"underlay_ex_style":unsafe{GetWindowLongPtrW(border,GWL_EXSTYLE)},"receiver_rect":platform::window_rect(placement.id)?}));
        if after != before + 1 {
            return Err(format!(
                "cross-process input failed: {}",
                serde_json::to_string(&results)?
            )
            .into());
        }
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
