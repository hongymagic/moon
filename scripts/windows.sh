#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
destination="$(powershell.exe -NoProfile -Command '[Environment]::GetFolderPath("LocalApplicationData")' | tr -d '\r')\\moon-dev"
directory="$(wslpath -u "$destination")"
mkdir -p "$directory"
# Copy only when the daemon is not using the file. Running clients can reuse it.
if ! cmp -s target/x86_64-pc-windows-msvc/debug/moon.exe "$directory/moon.exe"; then
  cp target/x86_64-pc-windows-msvc/debug/moon.exe "$directory/moon.exe"
fi
# The daemon opens Settings beside its own executable. Keep the development
# companion current, while allowing recovery without a Settings build.
settings="target/x86_64-pc-windows-msvc/debug/moon-settings.exe"
if [[ "${1:-}" != restore && -f "$settings" ]] && ! cmp -s "$settings" "$directory/moon-settings.exe"; then
  cp "$settings" "$directory/moon-settings.exe"
fi
exec "$directory/moon.exe" "$@"
