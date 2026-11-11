#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_core::{
        config::Config,
        world::{Event, World, reduce},
    };
    use moon_win::{platform, providers::Providers, runtime, surfaces::Surfaces};
    use std::time::{Duration, Instant};
    let program_started = Instant::now();
    let _apartment = moon_win::platform::Apartment::new()?;
    platform::enable_dpi_awareness()?;
    let monitors = platform::monitors()?;
    let mut config = Config::default();
    let arguments: Vec<_> = std::env::args().collect();
    if arguments.iter().any(|arg| arg == "--bar-only") {
        config.borders.enabled = false;
    }
    if arguments.iter().any(|arg| arg == "--rings-only") {
        config.bar.enabled = false;
    }
    config.workspaces[0].icon = Some("\u{f269}".into());
    if std::env::args().any(|arg| arg == "--missing-font") {
        config.bar.font = "moon intentionally missing font".into();
    }
    let mut world = World::new(config, monitors)?;
    for window in platform::enumerate_windows()? {
        if window.process.eq_ignore_ascii_case("testwin")
            || window.process.eq_ignore_ascii_case("testwin.exe")
        {
            world = reduce(&world, Event::WindowAdded(window))?.world;
        }
    }
    struct Restore(Vec<(moon_core::ids::WindowId, platform::OriginalPlacement)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            for (id, original) in &self.0 {
                let _ = platform::restore_placement(*id, original);
            }
        }
    }
    let originals = world
        .windows
        .keys()
        .map(|id| platform::capture_placement(*id).map(|original| (*id, original)))
        .collect::<windows::core::Result<Vec<_>>>()?;
    let _restore = Restore(originals);
    for placement in world.plan().placements {
        platform::place(placement.id, placement.rect)?;
    }
    if let Some(id) = world.focused {
        platform::focus(id)?;
    }
    let mut surfaces = Surfaces::new()?;
    let threads_before_draw = thread_count()?;
    let mut values = std::collections::BTreeMap::new();
    surfaces.update(&world, &values, None)?;
    let threads_after_draw = thread_count()?;
    if arguments.iter().any(|arg| arg == "--front-inspection") {
        reveal_owned_surfaces(&world)?;
    }
    let ready_ms = program_started.elapsed().as_secs_f64() * 1000.0;
    for iteration in 0..10 {
        let direction = if iteration % 2 == 0 { "focus left" } else { "focus right" };
        world = reduce(&world, Event::Command(direction.parse()?))?.world;
        surfaces.update(&world, &values, None)?;
        runtime::pump(0);
    }
    let before_updates = runtime::resource_stats()?;
    let pixels_before = surfaces.surface_pixel_bytes();
    let mut render_ms = Vec::new();
    let resize = arguments.iter().any(|argument| argument == "--resize");
    let updates = if resize { 128usize } else { 50 };
    for iteration in 0..updates {
        let frame_start = Instant::now();
        let direction = if resize {
            if iteration % 2 == 0 { "resize width +2%" } else { "resize width -2%" }
        } else if iteration % 2 == 0 {
            "focus left"
        } else {
            "focus right"
        };
        world = reduce(&world, Event::Command(direction.parse()?))?.world;
        surfaces.update(&world, &values, None)?;
        render_ms.push(frame_start.elapsed().as_secs_f64() * 1000.0);
        runtime::pump(0);
    }
    let after_updates = runtime::resource_stats()?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut providers = Providers::new();
        while sender.send(providers.sample()).is_ok() {
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    let start = Instant::now();
    let mut cpu_start = None;
    let seconds = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20u64);
    while start.elapsed() < Duration::from_secs(seconds) {
        if cpu_start.is_none() && start.elapsed() >= Duration::from_secs(5) {
            cpu_start = Some((Instant::now(), cpu_ticks()?));
        }
        if let Some(sample) = receiver.try_iter().last() {
            values = sample;
        }
        surfaces.update(&world, &values, None)?;
        if !runtime::pump(1000) {
            break;
        }
    }
    render_ms.sort_by(f64::total_cmp);
    let idle_cpu_one_core_percent = cpu_start
        .map(|(at, ticks)| {
            cpu_ticks()
                .map(|now| (now - ticks) as f64 / 10_000_000.0 / at.elapsed().as_secs_f64() * 100.0)
        })
        .transpose()?;
    println!(
        "{}",
        serde_json::json!({"surfaces_rendered": true, "surface_pixel_bytes_before": pixels_before, "surface_pixel_bytes_after": surfaces.surface_pixel_bytes(), "threads_before_draw": threads_before_draw, "threads_after_draw": threads_after_draw, "threads_final": thread_count()?, "bar": world.config.bar.enabled, "borders": world.config.borders.enabled, "backend": surfaces.backend(), "monitors": world.monitors.len(), "windows": world.windows.len(), "seconds": seconds, "ready_ms": ready_ms, "update_count": updates, "resize": resize, "render_median_ms": render_ms[updates / 2], "render_p99_ms": render_ms[(updates * 99).div_ceil(100) - 1], "idle_cpu_one_core_percent": idle_cpu_one_core_percent, "before_updates": before_updates, "after_updates": after_updates, "resources": runtime::resource_stats()?})
    );
    Ok(())
}
#[cfg(windows)]
fn cpu_ticks() -> windows::core::Result<u64> {
    use windows::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    let (mut created, mut exited, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user)?;
    }
    let ticks =
        |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    Ok(ticks(kernel) + ticks(user))
}
#[cfg(not(windows))]
fn main() {
    eprintln!("surface_smoke requires Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn thread_count() -> windows::core::Result<u32> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::{Diagnostics::ToolHelp::*, Threading::GetCurrentProcessId},
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)?;
        let mut entry =
            THREADENTRY32 { dwSize: size_of::<THREADENTRY32>() as u32, ..Default::default() };
        let mut count = 0;
        let mut found = Thread32First(snapshot, &mut entry).is_ok();
        while found {
            if entry.th32OwnerProcessID == GetCurrentProcessId() {
                count += 1;
            }
            found = Thread32Next(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        Ok(count)
    }
}

#[cfg(windows)]
fn reveal_owned_surfaces(world: &moon_core::world::World) -> windows::core::Result<()> {
    use windows::Win32::{
        Foundation::{HWND, LPARAM},
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::*,
    };
    use windows::core::BOOL;
    unsafe extern "system" fn find(hwnd: HWND, param: LPARAM) -> BOOL {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == GetCurrentProcessId()
                && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT.0 != 0
            {
                (*(param.0 as *mut Vec<HWND>)).push(hwnd);
            }
            BOOL(1)
        }
    }
    unsafe {
        let mut windows = Vec::<HWND>::new();
        EnumWindows(Some(find), LPARAM((&mut windows as *mut Vec<HWND>) as isize))?;
        for hwnd in
            windows.into_iter().chain(world.windows.keys().map(|id| moon_win::platform::hwnd(*id)))
        {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )?;
        }
    }
    Ok(())
}
