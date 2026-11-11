# Three-application focus and WinUI 3 validation

`scripts/application-integration.ps1` provides the S4 scenario: an owned
testwin window, the Windows About dialog, and PowerToys Settings. Its temporary
configuration ignores every other process and assigns the three applications
separate workspaces. The probe sends 100 guarded Alt workspace chords and checks
both the actual foreground HWND and the committed world after each transition.
It separately checks Settings tiling, floating, hiding, showing, and restoration
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

The remaining Settings window was recorded at both
`target/owned-settings-recovery.json` and
`%LOCALAPPDATA%\moon-dev\owned-settings-recovery.json`: HWND 1378216, PID 37612,
creation FILETIME 134389111018046810, class `WinUIDesktopWin32WindowClass`, and
image `C:\Program Files\PowerToys\WinUI3Apps\PowerToys.Settings.exe`.
Its original visible frame was `(2560,80)-(3800,2120)`. The existing PowerToys
runner, PID 18556, was preserved. `scripts/close-owned-settings.ps1` checks the
complete identity and uses the signed daemon's `expected_focus` precondition
before closing it.

The test now detects an elevated PowerToys runner before opening Settings in a
development run. Its signed-installation mode uses an explicitly elevated test
coordinator for input over elevated windows and ShellExecute for the signed
daemon. The normal daemon must first stop and restore its windows; the test
then owns its isolated daemon and cleanup. The normal installation can resume
after the probe. PowerShell 5.1 compiled the native INPUT helpers and parsed the
scripts successfully; actual signed focus results are recorded separately by
the probe.
