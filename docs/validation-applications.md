# Three-application focus and WinUI 3 validation

S4 passed **100/100** hook-driven foreground transitions on 12 November 2026,
using three newly owned medium-integrity applications: testwin, Windows About
(`winver.exe`), and Paint 11.2609.401.0. The tested moon image SHA-256 was
`1A7F502085F9CD067C18975E76E454E75F60F28FC75C809E1541DA86F33FC5CF`,
including the final hook key-pair ownership correction.
`target/application-integration-paint-final.json` records every transition, native
identity, check, runtime module, and cleanup result. The earlier candidate also
passed; its separate report remains at `target/application-integration-paint.json`.

Paint loaded `Microsoft.UI.Xaml.dll` version `3.2.0.2511` from
`Microsoft.WindowsAppRuntime.2_2.6.5.0_x64__8wekyb3d8bbwe`, proving that this
specific tested process used WinUI 3. It passed exact native tiling, floating,
workspace hiding/showing, and original-frame restoration. All three processes
had integrity RID 8192. Paint opened only an owned copy of its installed blank
bitmap template; its file hash remained unchanged. All owned windows, daemon,
watchdog, and journal were gone after cleanup, and the preexisting PowerToys
runner survived.

The final hook recorded 515 callbacks, with own-work p99 of 135 microseconds and
complete callback p99 of 581 microseconds. The latter includes downstream hooks; this
test establishes focus correctness and WinUI 3 behavior, not the whole-callback
latency budget. Microsoft documents that
[WinUI 3 ships with the Windows App SDK](https://learn.microsoft.com/en-us/windows/apps/winui).

`scripts/application-integration.ps1` provides the S4 scenario: an owned
testwin window, the Windows About dialog, and Paint (`-ThirdApplication Paint`)
or PowerToys Settings. Its temporary
configuration ignores every other process and assigns the three applications
separate workspaces. The probe sends 100 guarded Alt workspace chords and checks
both the actual foreground HWND and the committed world after each transition.
It separately checks WinUI tiling, floating, hiding, showing, and restoration
to its original native frame before cleanup. These focus observations are not a
placement-latency benchmark.

PowerToys Settings is a WinUI 3 application: Microsoft's current
[project](https://github.com/microsoft/PowerToys/blob/main/src/settings-ui/Settings.UI/PowerToys.Settings.csproj)
enables WinUI and the Windows App SDK; its
[application source](https://github.com/microsoft/PowerToys/blob/main/src/settings-ui/Settings.UI/SettingsXAML/App.xaml.cs)
delegates an argument-free launch to the existing runner. The probe therefore
identifies the actual new window and process instead of assuming the launcher
PID owns the UI.

The first native attempt on 12 November 2026 stopped before starting a daemon or
sending a focus chord. The existing PowerToys runner launched Settings at high
integrity (RID 12288), beyond the unsigned medium-integrity test's scope. The
owned testwin and About windows were closed. Closing the new Settings HWND from
medium integrity returned access denied (Win32 error 5). No S4 or WinUI 3 pass is
claimed from that attempt.

The Settings recovery identity was recorded at both
`target/owned-settings-recovery.json` and
`%LOCALAPPDATA%\moon-dev\owned-settings-recovery.json`: HWND 1378216, PID 37612,
creation FILETIME 134389111018046810, class `WinUIDesktopWin32WindowClass`, and
image `C:\Program Files\PowerToys\WinUI3Apps\PowerToys.Settings.exe`.
Its original visible frame was `(2560,80)-(3800,2120)`. The existing PowerToys
runner, PID 18556, was preserved. `scripts/close-owned-settings.ps1` checks the
complete identity and uses the signed daemon's `expected_focus` precondition
before closing it.

After the first installation's UAC prompt was canceled, the restored GlazeWM
3.9.1 provided an existing trusted route for cleanup. Its installed CLI help
confirmed targeted commands; a query mapped the exact HWND to container UUID
`0fb4d64d-6e25-4a0e-a02d-296322b9d26e`. The guarded cleanup command destroyed
that Settings window. All five other managed HWND/PID/creation-time identities
and PowerToys runner PID 18556 survived. Evidence is in
`target/owned-settings-glaze-cleanup.json`; no elevation was requested.

The canceled installation also passed rollback verification: the Glaze startup
shortcut and all three saved configuration files matched their backup hashes;
the previous desktop components were running; no moon installation directory,
task, PATH entry, process, signing certificate, or trust entry existed. See
`target/handover-rollback-results.json`. For the remaining isolated tests, the
restored old desktop components then stopped using captured process identities
and normal Glaze exit. All five Glaze-owned windows were recovered and verified
uncloaked. Startup shortcuts remained unchanged. That separate action is
recorded in `target/previous-desktop-stop.json`.

The test now detects an elevated PowerToys runner before opening Settings in a
development run. Its signed-installation mode uses an explicitly elevated test
coordinator for input over elevated windows and ShellExecute for the signed
daemon. The normal daemon must first stop and restore its windows; the test
then owns its isolated daemon and cleanup. The normal installation can resume
after the probe. PowerShell 5.1 compiled the native INPUT helpers and parsed the
scripts successfully; actual signed focus results are recorded separately by
the probe.
