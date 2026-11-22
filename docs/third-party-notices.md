# Third-party notices

Moon's original source, documentation and crescent artwork use the MIT license. The following components ship under their respective upstream terms.

| Component | Where it is used | Notices |
| --- | --- | --- |
| Symbols Nerd Font 3.5.1 | Private-use glyphs in the native bar and the Settings icon picker | The ZIP includes `licenses/Symbols-LICENSE.txt` and `licenses/Symbols-NOTICES.txt`. See the [pinned upstream font and icon-set credits](https://github.com/ryanoasis/nerd-fonts/tree/v3.5.1/patched-fonts/NerdFontsSymbolsOnly). |
| Rust crates | Native applications and Windows helpers | [Crate versions and license texts](rust-licenses.md) and [supplemental copyright notices](rust-copyrights.md) |
| Rust standard library | Native Windows binaries | The release ZIP includes the toolchain’s `licenses/Rust-library.html`. See the [Rust project copyright](https://github.com/rust-lang/rust/blob/1.100.0/COPYRIGHT) and [license texts](https://github.com/rust-lang/rust/tree/1.100.0/LICENSES) |
| Microsoft WebView2 SDK loader | Launches the shared Evergreen Runtime for Settings | [SDK license and notices](webview2-notices.md) |

The [website notices](../apps/website/public/notices.txt) cover the lunar mosaic, fonts, icons and UI libraries used on moon.hongy.dev.

The Rust license list is generated from the locked Windows dependency graph. When updating dependencies, regenerate it from the checkout root and check upstream notices for crates that omit license files from their published packages.

```sh
cargo install --locked cargo-about --version 0.9.3
cargo about generate --workspace --locked --fail --config scripts/licenses/about.toml \
  scripts/licenses/rust-notices.hbs -o docs/rust-licenses.md
```
