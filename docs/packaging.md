# Windows distribution package

GitHub Actions publishes a Windows x64 release after a push to `main` passes CI.
The tag uses `YYYY.MM.DD-n`, with the date in Australia/Sydney when the
release job selects its version. The first release that day starts at `0`.
The setup EXE, ZIP and their checksums carry the release tag in their names.
The package manifest, Explorer’s product version and the Moon CLI report the
same version. Local builds default to the Cargo workspace version.

The release queue serializes version selection and publication. Packaging verifies
both downloads before the job creates its Git tag. A rerun reuses the tag for that commit,
finishes a pending draft and compares the uploaded asset names and SHA-256
digests to the local files before publishing. A completed release is skipped on
reruns. If CI finishes out of merge order, the newest released commit on
`main` retains GitHub's **Latest** designation.

`scripts/package.ps1` packages already-built, explicitly selected executables.
It never launches them, builds Rust code, requests elevation, changes certificate
stores, or installs Moon. Run it using Windows PowerShell 5.1 or newer:

```powershell
$package = .\scripts\package.ps1 -Binary C:\build\moon.exe `
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

Release packaging writes `moon-<version>-windows-x64-setup.exe`,
`moon-<version>-windows-x64.zip`, a `.sha256` file for each download,
the extracted package directory and an external `.manifest.json`. The internal manifest lists sorted
payload paths, exact lengths, SHA-256 hashes, and executable identities. It does
not attempt to hash itself; the ZIP hash covers the manifest too. The archive is
reopened and every entry hashed before any final artifact is published. Existing
outputs are never overwritten. Cleanup attempts every temporary and partially
published path owned by the failed invocation; a locked leftover is reported
without masking the original failure or skipping the remaining paths.

`scripts/build-setup-exe.ps1` uses the x64 C# compiler supplied with Windows’
.NET Framework. It embeds the verified ZIP in the EXE and gives it Moon’s icon
and an `asInvoker` manifest. Double-clicking the EXE checks the embedded ZIP,
extracts it to a private working folder and opens the same setup window as
**Install Moon.cmd**. Setup asks for Windows approval when installation needs it.
After setup closes, the EXE removes the working folder. If the window crashes
while a worker may still be installing or restoring Moon, the EXE leaves the
folder in the user profile so the worker can finish.

To check the EXE without installing, run the following in PowerShell. Exit code
`0` means the embedded ZIP matched its build-time SHA-256 and contained the
setup entry point. To check the downloaded EXE itself, compare its `Get-FileHash`
result with the release’s `.exe.sha256` file. The download is unsigned.

```powershell
$check = Start-Process -FilePath $package.installer -ArgumentList /VERIFYONLY -PassThru -Wait
$check.ExitCode
```

To run the same native checks as CI, pass the packaging result to the validator.
It compares the EXE’s embedded ZIP with the release ZIP and exercises setup and
cleanup with a harmless test script.

```powershell
.\scripts\validate-setup-exe.ps1 -InstallerPath $package.installer -ArchivePath $package.archive -Version $package.version
```

Manifest JSON uses ordered fields, ordinal path ordering, UTF-8 without a BOM,
and LF line endings. ZIP entries have fixed 1980 timestamps and the same sorted
ordering. Identical inputs produce identical ZIP and manifest bytes on the same .NET compressor
version; compressed bytes are not promised to match across runtime versions.
Checksums detect changes, but do not authenticate the publisher on their own.

`-Development` permits a non-UIAccess daemon and external Visual C++ runtime
dependencies for packaging tests. It still validates the executables and requires
ordinary-permission Settings. It labels the directory,
archive, manifest, and README as development material. Only a strict release
build produces a setup EXE.

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
