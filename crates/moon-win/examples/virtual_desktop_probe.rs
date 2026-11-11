//! Read-only comparison of per-query COM activation and the apartment cache.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_win::{platform, runtime};
    use serde_json::json;
    use std::time::Instant;
    use windows::Win32::{
        System::Com::*,
        UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager},
    };

    let apartment = platform::Apartment::new()?;
    let ids: Vec<_> = platform::enumerate_windows()?.into_iter().take(8).map(|w| w.id).collect();
    if ids.is_empty() {
        return Err("no top-level window is available for a read-only desktop query".into());
    }
    let baseline = |id| -> windows::core::Result<bool> {
        let manager: IVirtualDesktopManager =
            unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_INPROC_SERVER)? };
        unsafe { manager.IsWindowOnCurrentVirtualDesktop(platform::hwnd(id)).map(|v| v.as_bool()) }
    };
    for &id in &ids {
        if baseline(id)? != platform::window_on_current_virtual_desktop(id)? {
            return Err("desktop membership changed during warmup".into());
        }
    }
    let initial = runtime::resource_stats()?;
    let mut results = Vec::new();
    for cache in [false, true] {
        let mut samples = Vec::new();
        for _ in 0..256 {
            let start = Instant::now();
            for &id in &ids {
                if cache {
                    platform::window_on_current_virtual_desktop(id)?;
                } else {
                    baseline(id)?;
                }
            }
            samples.push(start.elapsed().as_nanos() as f64 / 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        results.push(json!({"cached":cache,"batches":samples.len(),"queries_per_batch":ids.len(),"median_us":samples[samples.len()/2],"p99_us":samples[samples.len()*99/100],"resources":runtime::resource_stats()?}));
    }
    let nested = platform::Apartment::new()?;
    drop(nested);
    platform::window_on_current_virtual_desktop(ids[0])?;
    platform::invalidate_virtual_desktop_manager();
    platform::window_on_current_virtual_desktop(ids[0])?;
    drop(apartment);
    let _new_apartment = platform::Apartment::new()?;
    platform::window_on_current_virtual_desktop(ids[0])?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"initial":initial,"results":results,"nested_guard_and_reinitialization":true,"explicit_invalidation":true,"windows_modified":false})
        )?
    );
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("virtual_desktop_probe requires Windows");
}
