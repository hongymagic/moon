#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(any(windows, test))]
mod actions;
#[cfg(any(windows, test))]
mod application;
#[cfg(windows)]
mod clipboard;
#[cfg(any(windows, test))]
mod library;
#[cfg(any(windows, test))]
mod library_bridge;
#[cfg(windows)]
mod library_files;
#[cfg(windows)]
mod native;
#[cfg(any(windows, test))]
mod preview_layout;
#[cfg(windows)]
mod previews;
#[cfg(any(windows, test))]
mod startup;
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
