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

## Repaint-flag experiments

Two later full-pipeline comparisons each used 128 real hook-driven resize chords
and eight owned helpers, with the bar and borders enabled. Each pair used the
same candidate; the two pairs were recorded at different development stages and
should not be compared to each other as an isolated causal experiment.

| Pair | Placement flags | External key-to-placement median | p99 | Working set after idle |
|---|---|---:|---:|---:|
| Defer erase | Default | 14.575 ms | 47.200 ms | 59,097,088 bytes |
| Defer erase | Add `SWP_DEFERERASE` | 13.860 ms | 38.025 ms | 48,390,144 bytes |
| Copy bits | Default | 14.415 ms | 40.263 ms | 48,586,752 bytes |
| Copy bits | Add `SWP_NOCOPYBITS` | 11.506 ms | 36.181 ms | 53,936,128 bytes |

Neither flag met the median-under-10-ms and p99-under-30-ms placement budgets.
The no-copy variant also exceeded the 50-MB working-set limit. Neither received
cross-application repaint/flicker acceptance, so both temporary environment
switches were removed and the normal flag set remains unchanged. Raw records:
`target/placement-ab-20261112/defer-{0,1}.json` and
`target/final-validation-20261112/nocopy-{0,1}.json`.

[SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)
defines `SWP_DEFERERASE` as suppressing `WM_SYNCPAINT` and `SWP_NOCOPYBITS` as
discarding the previously saved client-area contents after resize. These
experiments did not use `SWP_NOREDRAW` or suppress `WM_WINDOWPOSCHANGING`.

## Installed WinUI 3 test candidate

PowerToys Settings 0.102.3101.0 is installed at
`C:\Program Files\PowerToys\WinUI3Apps\PowerToys.Settings.exe`. No Settings
process was open during the read-only inventory. Its current official project
sets `UseWinUI`, uses the Windows App SDK and builds into `WinUI3Apps`; the app
imports `Microsoft.UI.Xaml`.
[Project source](https://github.com/microsoft/PowerToys/blob/main/src/settings-ui/Settings.UI/PowerToys.Settings.csproj).

A later test found that the current elevated PowerToys runner also launches its
Settings window elevated. That owned window was closed through the verified
Glaze container after the unsigned test deferred its privileged scenario; the
existing runner survived. A launch without runner arguments delegates through
PowerToys and exits, so the actual UI PID must be identified independently.
[Launch behavior](https://github.com/microsoft/PowerToys/blob/main/src/settings-ui/Settings.UI/SettingsXAML/App.xaml.cs).
The subsequent medium-integrity Paint scenario provides the actual WinUI 3
window-management and three-application focus evidence in
[application verification](validation-applications.md); elevated Settings remains
part of installed S8 verification.
