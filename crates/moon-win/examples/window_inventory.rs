//! Read-only inventory for enumeration parity and stranded-window diagnostics.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_core::{command::Command, config::Config};
    use moon_win::platform;
    use serde_json::json;

    let _apartment = platform::Apartment::new()?;
    let _ = platform::enable_dpi_awareness();
    let config = Config::default();
    let mut windows = Vec::new();
    for info in platform::enumerate_windows()? {
        let mut commands = Vec::new();
        for rule in &config.rules {
            if rule.matches.iter().any(|m| m.matches(&info.process, &info.class, &info.title)) {
                for command in &rule.commands {
                    commands.extend(config.commands(command)?);
                }
            }
        }
        windows.push(json!({
            "window": info,
            "process_birth": moon_win::runtime::process_birth(info.process_id).ok(),
            "cloaked": platform::cloaked(info.id).ok(),
            "on_current_virtual_desktop": platform::window_on_current_virtual_desktop(info.id).ok(),
            "ignored_by_defaults": commands.contains(&Command::Ignore),
            "matching_default_commands": commands,
            "token": platform::token_capabilities(info.process_id).ok(),
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"monitors":platform::monitors()?,"windows":windows}))?
    );
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("window_inventory requires Windows");
}
