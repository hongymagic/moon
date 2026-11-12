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

## Paint WinUI 3 transient dialog

On 13 November 2026 (Sydney time), the real Paint colour-dialog scenario passed
**6/6** against ordinary Moon daemon PID37512, image SHA-256
`E050F021CFA3643CAF33B0EE007525FD1676575D688A73706FC14A8979512BE5`. Evidence:
`target/paint-dialog-final.json`; screenshots and every bounded accessibility
response are retained in `target/paint-dialog-final-assets`. Paint11.2609.401.0
ran at medium integrity (RID8192), loading `Microsoft.UI.Xaml.dll`3.2.0.2511
from WindowsAppRuntime2.6.5.0. It opened only a uniquely named owned copy of its
installed blank bitmap template.

The test identified the actual **Edit colours** XAML window by its observed
`Popup` accessibility class and required its own `ColorSpectrum`,
`PrimaryButton` (OK), and `CloseButton` (Cancel). Owned Paint PID41176/main
HWND7996540 then passed:

- Opening that genuine WinUI3 colour dialog, with its exact control identities.
- Floating the owner while the dialog remained accessible and native geometry
  matched Moon's placement.
- Switching away and back: both the main HWND and its owned tooltip HWND had
  DWM cloak value2 while hidden; the same dialog runtime identity returned.
- Canceling the dialog normally, then restoring tiling with exact native bounds.
- Keeping the temporary bitmap bytes unchanged.

The six report checks include the initial process/runtime identity proof.
Paint closed normally; no forced termination was needed, and cleanup errors
were empty. The four preexisting HWND/PID/creation-time identities, full semantic
layout, native foreground HWND263988, pointer and configuration hash were
restored. A separate read-only check confirmed zero Paint processes and the
same Moon PID37512 with `last_error=null`. Visual inspection of
`restored-dialog.png` confirmed the complete colour dialog after its workspace
round trip. This is real WinUI3 transient-dialog coverage, not a shell file
picker or a helper simulation. It does not claim that Paint exposes the
accessibility class `ContentDialog`.

`scripts/paint-dialog-integration.ps1` runs against an explicitly identified
existing daemon. Every accessibility read and cancellation runs in its own
ordinary MTA worker with an eight-second process deadline and repeated native
ownership checks. This follows Microsoft's [UI Automation threading
guidance](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading).
A bounded breadth-first TreeWalker deduplicates visited nodes and truncates
repeating sibling branches. Opening uses one click at the freshly observed
button center, after exact control, foreground, hit-test and held-input checks.
The test records every worker response and native failure state. Optional forced
cleanup is restricted to the exact retained process, unchanged unique blank
fixture, absence of a second document and no intervening input; needing that
cleanup makes a run fail.

PowerShell5.1 compilation and **11/11 isolated checks** pass in
`target/paint-dialog-isolated-validation.json`, including the observed selector,
rejection of foreign/hidden/non-XAML/ambiguous controls, invalid-window rejection
before input or accessibility access, and termination of a deliberately stalled
owned worker at its deadline. These checks did not change desktop windows.

Earlier failed attempts are retained as harness history:

- `target/paint-dialog-discovery.json` first identified the actual Edit colours
  button and closed its blank document normally.
- `target/paint-dialog-provider-failure.json` and
  `target/paint-dialog-recovery.json` record an initial unbounded accessibility
  attempt and its guarded cleanup. The harness inspected a descendants count
  only after the provider call, which did not bound that call. Microsoft also
  documents that [InvokePattern.Invoke can block for modal
  providers](https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.invokepattern.invoke?view=windowsdesktop-10.0).
  These observations do not establish an application or Moon defect.
- `target/paint-dialog-click-validation.json` failed because the test required
  the exact accessibility class `ContentDialog`. The diagnostic
  `target/paint-dialog-observed-tree.json` and its saved screenshot/tree proved
  that the real colour dialog was open as XAML `Window`/`Popup`, with all three
  specific controls. The small native popup was its Black tooltip; the main
  HWND stayed enabled. An ignored main-window `WM_CLOSE` with that dialog open
  was not proof of a Paint hang.
- `target/paint-dialog-selector-cycle-failure.json` records a repeated sibling
  identity in the palette tree. The traversal initially rejected the whole
  result. Bounded branch truncation preserved the already queued unique nodes
  while still requiring the exact dialog controls. The subsequent full run
  passed without changes to Moon or the user's configuration.

Every failed attempt stopped before floating/workspace commands, retained its
own cleanup evidence, and restored the original application identities and
layout. None is counted as a product acceptance pass.
