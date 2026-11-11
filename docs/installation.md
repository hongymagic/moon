# Installation and recovery

The personal installer builds on Windows' documented UIAccess requirements:
the executable has an `asInvoker`, `uiAccess=true` manifest, an Authenticode
signature trusted by this machine, and an installation under Program Files.
The installer protects both the directory and individual security metadata,
rejects reparse points, and limits write access to administrative identities.
Its RSA signing key is non-exportable and its certificate has only the code
signing extended key usage. The key stays in the local machine certificate
store; it is never exported to the repository or user profile.

The logon task uses `Limited`, interactive execution for the owning user.
Its small PowerShell action uses ShellExecute to let Windows apply the signed
UIAccess manifest. Applications launched by moon go through Explorer's ordinary
shell; the verification script measures their actual token rather than assuming
that UIAccess implies a particular integrity level.

The installer appends `Program Files\moon` to the owning user's PATH, making
`moon` discoverable from newly started ordinary Windows terminals. It preserves
the raw existing value, environment-variable references and registry value kind.
An equivalent pre-existing entry is left unowned. The protected installation
record identifies an entry added by moon; rollback and uninstall read the current
PATH and remove only that token, retaining later user edits. A changed value
detected before publication rejects the stale update. The installer broadcasts
`WM_SETTINGCHANGE` for `Environment` so Explorer can refresh new child processes;
already-running terminal processes retain their existing environment.

## First handover

Build the installation candidate from WSL:

```sh
cargo xwin build -p moon --features uiaccess --release --target x86_64-pc-windows-msvc --locked
```

After development probes have exited, `just handover` builds both the UIAccess
candidate and its two installation-only inventory/recovery helpers, then starts
the controlled handover. The equivalent explicit launch, after building the
helpers, is:

```sh
powershell.exe -NoProfile -ExecutionPolicy Bypass \
  -File "$(wslpath -w "$PWD/scripts/handover.ps1")" \
  -Binary "$(wslpath -w "$PWD/target/x86_64-pc-windows-msvc/release/moon.exe")"
```

Run this from the ordinary Windows account that will use moon. The installation
step requests Windows elevation from that same account. Handover records the
running GlazeWM, Zebar and tacky-borders processes, copies their configuration
files, and preserves their exact current-user startup shortcuts outside Startup.
It captures GlazeWM's exact owned window identities, asks GlazeWM to exit normally,
and verifies that the selected windows are uncloaked before stopping any remaining
captured bar or border process. The recovery helper checks HWND, PID, process
creation time and class; it never exposes every cloaked shell window. These
installation helpers are not installed or left running. Handover then installs
moon and verifies the signature, live UIAccess,
least-privilege startup, ordinary application launch and low-integrity IPC
rejection. These results and the backups are recorded beneath
`%LOCALAPPDATA%\moon\handover-<id>`; the active record is `handover.json`.

If installation or verification fails, handover removes any new owned moon
installation, restores the original shortcuts and restarts the previously
configured desktop components. A saved startup shortcut also supplies the launch
operation when validation had already stopped the previous manager. Recovery refuses to start another manager while a
moon process or recovery journal remains. If a recovery step cannot finish,
the record and backups remain available instead of silently discarding them.

For a deliberate return to the previous desktop:

```sh
powershell.exe -NoProfile -ExecutionPolicy Bypass \
  -File "$(wslpath -w "$PWD/scripts/handover.ps1")" -Rollback
```

This restores the saved startup shortcuts without replacing a different shortcut
that somebody has created since handover. Original configurations are backed up
but are not overwritten during rollback, since handover never edits them.

## Upgrades and uninstall

`just install` builds the UIAccess release and invokes `scripts/install.ps1`.
The installer refuses to replace an unrelated installation or startup task. A
running installation is restarted into the signed replacement, and its window
layout is compared before and after. The prior executable remains available
until the replacement is verified. On failure, the installer restores the old
executable, task and installation metadata; when needed, it adopts the saved
recovery journal to restore the prior layout. Recovery waits for the failed
replacement's watchdog before replacing its image.

`scripts/uninstall.ps1` asks the installed daemon to restore windows and exit,
requires outstanding recovery to finish, and then removes only the installation's
recorded executable files, task, signing key and trust entries. It leaves personal
configuration and validation records. Use `handover.ps1 -Rollback` when the old
desktop's startup and processes should also return.

## Verification performed so far

`scripts/validate-install.ps1` passes fourteen checks on Windows PowerShell
5.1.26100.9818: all installation and handover scripts parse; native token access
works; atomic metadata publication and replacement work; native arguments round
trip spaces, quotes, trailing backslashes, Unicode and empty strings; embedded
manifests can be read without executing the image; only the owned limited task
shape is accepted; an actual code-signing-only non-exportable CNG certificate is
accepted; and an unrelated signing identity is refused. Its isolated temporary
certificate is removed with its private key. Six additional cases exercise the
actual registry API in an isolated disposable key: raw PATH text and type,
trailing separators, subsequent user edits, pre-existing entries, absent values,
concurrent edits and duplicate preservation. These tests do not change the user's
PATH, machine trust or scheduled tasks.

The actual elevated install, running upgrade and rollback need their own captured
results. The helper checks do not establish those outcomes. After installation,
`scripts/verify-uiaccess.ps1` must run from ordinary medium-integrity PowerShell
and writes `%LOCALAPPDATA%\moon\uiaccess-results.json` by default.

`scripts/verify-elevated.ps1 -TestwinPath <testwin.exe>` performs the complementary
elevated-window check. It requests a high-integrity coordinator through UAC,
creates two owned elevated helper windows, checks native placement, exercises
twenty hook-driven focus changes, and toggles fullscreen and back through the
normal Alt+F binding. Input is sent only after verifying an owned helper is the
foreground window. Cleanup closes those helpers, verifies the original workspace
trees and restores the previous focus. It records
`%LOCALAPPDATA%\moon\elevated-results.json`. Its `-CompileOnly` check has passed on
this machine without requesting elevation; that check verifies the native x64
input structure and token-reading helper, not the S8 outcome.

## Sources

- [Microsoft UIAccess security requirements](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview), updated 19 February 2026.
- [New-SelfSignedCertificate](https://learn.microsoft.com/en-us/powershell/module/pki/new-selfsignedcertificate?view=windowsserver2025-ps), including certificate type and key export policy.
- [New-ScheduledTaskPrincipal](https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/new-scheduledtaskprincipal?view=windowsserver2025-ps), including limited run level and interactive logon.
- [Set-AuthenticodeSignature](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.security/set-authenticodesignature?view=powershell-5.1).
- [RegistryKey.GetValue](https://learn.microsoft.com/en-us/dotnet/api/microsoft.win32.registrykey.getvalue?view=netframework-4.8.1), including `DoNotExpandEnvironmentNames` for preserving raw registry text.
- [WM_SETTINGCHANGE](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange), including the `Environment` broadcast for updated user variables.

## Daily-driver observations

After the signed handover, `just soak` appends a durable timestamped observation
to `%LOCALAPPDATA%\moon\soak\session-<id>.jsonl`. It records process start time,
resource and latency statistics, the executable file hash, competing desktop
components and any reported error. Missing daemons and failed checks are also
recorded and return a failure exit code. `record-soak.ps1 -Incident "description"`
adds an observed papercut to the same record. Each call is one sample, not a
background service or a claim of continuous monitoring. Seven days passing on
the calendar cannot substitute for the required observations and acceptance
review; restarting or returning to the old manager must remain visible.
