#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    let count = std::env::args().nth(1).and_then(|value| value.parse().ok()).unwrap_or(4);
    if let Err(error) = moon_win::platform::run_test_windows(count) {
        eprintln!("testwin: {error}");
        std::process::exit(1);
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("testwin requires Windows 11");
    std::process::exit(1);
}
