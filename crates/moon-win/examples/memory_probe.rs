#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_win::{providers::Providers, runtime, surfaces::Surfaces};
    let mode = std::env::args().nth(1).unwrap_or_else(|| "providers".into());
    let initial = runtime::resource_stats()?;
    match mode.as_str() {
        "providers" => {
            let mut providers = Providers::new();
            for _ in 0..5 {
                let _values = providers.sample();
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            println!(
                "{}",
                serde_json::json!({"mode": mode, "initial": initial, "after": runtime::resource_stats()?})
            );
        }
        "graphics" => {
            let _apartment = moon_win::platform::Apartment::new()?;
            let surfaces = Surfaces::new()?;
            for _ in 0..5 {
                runtime::pump(1000);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            println!(
                "{}",
                serde_json::json!({"mode": mode, "backend": surfaces.backend(), "initial": initial, "after": runtime::resource_stats()?})
            );
        }
        _ => return Err("use providers or graphics".into()),
    }
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("memory_probe requires Windows");
    std::process::exit(1);
}
