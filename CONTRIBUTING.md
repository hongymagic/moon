# Contributing to Moon

For bugs, include the Moon release tag, Windows build (`winver`), and the smallest set of steps that shows the problem. For a layout or rendering issue, include your monitor count, display scaling and relevant configuration. Redact personal window titles and paths before posting. For security issues, use [private reporting](SECURITY.md).

## Build and test

Clone the source and enter the repository.

```sh
git clone https://github.com/hongymagic/moon.git
cd moon
```

Rustup reads the compiler and target from [rust-toolchain.toml](rust-toolchain.toml). You can run the portable tests with `cargo test --workspace --locked`. On Windows x64, install the Visual Studio C++ build tools and Windows SDK, then build and test the native code.

```powershell
cargo build --workspace --target x86_64-pc-windows-msvc --locked
cargo test --workspace --target x86_64-pc-windows-msvc --locked
.\target\x86_64-pc-windows-msvc\debug\moon.exe doctor
```

From WSL, install Rustup and `just`, run `just bootstrap` to install the pinned cross-compilation tools and cache the Windows SDK and CRT, then use `just build` and `just run doctor`. Run `just check` for formatting, lint, dependency checks and the portable test suite. See the [Rust Windows MSVC toolchain guide](https://rust-lang.github.io/rustup/installation/windows-msvc.html) and the [project recipes](justfile).

The native Settings host embeds the TypeScript frontend from `apps/settings/dist`. For UI changes, use the Bun version in [package.json](apps/settings/package.json) and run `just settings-test`. Commit both the source changes and the regenerated assets.

The [marketing website](https://moon.hongy.dev) lives in `apps/website`. From that directory, run `bun install --frozen-lockfile` and `bun run dev` to preview it. Run `bun test`, `bun run check` and `bun run build` before submitting a change. Vercel builds the same directory from GitHub.

## Send a change

Open a pull request against `main`. Describe the behavior before and after your change and how you verified it. For a visual change, include a screenshot or pixel measurement and the display scale you tested. If your change affects window management, describe what happened to the other windows in the workspace and to the desktop when Moon exited. Full desktop integration runs belong on a disposable Windows desktop. See [integration validation](docs/validation-integration.md).

CI checks Rust, native Windows behavior and the Settings UI. After a successful merge to `main`, the release workflow publishes a Windows ZIP with a tag in the form `YYYY.MM.DD-n`. The date uses Australia/Sydney time and the first release of each day ends in `-0`. See [packaging](docs/packaging.md).

Contributions to Moon are licensed under the [MIT license](LICENSE). Keep third-party notices with any contributed assets. Our [code of conduct](CODE_OF_CONDUCT.md) applies to issues, pull requests and reviews.
