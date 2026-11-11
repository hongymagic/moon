//! Read-only startup/session guard diagnostics. Does not lock or switch desktops.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_win::{platform, runtime};
    use serde_json::json;

    let state = platform::session_state()?;
    let desktop = platform::input_desktop_available()?;
    let interactive = platform::session_interactive()?;
    let initial = runtime::resource_stats()?;
    for _ in 0..64 {
        platform::session_state()?;
        platform::input_desktop_available()?;
    }
    let after = runtime::resource_stats()?;
    if after.handles != initial.handles {
        return Err(format!(
            "Read-only session checks leaked handles: {} -> {}",
            initial.handles, after.handles
        )
        .into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "session":state,"input_desktop_available":desktop,"session_interactive":interactive,
            "repeated_checks":64,"initial_handles":initial.handles,"final_handles":after.handles,
            "desktop_or_session_modified":false,
        }))?
    );
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("session_probe requires Windows");
}
