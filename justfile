set shell := ["bash", "-euo", "pipefail", "-c"]

export MOON_WINDOWS_TARGET := env("MOON_WINDOWS_TARGET", if arch() == "aarch64" { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" })

default:
    @just --list

bootstrap:
    cargo install --locked cargo-xwin --version 0.23.1
    cargo install --locked cargo-deny --version 0.21.0
    cargo install --locked cargo-audit --version 0.23.0
    XWIN_ARCH=x86_64,aarch64 cargo xwin cache xwin

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
    cargo xwin clippy --workspace --all-targets --target "$MOON_WINDOWS_TARGET" --locked -- -D warnings

build:
    cargo xwin build --workspace --target "$MOON_WINDOWS_TARGET" --locked

release:
    cargo xwin build --workspace --release --target "$MOON_WINDOWS_TARGET" --locked

run *args="doctor": build
    bash scripts/windows.sh {{args}}

restore:
    bash scripts/windows.sh restore

takeover: build
    bash scripts/windows.sh start --takeover

integration: build
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/integration.ps1")" -BuildDirectory "$(wslpath -w "$PWD/target/$MOON_WINDOWS_TARGET/debug")"

install:
    just settings
    cargo xwin build -p moon --features uiaccess --release --target "$MOON_WINDOWS_TARGET" --locked
    cargo xwin build -p moon-settings --release --target "$MOON_WINDOWS_TARGET" --locked
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/launch-from-explorer.ps1")" -Mode Install -Binary "$(wslpath -w "$PWD/target/$MOON_WINDOWS_TARGET/release/moon.exe")"

handover keep_previous_desktop_stopped="false":
    @[[ {{quote(keep_previous_desktop_stopped)}} == true || {{quote(keep_previous_desktop_stopped)}} == false ]] || { echo 'Use: just handover [true|false]' >&2; exit 2; }
    just settings
    cargo xwin build -p moon --features uiaccess --release --target "$MOON_WINDOWS_TARGET" --locked
    cargo xwin build -p moon-settings --release --target "$MOON_WINDOWS_TARGET" --locked
    cargo xwin build -p moon-win --example window_inventory --example recover_windows --release --target "$MOON_WINDOWS_TARGET" --locked
    args=(); if [[ {{quote(keep_previous_desktop_stopped)}} == true ]]; then args+=(-KeepPreviousDesktopStopped); fi; powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/launch-from-explorer.ps1")" -Mode Handover -Binary "$(wslpath -w "$PWD/target/$MOON_WINDOWS_TARGET/release/moon.exe")" "${args[@]}"

soak:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/record-soak.ps1")"

# Package already-verified inputs; this recipe neither builds nor installs.
package binary settings inventory recovery output version="0.1.0":
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(wslpath -w "$PWD/scripts/package.ps1")" -Binary "$(wslpath -w {{quote(binary)}})" -SettingsBinary "$(wslpath -w {{quote(settings)}})" -InventoryBinary "$(wslpath -w {{quote(inventory)}})" -RecoveryBinary "$(wslpath -w {{quote(recovery)}})" -OutputDirectory "$(wslpath -w {{quote(output)}})" -Version {{quote(version)}} -Target "$MOON_WINDOWS_TARGET"
