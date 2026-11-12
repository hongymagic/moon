set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

bootstrap:
    rustup target add x86_64-pc-windows-msvc
    cargo install --locked cargo-xwin --version 0.23.1
    cargo install --locked cargo-deny --version 0.20.2
    cargo install --locked cargo-audit --version 0.23.0
    XWIN_ARCH=x86_64 cargo xwin cache xwin

test:
    cargo test --workspace --locked

settings:
    cd apps/settings && bun install --frozen-lockfile && bun run check && bun run build

settings-test: settings
    cd apps/settings && bunx playwright install chromium --only-shell && bun run test

check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo deny check
    cargo audit
    just test
    cargo xwin clippy --workspace --all-targets --target x86_64-pc-windows-msvc --locked -- -D warnings

build:
    cargo xwin build --workspace --target x86_64-pc-windows-msvc --locked

release:
    cargo xwin build --workspace --release --target x86_64-pc-windows-msvc --locked

run *args="doctor": build
    bash scripts/windows.sh {{args}}

restore:
    bash scripts/windows.sh restore

takeover: build
    bash scripts/windows.sh start --takeover

integration: build
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/integration.ps1")" -BuildDirectory "$(wslpath -w "$PWD/target/x86_64-pc-windows-msvc/debug")"

install:
    just settings
    cargo xwin build -p moon --features uiaccess --release --target x86_64-pc-windows-msvc --locked
    cargo xwin build -p moon-settings --release --target x86_64-pc-windows-msvc --locked
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/install.ps1")" -Binary "$(wslpath -w "$PWD/target/x86_64-pc-windows-msvc/release/moon.exe")"

handover:
    just settings
    cargo xwin build -p moon --features uiaccess --release --target x86_64-pc-windows-msvc --locked
    cargo xwin build -p moon-settings --release --target x86_64-pc-windows-msvc --locked
    cargo xwin build -p moon-win --example window_inventory --example recover_windows --release --target x86_64-pc-windows-msvc --locked
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/handover.ps1")" -Binary "$(wslpath -w "$PWD/target/x86_64-pc-windows-msvc/release/moon.exe")"

soak:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/record-soak.ps1")"

# Package already-verified inputs; this recipe neither builds nor installs.
package binary settings inventory recovery output version="0.1.0":
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/package.ps1")" -Binary "$(wslpath -w {{quote(binary)}})" -SettingsBinary "$(wslpath -w {{quote(settings)}})" -InventoryBinary "$(wslpath -w {{quote(inventory)}})" -RecoveryBinary "$(wslpath -w {{quote(recovery)}})" -OutputDirectory "$(wslpath -w {{quote(output)}})" -Version {{quote(version)}}
