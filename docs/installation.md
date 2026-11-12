# Installation and recovery

## Guided setup

The complete Windows package includes **Install Moon.cmd**. Open it from the
ordinary Windows account that will use Moon. The setup window offers one install
action, explains the Windows approval prompt, reports progress and ends with
**Open Settings**. A first installation uses the existing controlled desktop
handover; an update preserves the active window layout. Cancellation requests a
safe transaction checkpoint and rollback instead of terminating the worker.
After removal has crossed its final deletion boundary, the window waits for safe
completion. Diagnostics are available through a separate **Details** action.
Declining the Windows approval prompt is reported as cancellation using its native
error code ([ERROR_CANCELLED, 1223](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--1000-1299-)),
including when the previous desktop was explicitly left stopped.
The elevation launcher retains that code through .NET `Process.Start`; PowerShell's
[`Start-Process` source](https://github.com/PowerShell/PowerShell/blob/v7.7.0/src/Microsoft.PowerShell.Commands.Management/commands/management/Process.cs)
discards it while wrapping a shell launch error. Isolated Windows
checks cover the exception chain, installer/removal handlers and stopped rollback
without opening another approval prompt.

Guided setup checks that the extracted applications and first-install recovery
helpers are present before enabling installation. Repair remains available when
an installed executable is missing. Retrying clears the previous attempt's screen
state while retaining its diagnostic files. Cancellation displays the actual
recovery outcome, including when the previous window manager remains stopped.
An explicit `-KeepPreviousDesktopStopped` preference, or that preference in a
validated handover record for the same account, carries into guided
retries. An unrelated or unreadable record is not treated as permission to restart
another manager. The preference survives signing in again; process identities and
recovery actions from an earlier session are never reused for this purpose.

After installation, setup rechecks WebView2 when activated and updates the next
action to **Open Settings** when it becomes available. A failed Settings or browser
launch leaves setup open with an actionable message. Removal defaults keyboard
focus to **Keep Moon**; an already-removed installation completes without another
approval request. Reduced-motion users receive status text without a moving or
misleading percentage bar. These choices follow Microsoft's guidance for
[clear dialog actions](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/dialogs-and-flyouts/dialogs),
[unknown-duration progress](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/progress-controls),
and [WebView2 runtime detection](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution).

The setup polish passed 32 isolated Windows state/handler checks and the existing
14 recovery checks; reports are `target/setup-polish-validation.json` and
`target/setup-polish-recovery-validation.json`. WPF compilation also passed.
These checks did not show a setup window, request UAC, change installation state,
or replace the current desktop manager. A new visual preview and accepted signed
installation remain separate acceptance steps.

Only one elevated installation or removal can run at a time, across accounts and
Windows sessions. Its named mutex permits Administrators and SYSTEM and stays held
through recovery. Ordinary handovers have a separate account/session lock so they
cannot race while saving or stopping the previous desktop. An interrupted process
releases its lock; the next operation still validates the saved ownership record.
Progress files are advisory: their readers allow replacement, and a progress I/O
failure cannot interrupt rollback or keep the setup window waiting after its worker
exits. Support files enter the rollback list before replacement is attempted.

Setup uses Windows' built-in WPF so it can run before WebView2 is available. It
checks the documented Evergreen runtime registration. If the runtime is absent,
it explains that Settings needs it and links to
[Microsoft's WebView2 download page](https://developer.microsoft.com/en-us/microsoft-edge/webview2/consumer/).
It does not download or run a third-party installer automatically. Native keyboard
focus, readable contrast, high-contrast colors and the system animation preference
are respected. The dialog is bounded and non-resizable so an existing tiling
manager does not stretch a simple setup page across a whole monitor.

Moon appears in **Start** and **Settings → Apps → Installed apps**. The Start
entry opens the separate, ordinary-permission `moon-settings.exe`; both entries
use the product icon embedded in `moon.exe`. The app listing includes its version,
location and removal command. Removal asks for confirmation, restores managed
windows first and keeps personal configuration. The installed protected scripts
make this entry independent of the original download folder.

Shell registration belongs to the installation's recorded identity. Setup refuses
to replace an unrelated or edited Start-menu shortcut. Upgrade rollback restores
its prior exact bytes and app-list metadata; uninstall preserves a shortcut the
user edited later. The private per-user app-list key must still have matching
installation and Windows-account markers before removal.

Handover also records a unique operation identity in the protected installation
metadata. Cleanup removes only that operation's result, even if the installer's
success response was lost. Removal rechecks both the installation and operation
identities while holding its own lock. A later independent update is no longer
owned by the earlier handover: remove that installation explicitly before using
the earlier handover record to restore the previous desktop.

These conventions follow Microsoft's [shell-link contract](https://learn.microsoft.com/en-us/windows/win32/shell/links),
[installed-app metadata](https://learn.microsoft.com/en-us/windows/win32/msi/uninstall-registry-key),
[WebView2 distribution guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)
and [keyboard-accessibility guidance](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/keyboard-accessibility).
The older [setup design guide](https://learn.microsoft.com/en-us/windows/win32/uxguide/exper-setup)
supports the simple flow; Microsoft explicitly identifies its visual examples as
Windows 7-era material, so they are not used as current Windows 11 styling guidance.

The transaction helpers use the Windows PowerShell 5.1/.NET Framework
[mutex access-control constructor](https://learn.microsoft.com/en-us/dotnet/api/system.threading.mutex.-ctor?view=netframework-4.8.1#system-threading-mutex-ctor(system-boolean-system-string-system-boolean@-system-security-accesscontrol-mutexsecurity)),
available without another runtime. Recovery accounts for the documented
[ReplaceFile failure states](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew#return-value),
including failure after the original destination has disappeared.

`scripts/validate-install-recovery.ps1` passed 14 isolated Windows checks on
2026-11-12. It exercises real cross-process mutex exclusion and abandonment, file
sharing, the actual setup completion handler, and the actual installation and
handover recovery bodies with injected failures. Files and mutexes belong only to
the test; desktop RPC, signing, and elevation are substituted. This is recovery
logic evidence, not a completed elevated installation or uninstall.

## Installation boundaries

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
Direct invocation of the signed UIAccess image from an ordinary terminal still
requires the native `verify-cli-launch.ps1` result. PATH discovery alone does not
prove that launch, exit codes, stdout or subscription streaming work.

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

When copying the candidate to a Windows-local folder, keep `moon-settings.exe`,
`Install Moon.cmd`, `setup.ps1`, `handover.ps1`, `install-common.ps1`, `install.ps1`,
`uninstall.ps1`, `verify-uiaccess.ps1`, `window_inventory.exe` and
`recover_windows.exe` beside the candidate `moon.exe`.
Handover checks its required scripts before stopping the old desktop and resolves
the inventory/recovery helpers beside the script before checking the repository
build directory. Explicit `-InventoryPath` and `-RecoveryPath` remain available.
Refresh the whole script set together before retrying a prepared candidate.

Run this from the ordinary Windows account that will use moon. The installation
step requests Windows elevation from that same account. Handover records the
running GlazeWM, Zebar and tacky-borders processes, copies their configuration
files, and preserves their exact current-user startup shortcuts outside Startup.
If Windows withholds a UIAccess process's WMI path or command line, handover reads
the image path and creation time with limited process access. Missing arguments
must come from exactly one unchanged startup shortcut targeting that full image
path; its arguments and working directory are preserved. Missing, conflicting or
ambiguous launch provenance stops preflight before desktop changes. Every process
is checked again by PID, creation time and image before shutdown.
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

When continuing with Moon without restarting the old desktop, pass
`-KeepPreviousDesktopStopped` to `handover.ps1`, including a requested rollback.
Recovery still restores the saved startup shortcut and installation state, but
does not launch GlazeWM, Zebar or tacky-borders. It records
`rolled_back_stopped`. This option follows the user's current no-fallback
instruction; it does not remove the preserved startup shortcut or backups.

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
The recovery snapshot is accepted only when the journal's owner identity and world
revision match a stable layout query; capture retries a lagging asynchronous
checkpoint with a bounded wait. Its reader permits atomic journal replacement.
`validate-upgrade-snapshot.ps1` passed eight isolated checks: checkpoint lag,
revision changes during capture, stale owner rejection, revision-independent
comparison, and changes to focus, active workspace, tree order and membership
despite identical rectangles. It does not contact a daemon or alter an installation.

`scripts/uninstall.ps1` asks the installed daemon to restore windows and exit,
requires outstanding recovery to finish, and then removes only the installation's
recorded executable files, task, signing key and trust entries. It leaves personal
configuration and validation records. Use `handover.ps1 -Rollback` when the old
desktop's startup and processes should also return.

## Verification performed so far

The guided setup additions passed nine isolated shell-registration checks and four
status/cancellation checks. They exercised actual registry and shell-link APIs in
private test locations, including exact upgrade rollback, unowned-entry refusal,
concurrent shortcut edits and preservation of a user-modified shortcut. Evidence:
`target/install-shell-checks.json` and `target/setup-state-checks.json`. Existing
eleven-case handover and eight-case consistent-upgrade-snapshot checks also pass.

The real installation/removal confirmation views were previewed on the Windows
desktop without invoking either action. UI Automation verified accessible primary
actions and Tab movement to **Not now**/**Keep Moon**; both owned windows closed
and the prior foreground returned. Screenshots and the report are in
`target/setup-preview-final`. Both views passed again with the final static-runtime
image, including its actual embedded crescent icon in the header and title bar.
A first preview exposed stretching by the existing tiling manager; the bounded
dialog corrected it. These previews did not run installation. Subsequent accepted
and canceled elevated attempts are recorded below; neither the previews nor the
failed attempts establish installed signed acceptance.

`scripts/validate-install.ps1` passes fourteen checks on Windows PowerShell
5.1.26100.9818: all installation and handover scripts parse; native token access
works; atomic metadata publication and replacement work; native arguments round
trip spaces, quotes, trailing backslashes, Unicode and empty strings; embedded
manifests can be read without executing the image; only the owned limited task
shape is accepted; an actual code-signing-only non-exportable CNG certificate is
accepted; and an unrelated signing identity is refused. Its isolated temporary
certificate is removed with its private key. Six of those fourteen cases exercise the
actual registry API in an isolated disposable key: raw PATH text and type,
trailing separators, subsequent user edits, pre-existing entries, absent values,
concurrent edits and duplicate preservation. These tests do not change the user's
PATH, machine trust or scheduled tasks.

`scripts/validate-handover.ps1` passes eleven focused Windows PowerShell checks:
native process identity, the null-WMI shortcut fallback, exact argument and working
directory preservation, authoritative available command lines, and rejection of
unqueryable processes, conflicting paths, missing or ambiguous provenance,
changed shortcuts and stale process creation times. All shortcut fixtures live in
a disposable temporary directory. No application, Startup folder or elevation
prompt is involved. The native path/time APIs accept
[limited process-query access](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew);
[GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes)
provides the creation time used to reject a reused PID.

The initial real handover reached the Windows consent prompt and was canceled on
11 November at 23:03:06 UTC. Cancellation rollback restored the previous startup
shortcut/configuration hashes and restarted the previous desktop components;
the verification found no moon installation, task, signing/trust entries or PATH
entry. The detailed record is `target/handover-rollback-results.json`. This proves
the cancellation path; the signed install and running upgrade/rollback still
need their own captured successful results. Installation is now authorized, but
an elevated attempt still needs its Windows consent prompt to be accepted.
After installation,
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

## Live installation attempts and SID fix

The first accepted attempt with daemon `9B85EBEE…503B1546` failed and rolled
back. Its caller initially reported only the elevated worker's exit code; see
`target/live-install-20261112.log`. The diagnostic retry used
`1E77B62EB4D60AA7252BA0CE562880D405D48E40E03A963A1E67AB7DB2C08D8A`.
It retained the actual timeout waiting for the signed replacement daemon with
UIAccess, then recorded `rolled_back`. Evidence:
`target/live-install-diagnostic-retry.log` and
`target/live-install-diagnostic-attempt.json`. A separate operation with the same
image also reached the readiness timeout, as recorded in
`target/live-install-launch-probe.log`; its log does not by itself establish the
complete rollback outcome. None of these outcomes proves a working signed
installation.

Read-only token inspection found different authentication LUIDs for the ordinary
and linked elevated token, with the same logon SID
`S-1-5-5-0-14298163`. The old endpoint used the differing LUIDs. The Rust transport,
installer helper and standalone IPC example now use `TokenLogonSid`; the pipe
ACL still grants the current logon SID and retains its medium-integrity floor.
Microsoft documents the [logon SID as the current sign-in session's identity](https://learn.microsoft.com/en-us/windows/win32/secauthz/access-tokens)
and [`TokenLogonSid` as a `TOKEN_GROUPS` result](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-token_information_class).
The captured token comparison is
`target/live-install-token-link-evidence.json`; four IPC native checks, including
equality for linked tokens, pass in `target/sid-ipc-native-tests.txt`.

The endpoint-compatible candidate hashes are:

- Normal daemon: `DBADD0C08FA4C78AA1EF567520B593B7D3F68297BD3724C641CEDB082ECBCCAE`.
- UIAccess daemon: `521E8BD9E926AF8108A722564452DF3970E5A935D496A7EF22D37AE865C1E219`.
- Settings: `6A633A06B872765EA30AEDCBC9839F856FD3EA70BA5D855AC1BD85CCC8670E92`.

Both daemon and Settings must be updated together. The attempted bundle's report is
`target/package-moon-live-SIDfix-20261112-215657-2996bae7.json`; inventory and
recovery helpers retain their verified earlier bytes because they do not use IPC.
An attempt with this bundle was canceled at Windows consent before its
elevated installation ran. `target/live-install-SIDfix.log` records that outcome.
The refreshed bundle `moon-live-SIDfix-20261112-221404-db28feb0` was also canceled
at consent, at 11:17:03 UTC on 12 November. Operation
`e01d4dc15f434670a6fc13040edfb648` retained native code 1223, reported setup
`cancelled`, and recorded handover `rolled_back_stopped`; normal Moon subsequently
resumed without restarting GlazeWM. Evidence is in
`target/live-install-sid-cancellation-fixed.log` and
`target/live-install-sid-cancellation-fixed-state.json`.
The current UIAccess candidate therefore still needs an accepted installation
and successful signed runtime checks. A fixed endpoint alone does not establish
that scheduled startup or every other installation phase succeeds.

The updated diagnostics and recovery helpers passed 17 setup/startup checks,
14 recovery checks and 14 installer-helper checks. Their records are
`target/setup-startup-diagnostics-checks.json`,
`target/install-recovery-startup-checks.json` and
`target/install-helper-startup-checks.json`. The subsequent cancellation fix
passes 22 checks, including native error-code preservation and stopped rollback,
in `target/setup-cancellation-validation.json`; it requested no UAC prompt and
changed no installation. These isolated checks do not stand in for an actual
task-triggered signed daemon or an upgrade.

After a successful signed installation,
`scripts/verify-installed-settings.ps1` checks the actual `settings` RPC launch
from an ordinary PowerShell client. It requires valid signatures for the installed
UIAccess daemon and Settings, with no existing Settings process, and binds the newly launched companion to
its path and creation time, and verifies medium integrity without elevation or
UIAccess. Read-only UI Automation must find **Moon is running** and an unedited
document. It compares the configuration hash before and after, closes only the
owned unedited window gracefully, and retains a JSON report. Its syntax, native
token reader and UI Automation assembly loading have been checked; the signed
scenario has not yet run.

## Sources

- [Microsoft UIAccess security requirements](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview), updated 19 February 2026.
- [New-SelfSignedCertificate](https://learn.microsoft.com/en-us/powershell/module/pki/new-selfsignedcertificate?view=windowsserver2025-ps), including certificate type and key export policy.
- [New-ScheduledTaskPrincipal](https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/new-scheduledtaskprincipal?view=windowsserver2025-ps), including limited run level and interactive logon.
- [Set-AuthenticodeSignature](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.security/set-authenticodesignature?view=powershell-5.1).
- [RegistryKey.GetValue](https://learn.microsoft.com/en-us/dotnet/api/microsoft.win32.registrykey.getvalue?view=netframework-4.8.1), including `DoNotExpandEnvironmentNames` for preserving raw registry text.
- [WM_SETTINGCHANGE](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange), including the `Environment` broadcast for updated user variables.

## Daily-driver observations

The SID-fix installation attempt used the bundle staged locally under
`%LOCALAPPDATA%\moon-live-SIDfix-20261112-215657-2996bae7\moon-0.1.0-windows-x64`.
Its manifest records the executable, helper and script hashes. Staging did not
sign, trust, install or launch the images; the subsequent consent cancellation
is recorded above.

Earlier, after the ordinary Windows checks, the unchanged GlazeWM startup
shortcut restored GlazeWM, Zebar and tacky-borders on 12 November. The checked process
paths and creation times are recorded in
`target/revised-release-20261112/previous-desktop-restored.json`.
`scripts/restore-previous-desktop.ps1 -ReceiptPath <shutdown-receipt>` verifies
shortcut hashes and the captured executable paths before resuming that setup.
That is a historical restoration record. The user subsequently requested no
fallback to the previous manager. Ordinary SID-fix Moon is now running with the
personal configuration: Arc is assigned to workspace 1, Windows Terminal to 2,
and ChatGPT/Claude to 10. All four memberships remained correct in the settled
snapshot; the two visible workspace-10 frames matched the plan, and the other
two windows had shell-cloak value 2. Evidence is in
`target/live-workspace-after.json` and `target/live-workspace-settled.json`.
Actual visits to all three workspaces verified their expected visible applications
and exact native frames before returning to workspace 10; see
`target/live-workspace-visits.json`. Four isolated native checks then verified
per-window startup assignments, reveal/hide and original-frame restoration in
`target/startup-assignment-native-results.json`. The matching Settings image
passed all eight native save/conflict/reload/isolation checks with the normal
daemon, without changing personal configuration; evidence is in
`target/sid-settings-ui/report.json`. Normal Moon resumed after those isolated
checks. These results establish ordinary-session behavior, not signed-installation,
performance-budget or seven-day-soak acceptance.

After the signed handover, `just soak` appends a durable timestamped observation
to `%LOCALAPPDATA%\moon\soak\session-<id>.jsonl`. It records process start time,
resource and latency statistics, the executable file hash, competing desktop
components and any reported error. Missing daemons and failed checks are also
recorded and return a failure exit code. `record-soak.ps1 -Incident "description"`
adds an observed papercut to the same record. Each call is one sample, not a
background service or a claim of continuous monitoring. Seven days passing on
the calendar cannot substitute for the required observations and acceptance
review; restarting or returning to the old manager must remain visible.
