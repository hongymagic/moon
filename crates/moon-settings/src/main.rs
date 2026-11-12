#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod native;
#[cfg(any(windows, test))]
mod store;

fn main() {
    #[cfg(windows)]
    if let Err(error) = native::run() {
        native::show_error(&error);
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    eprintln!("Moon Settings runs on Windows 11.");
}
