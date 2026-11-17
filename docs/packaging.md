# Windows distribution package

GitHub Actions publishes a Windows x64 release after a push to `main` passes CI.
The tag uses `YYYY.MM.DD-n`, with the date in Australia/Sydney when the
release job selects its version. The first release that day starts at `0`.
The ZIP, manifest, checksum, Windows product/file versions and Moon CLI use
the same version. Local builds default to the Cargo workspace version.

The release queue serializes version selection and publication. Packaging verifies
the ZIP before the job creates its Git tag. A rerun reuses the tag for that commit,
finishes a pending draft and compares the uploaded asset names and SHA-256
digests to the local files before publishing. A completed release is skipped on
reruns. If CI finishes out of merge order, the newest released commit on
`main` retains GitHub's **Latest** designation.

`scripts/package.ps1` packages already-built, explicitly selected executables.
It never launches them, builds Rust code, requests elevation, changes certificate
stores, or installs Moon. Run it using Windows PowerShell 5.1 or newer:

```powershell
.\scripts\package.ps1 -Binary C:\build\moon.exe `
  -SettingsBinary C:\build\moon-settings.exe `
  -InventoryBinary C:\build\examples\window_inventory.exe `
  -RecoveryBinary C:\build\examples\recover_windows.exe `
  -RustLibraryNotices (Join-Path (rustc --print sysroot) 'share/doc/rust/COPYRIGHT-library.html') `
  -Version 0.1.0 -OutputDirectory C:\releases
```

From WSL, `just package <moon> <settings> <inventory> <recovery> <output> [version]`
converts these explicit paths for Windows. The default version is `0.1.0`.
Build the frontend before building Settings, the daemon with `--features
uiaccess`, and the inventory/recovery examples for `x86_64-pc-windows-msvc`.
The installer signs its protected copies locally; packaging does not require or
create a machine-trusted signing certificate.

The complete folder and ZIP contain **Install Moon.cmd**, the setup/install/
uninstall/common/handover/verification scripts, both application executables,
the two recovery helpers, all current Markdown/JSON documentation, the Symbols
font notices, and the standalone `examples/workspace-state.ps1` IPC extension.
GitHub release bundles include the compiler’s standard-library notices under
`licenses/Rust-library.html`. The Evergreen WebView2 runtime is a separately detected prerequisite; the
WebView2 loader is statically linked by the pinned Rust dependency.

Validation requires x64 PE application images, matching product/version/original
filename resources and multi-size icon resource 101, `asInvoker` manifests,
`PerMonitorV2`, UIAccess for the daemon and ordinary permissions for Settings.
The ordinary and delay-import tables of all four executables are checked for
external Visual C++ runtime DLLs; strict packaging requires the repository's
`+crt-static` Windows build. The manifest records each executable's imported DLLs.
Every PowerShell payload is parsed before publication. A complete staging folder
can instead be supplied with `-StageDirectory`; unexpected files and links are
rejected. If it already contains `package-manifest.json`, every recorded hash,
size, entry, and package identity is verified before repackaging.

The output is `moon-<version>-windows-x64/`, the corresponding `.zip`, an
external `.manifest.json`, and `.zip.sha256`. The internal manifest lists sorted
payload paths, exact lengths, SHA-256 hashes, and executable identities. It does
not attempt to hash itself; the ZIP hash covers the manifest too. The archive is
reopened and every entry hashed before any final artifact is published. Existing
outputs are never overwritten. Cleanup attempts every temporary and partially
published path owned by the failed invocation; a locked leftover is reported
without masking the original failure or skipping the remaining paths.

Manifest JSON uses ordered fields, ordinal path ordering, UTF-8 without a BOM,
and LF line endings. ZIP entries have fixed 1980 timestamps and the same sorted
ordering. Identical inputs produce identical output on the same .NET compressor
version; compressed bytes are not promised to match across runtime versions.
Checksums detect changes, but do not authenticate the publisher on their own.

`-Development` permits a non-UIAccess daemon and external Visual C++ runtime
dependencies for packaging tests. It still validates the executables and requires
ordinary-permission Settings. It labels the directory,
archive, manifest, and README as development material, unsuitable for guided
installation. Use this only for packaging tests; final installation bundles
must pass the default strict checks.

Current official references checked 2026-11-12:

- [ZIP entry creation](https://learn.microsoft.com/en-us/dotnet/api/system.io.compression.ziparchive.createentry)
  and [timestamp semantics](https://learn.microsoft.com/en-us/dotnet/api/system.io.compression.ziparchiveentry.lastwritetime).
- [PowerShell file and stream SHA-256 hashing](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/get-filehash).
- [WebView2 runtime distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution).
- [Portable Executable sections and import directories](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format).
- [Shell-safe recipe quoting](https://just.systems/man/en/functions.html).

`scripts/validate-package.ps1` passed 14 isolated Windows checks on 2026-11-12
with development inputs. The checks covered archive readback, deterministic
repackaging, output preservation, missing and malformed inputs, version and
permission declarations, external VC runtime rejection, staged tamper detection,
failure cleanup with a locked file, and unchanged original inputs. No application
executable was started and no installation was changed. This validates packaging
behavior; application acceptance is recorded separately in
[acceptance-status.md](acceptance-status.md).

The final 0.1.0 build passed strict input preflight on 2026-11-12: both application
identities, versions, icons, DPI declarations and execution permissions matched;
all four executables had no external Visual C++ runtime imports. The daemon
SHA-256 was `9B85EBEEFD8404AC8E1FCB2F5395883D81CF1ADD8E7CCFE79488F7DD503B1546`
and Settings was
`7901563646D88335A3882DC0604691F5868D5F4BEA625D9C0D1479368A7BE050`.
The published package manifest records the exact payload hashes; the release
report records archive readback and the external ZIP checksum. Preflight does
not substitute for those publication checks or the application acceptance gates.
