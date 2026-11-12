#[path = "../windows-resources.rs"]
mod windows_resources;

fn main() {
    windows_resources::embed(
        "moon",
        "Moon",
        "app.manifest",
        std::env::var_os("CARGO_FEATURE_UIACCESS").is_some(),
    );
}
