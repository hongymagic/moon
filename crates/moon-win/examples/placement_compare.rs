//! Compare eight-window async placement with a synchronous deferred batch.
//! Usage: placement_compare.exe C:\path\testwin.exe [samples-per-method]
#[cfg(not(windows))]
fn main() {
    eprintln!("This probe runs on Windows.");
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_core::{geometry::Rect, ids::WindowId};
    use moon_win::platform;
    use serde_json::json;
    use std::{
        process::{Child, Command},
        thread,
        time::{Duration, Instant},
    };
    use windows::Win32::{
        Foundation::RECT,
        Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
        UI::WindowsAndMessaging::*,
    };

    struct OwnedWindows(Child);
    impl Drop for OwnedWindows {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct RestoreFocus(Option<WindowId>);
    impl Drop for RestoreFocus {
        fn drop(&mut self) {
            if let Some(id) = self.0.filter(|id| platform::is_window(*id)) {
                let _ = platform::focus(id);
            }
        }
    }
    fn visible_frame(id: WindowId) -> windows::core::Result<Rect> {
        let mut frame = RECT::default();
        unsafe {
            DwmGetWindowAttribute(
                platform::hwnd(id),
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut frame as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            )?;
        }
        Ok(Rect::new(frame.left, frame.top, frame.right - frame.left, frame.bottom - frame.top))
    }
    fn deferred(targets: &[(WindowId, Rect)]) -> windows::core::Result<()> {
        unsafe {
            let mut positions = Vec::with_capacity(targets.len());
            for &(id, target) in targets {
                let handle = platform::hwnd(id);
                let frame = visible_frame(id)?;
                let mut outer = RECT::default();
                GetWindowRect(handle, &mut outer)?;
                let left = frame.x - outer.left;
                let top = frame.y - outer.top;
                let right = outer.right - frame.right();
                let bottom = outer.bottom - frame.bottom();
                positions.push((
                    handle,
                    Rect::new(
                        target.x - left,
                        target.y - top,
                        target.width + left + right,
                        target.height + top + bottom,
                    ),
                ));
            }
            let mut batch = BeginDeferWindowPos(positions.len() as i32)?;
            for (handle, position) in positions {
                // Every call can replace the HDWP. On failure, abandon it;
                // Microsoft explicitly forbids calling EndDeferWindowPos then.
                batch = DeferWindowPos(
                    batch,
                    handle,
                    None,
                    position.x,
                    position.y,
                    position.width,
                    position.height,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
                )?;
            }
            EndDeferWindowPos(batch)
        }
    }
    fn verify(targets: &[(WindowId, Rect)]) -> windows::core::Result<bool> {
        for &(id, target) in targets {
            if visible_frame(id)? != target {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn distribution(values: &[u64]) -> serde_json::Value {
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        let percentile = |p: usize| sorted[(sorted.len() * p).div_ceil(100).saturating_sub(1)];
        json!({"count":sorted.len(),"median_us":percentile(50),"p95_us":percentile(95),"p99_us":percentile(99),"max_us":sorted.last()})
    }

    let path = std::env::args().nth(1).ok_or("pass the path to testwin.exe")?;
    let samples: usize = std::env::args().nth(2).map(|s| s.parse()).transpose()?.unwrap_or(200);
    if !(20..=2000).contains(&samples) || !samples.is_multiple_of(2) {
        return Err("samples per method must be even and between 20 and 2000".into());
    }
    let _apartment = platform::Apartment::new()?;
    let _ = platform::enable_dpi_awareness();
    let _focus = RestoreFocus(platform::foreground());
    let monitor = platform::monitors()?.into_iter().next().ok_or("no monitor")?;
    let area = monitor.work_area;
    if area.width < 800 || area.height < 500 {
        return Err("comparison requires a work area at least 800 by 500 physical pixels".into());
    }
    let child = OwnedWindows(Command::new(path).arg("8").spawn()?);
    let deadline = Instant::now() + Duration::from_secs(10);
    let windows = loop {
        let mut windows: Vec<_> = platform::enumerate_windows()?
            .into_iter()
            .filter(|window| window.process_id == child.0.id() && window.class == "MoonTestWindow")
            .collect();
        windows.sort_by_key(|window| window.id);
        if windows.len() == 8 {
            break windows;
        }
        if Instant::now() >= deadline {
            return Err("helper did not create eight windows".into());
        }
        thread::sleep(Duration::from_millis(20));
    };
    // Both layouts change every frame. They remain non-overlapping and entirely
    // inside the same monitor, with enough room for normal window minimum sizes.
    let targets: [Vec<_>; 2] = std::array::from_fn(|phase| {
        windows
            .iter()
            .enumerate()
            .map(|(index, window)| {
                let cell_width = (area.width - 48) / 4;
                let cell_height = (area.height - 48) / 2;
                (
                    window.id,
                    Rect::new(
                        area.x + 16 + (index as i32 % 4) * cell_width + phase as i32 * 8,
                        area.y + 16 + (index as i32 / 4) * cell_height + phase as i32 * 8,
                        cell_width - 24 - phase as i32 * 8,
                        cell_height - 24 - phase as i32 * 8,
                    ),
                )
            })
            .collect()
    });
    let mut submitted = [Vec::new(), Vec::new()];
    let mut settled = [Vec::new(), Vec::new()];
    let warmup = 20;
    for index in 0..(samples * 2 + warmup) {
        // ABBA balances first/second layout and order effects for both methods.
        let method = usize::from(matches!(index % 4, 1 | 2));
        let target = &targets[index % 2];
        let started = Instant::now();
        if method == 0 {
            for &(id, rect) in target {
                platform::place(id, rect)?;
            }
        } else {
            deferred(target)?;
        }
        let submit_us = started.elapsed().as_micros() as u64;
        let deadline = started + Duration::from_secs(2);
        while !verify(target)? {
            if Instant::now() >= deadline {
                let actual: Vec<_> = target
                    .iter()
                    .map(|&(id, wanted)| (id, wanted, visible_frame(id).ok()))
                    .collect();
                return Err(format!("method {method} placement timeout: {actual:?}").into());
            }
            thread::yield_now();
        }
        if index >= warmup {
            submitted[method].push(submit_us);
            settled[method].push(started.elapsed().as_micros() as u64);
        }
        thread::sleep(Duration::from_millis(10));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "windows":8,
            "samples_per_method":samples,
            "warmup_per_method":warmup/2,
            "monitor":monitor,
            "all_frames_exact":true,
            "measurement":"Call entry to submission, then exact DWM frame readback for all eight windows; excludes input hooks and compositor presentation",
            "flicker":"Not measured: frame coordinates do not establish visual flicker or paint completion",
            "async_set_window_pos":{"submission":distribution(&submitted[0]),"settled":distribution(&settled[0]),"submission_samples_us":submitted[0],"settled_samples_us":settled[0]},
            "defer_window_pos":{"submission":distribution(&submitted[1]),"settled":distribution(&settled[1]),"submission_samples_us":submitted[1],"settled_samples_us":settled[1]}
        }))?
    );
    Ok(())
}
