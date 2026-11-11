//! Explicit recovery of selected handles from a previously captured inventory.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_core::{ids::WindowId, world::WindowInfo};
    use moon_win::platform;
    use serde_json::{Value, json};

    let mut arguments = std::env::args().skip(1);
    let path = arguments.next().ok_or("expected inventory JSON path followed by decimal HWNDs")?;
    let source: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let recorded = source["native"]["windows"].as_array().ok_or("missing native inventory")?;
    let ids = arguments.map(|s| s.parse::<u64>().map(WindowId)).collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Err("select at least one explicitly validated HWND".into());
    }
    let _apartment = platform::Apartment::new()?;
    let _ = platform::enable_dpi_awareness();
    let mut verified = Vec::new();
    for id in ids {
        let original = recorded
            .iter()
            .find(|entry| entry["window"]["id"] == json!(id))
            .ok_or("HWND absent from recorded inventory")?;
        let birth =
            original["process_birth"].as_u64().ok_or("missing process creation identity")?;
        let original: WindowInfo = serde_json::from_value(original["window"].clone())?;
        let current = platform::window_info(id)?;
        if current.process_id != original.process_id
            || current.process != original.process
            || current.class != original.class
            || !moon_win::runtime::process_matches(current.process_id, birth)?
            || !platform::window_on_current_virtual_desktop(id)?
        {
            return Err(format!("Window identity or native desktop changed for {id}").into());
        }
        verified.push(original);
    }
    let mut result = Vec::new();
    for window in verified {
        platform::restore(window.id, window.rect)?;
        result.push(json!({"id":window.id,"process":window.process,"cloaked":platform::cloaked(window.id)?}));
    }
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("recover_windows requires Windows");
}
