# S5: repeated placement comparison

On 12 November 2026, `placement_compare.exe` created eight owned test windows
on the 4K, 125% monitor. It compared the shipping asynchronous `SetWindowPos`
path with synchronous `BeginDeferWindowPos` / `DeferWindowPos` /
`EndDeferWindowPos`. Each method received 200 measured samples after ten warm-up
samples. ABBA ordering balanced both methods across two changing layouts, with
10 ms between samples. All 400 measured batches ended with all eight visible
frames exactly matching their targets, using successful direct
`DWMWA_EXTENDED_FRAME_BOUNDS` reads without a fallback API.

| Method | Submit median | Submit p99 | Exact-frame median | Exact-frame p99 | Exact-frame maximum |
|---|---:|---:|---:|---:|---:|
| Asynchronous SetWindowPos | 0.958 ms | 2.050 ms | 9.186 ms | 12.178 ms | 21.600 ms |
| Deferred batch | 7.342 ms | 20.829 ms | 7.587 ms | 21.096 ms | 24.744 ms |

The deferred batch improved median completion in this sample but held the
calling thread longer and had a worse p99. The shipping asynchronous path
remains unchanged. These are placement-and-readback measurements, not
hook-to-placement measurements; they exclude configuration reduction, journal
writes and rendering. The sample uses eight responsive windows of one helper,
so it does not establish behavior across slow or unresponsive applications.
Flicker and compositor presentation were not measured: matching window
coordinates does not prove when another process finished painting.

Build with `cargo xwin build -p moon-win --example placement_compare --release
--target x86_64-pc-windows-msvc`, then run
`placement_compare.exe C:\path\testwin.exe 200`. The helper closes on success
or error and prior foreground is restored. Raw samples from the recorded run
are retained locally in `target/placement-comparison.json`.

The implementation follows Microsoft's current requirements: all batched
windows share a parent, every returned HDWP replaces the previous value, and
a failed defer operation is abandoned without calling EndDeferWindowPos.
References: [BeginDeferWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-begindeferwindowpos),
[DeferWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-deferwindowpos),
[EndDeferWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enddeferwindowpos),
[SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)
and [DwmFlush's caller-specific limits](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmflush).

## Installed WinUI 3 test candidate

PowerToys Settings 0.102.3101.0 is installed at
`C:\Program Files\PowerToys\WinUI3Apps\PowerToys.Settings.exe`. No Settings
process was open during the read-only inventory. Its current official project
sets `UseWinUI`, uses the Windows App SDK and builds into `WinUI3Apps`; the app
imports `Microsoft.UI.Xaml`.
[Project source](https://github.com/microsoft/PowerToys/blob/main/src/settings-ui/Settings.UI/PowerToys.Settings.csproj).

A later native test can open the dashboard without editing a setting. A launch
without runner arguments delegates through PowerToys and exits, so the test must
identify the newly created Settings window rather than assuming the launcher's
PID is the UI process. Cleanup must preserve existing PowerToys processes.
[Launch behavior](https://github.com/microsoft/PowerToys/blob/main/src/settings-ui/Settings.UI/SettingsXAML/App.xaml.cs).
This inventory is not a WinUI 3 window-management acceptance pass.
