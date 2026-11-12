#[path = "../windows-resources.rs"]
mod windows_resources;

fn main() {
    // Settings always runs asInvoker without UIAccess, independently of daemon features.
    windows_resources::embed("moon-settings", "Moon Settings", "../moon/app.manifest", false);
}
