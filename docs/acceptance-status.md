# Acceptance evidence

This ledger maps the checks in `.claude/plans/moon-v1.md` to observed evidence.
It was reviewed on 12 November 2026. Implemented event handling and passing
simulated tests do not establish that the corresponding physical Windows event
has been exercised.

## Product polish candidate, 12 November 2026

The current normal-permission Windows candidate is
`075E96CD3C7C95E24D04F07EC9BCFF7D53F74F60E2F6AC53B23B179C7DFBB134`.
Settings is
`7901563646D88335A3882DC0604691F5868D5F4BEA625D9C0D1479368A7BE050`.
The corresponding UIAccess installation image is
`9B85EBEEFD8404AC8E1FCB2F5395883D81CF1ADD8E7CCFE79488F7DD503B1546`;
it has been built, but has not been signed, installed or run with UIAccess.
All four distribution executables use the static C runtime. Setup checks the
separate WebView2 Evergreen prerequisite for Settings.

| Area | Recorded result |
|---|---|
| Configuration and persistence | 86 portable tests pass: 70 core, 10 CLI and six store cases. Windows adds five store cases and 18 native unit checks; four interactive hook cases are explicitly excluded from this run and retain their earlier separate evidence. |
| Settings interaction | 13 browser scenarios and eight live Windows checks pass. The native run proves comment-preserving save, applying the expected effective configuration, conflict retention, reload confirmation, Appearance rendering and refusing to reload a daemon using another file. Report: `target/polish-settings-ui/report.json`. |
| Native layout and recovery | All 14 integration scenarios pass on the current static-runtime candidate, including guarded close, restart, watchdog recovery and failed-startup recovery. Report: `target/polish-static-integration-results.json`. |
| Workspace motion | Workspaces are the carousel pages. A focused window activates its containing workspace; focus within that workspace does not animate. Ten native integration cases and nine grouped-thumbnail probe cases cover the behavior described in [carousel validation](validation-carousel.md). |
| Icons and setup | Two binary-resource checks and 60 isolated installer/registration/handover checks pass. Setup preview checks do not invoke Install or Remove. Multi-size icons and product identity are embedded in both application images. |
| Distribution | The packaging validator passes 14 isolated cases, including deterministic archive readback, input tampering, permission declarations, external-runtime rejection and partial-publication cleanup. [Packaging](packaging.md) describes strict validation of the final inputs. |
| Build and dependencies | Portable and Windows Clippy pass with warnings denied; frontend type checking, generated assets and formatting pass. Windows-target dependency policy checks pass. The cross-platform lockfile's two unused Linux dependency advisories are documented in [product polish](product-polish.md). |

The user's deferred Windows installation approval remains in effect. This
candidate does not establish signed-install, elevated-window, physical resilience
or seven-day-soak acceptance. Earlier rows below retain their candidate identity;
they are not silently promoted to measurements of the new binary.

## Milestone checks

| Check | Observed evidence | Acceptance still requiring a measured result |
|---|---|---|
| S1: enumeration | Native probe finds all eight owned helpers. The same-desktop Glaze comparison accounts for every Glaze-managed HWND and explains ignored/shell-cloaked differences; four already-stranded ordinary windows were recovered by exact identity. | Covered on this Windows build; see platform evidence. |
| S2: cloak | Native shell cloak becomes `DWMWA_CLOAKED = 2`; uncloak returns zero. Integration verifies workspace visibility and forced-death restoration. | Covered on this Windows build. |
| S3: hotkeys | Alt+H, Alt+Shift+1 and Alt+Tab each match ten trials with the helper retaining foreground. Final ordered masking passes 22 renewal checks and 256 trigger checks across 128 burst chords with both release orders. Consumed chords preserve HKL and leak no shortcut or character events. All 52 default routes preserve foreground, HKL and native menu state. Key-pair ownership survives modifier changes and renewal; ordinary F24 still reaches other hooks. | Repeat after actual lock/unlock and verify no menu or task-switcher activation. |
| S4: focus | [100/100 hook-driven foreground changes](validation-applications.md) across three distinct medium-integrity processes: testwin, Windows About and Paint. World focus and the actual foreground HWND agree for every transition, including the final key-pair-corrected candidate 1A7F5020…. | Covered on the recorded candidate; elevated-window focus remains S8. |
| S5: placement | All 400 repeated batches have eight exact DWM frames. Async settled median/p99: 9.186/12.178 ms; deferred: 7.587/21.096 ms. The async path retains faster submission and better tail latency. | Full key-to-placement distribution and a comparative visual flicker assessment are separate checks. |
| S6: border underlay | Eight attributed rings have continuous corners and gradients at physical 125%; four physical clicks across the actual underlay reached a separate receiver process exactly once each. | Complete physical 100% and 150% visual checks. |
| S7: text | Native DirectWrite bar and Nerd Font glyphs render on the development display; font data is embedded. | Exercise a Windows installation without a system Nerd Font and retain the visual result. |
| M1: core and sandbox | Core invariants, reducer benchmark and all fourteen native integration scenarios pass on normal release candidate 1A7F5020…, including guarded close rejection, exact restart state, crash recovery and injected startup failure. | Covered for this normal release candidate; repeat on the installed signed candidate. |
| M2: config and workspaces | All 46 active GlazeWM chords have equivalent moon defaults; none are missing. Real rule fixtures, command parsing, live reload, cloak, restart, public IPC extension and parser fuzzers pass. Sixteen ordinary native regressions include terminated-process liveness, bounded IPC, atomic-replacement sharing, modifier state and border ownership. All 52 defaults route exact command payloads through the live hook without executing daemon effects. Five portable writer tests cover durable barriers, coalescing, error handling and shutdown. A source/data-flow review and 231 saved evidence files found no raw keyboard-event logging or pressed-state dumps; [scope and limits](validation-platform.md#input-logging-evidence) are recorded. | Exercise every binding's resulting behavior with moon as the sole manager. |
| M3: bar | Pure scene tests, provider probes, native rendering, centered pills, custom theme and default glyphs pass. | Record side-by-side visual parity and the full-process idle/performance measurements. |
| M4: borders | Pure ring geometry and physical 125% rendering pass; floating native tint is implemented. | Complete physical 100% and 150% scaling and multi-monitor visual checks. |
| M5: installation | The actual handover reached UAC and was canceled. Its rollback restored the startup shortcut and configurations byte-for-byte; old components restarted, and no moon installation, task, signing/trust entries or PATH entry remained. [Rollback evidence](validation-applications.md) distinguishes this observed cancellation path from the isolated installer-helper tests. | Signed installation and running upgrade/rollback are still untested. A new Windows approval prompt is deferred until the user is available. |
| S8: privilege boundary | Medium-integrity IPC succeeds; low-integrity IPC is rejected. | With the installed signed binary, exercise elevated foreground hotkeys/placement and confirm launched applications are medium integrity without UIAccess. |
| M5: clean first run | No-config loading and native sandbox adoption pass. | Run the copied executable on a clean Hyper-V Windows VM with no config and confirm tiling. |
| M5: seven-day soak | No seven-day observation has elapsed. | Seven consecutive days as the sole manager, no lost windows, recorded uptime and papercuts, no fallback to GlazeWM. |

## Physical resilience matrix

For each row, retain the before/after workspace tree, active workspace, visible
native rectangles, cloak state and daemon process/revision. The running daemon
must stay usable and every owned test window must be recoverable afterward.

| Scenario | Current measured status | Completion evidence |
|---|---|---|
| Sleep and wake | Not exercised. | Suspend and resume the machine; hooks renew, windows retain membership, bar and placements recover. |
| Lock and unlock | Not exercised. | Lock and unlock the session; no placement or focus during the lock, hotkeys work afterward. |
| Explorer restart | Passed on final candidate 1A7F5020… after excluding shell/desktop handles from fullscreen detection. The same daemon retained four helpers, complete layout semantics and exact frames; AppBar reservation and tray registration returned. All five visible user applications survived. Cleanup restored original frames/work area. See `target/explorer-restart-results.json` and [platform evidence](validation-platform.md#actual-explorer-restart). | Covered on this Windows build. Tray API recovery is verified; visible-pixel icon position is not asserted. |
| Display added/removed | Core display-removal tests pass; physical hotplug not exercised. | Attach/remove a monitor with visible and hidden workspaces; all windows land on a surviving display. |
| Resolution and DPI changes | Geometry tests include 125%; physical changes not exercised. | Change resolution and scale, checking AppBar reservation, physical frame bounds and ring/text alignment. |
| UAC prompt | Not exercised. | Enter and leave the secure desktop; no unwanted focus, normal bindings resume after the prompt. |
| RDP session | Session-scoped journals implemented; RDP not exercised. | Connect/disconnect RDP, verify separate session ownership/recovery and restored local display geometry. |
| Native virtual desktop switch | Suspension and persistent-tree tests pass; physical switch not exercised. | Switch native desktops with managed windows on both; no forced switch back, off-desktop uncloak or membership loss. |
| WSLg windows | Host `.wslconfig` explicitly sets `guiApplications=false`; the distro has no X11/Wayland display. WSLg 1.0.79 and `xmessage` are installed, but enabling GUI support requires restarting WSL, including this build environment. | Tile, focus, hide, restore and close a real WSLg application. |
| WinUI 3 windows | Paint 11.2609.401.0 passed exact tile/float placement, hook-driven focus, hide/show, original-frame restoration and owned-window cleanup on final candidate 1A7F5020…. Its loaded Microsoft.UI.Xaml.dll version3 and medium token were measured; [application evidence](validation-applications.md) records both. | Application-window behavior is covered; WinUI 3 transient dialogs require a separate measured scenario. |
| Fullscreen games | Native borderless helper transition passes. | Exercise an actual fullscreen game and return to tiling without a persistent bar/border or manual-fullscreen flag change. |
| Multi-monitor actions | Pure monitor/workspace movement tests pass; physical multi-monitor not exercised. | Move windows and workspaces between real displays, including differing DPI; verify cursor jump and per-monitor overrides. |

## Performance budgets

| Budget | Evidence available |
|---|---|
| Hook callback p99 under 50 µs | Current static-runtime candidate: 140 µs p99 for Moon's own work and 719 µs for the whole callback. Both exceed the budget. The earlier 9 µs sample predates the key-pair correctness fix. |
| Eight-window key-to-placement median under 10 ms, p99 under 30 ms | 128/128 exact placements pass. Current independent median/p99 are 12.665/32.790 ms; latency remains above budget. |
| 50-window reducer plus layout under 100 µs | Passed: 22.59–22.93 µs in the recorded Criterion run. |
| Idle CPU 0%, no timer faster than 1 Hz | Current ten-second sample is 0.625% of one core. Providers run at 1 Hz and unchanged scenes do not redraw; animation uses a bounded active-only timer. Literal zero CPU is not established. |
| Working set under 50 MB, bar and borders on one 4K monitor | Current eight-window process: 45,682,688 bytes before the workload, 54,898,688 after it and 52,674,560 after idle. The current candidate fails the budget. Earlier lower measurements do not establish a pass for it. |
| Ready in under 300 ms | Current eight-window candidate measured 129.901 ms and passes. The signed installation still needs its own result. |

These results use normal-permission candidate `075E96CD…`, eight owned helpers,
the default WARP renderer, and a 3840×2160 display at 125% scaling, with the bar
and borders enabled. Raw records are in
`target/postpolish-performance/static-crt/`. A separate four-window workspace
sample observed all six full-width animations below the 50-pixel bar, with
47,575,040 bytes peak working set. Its command-to-overlay-disappearance interval
was 251.64–269.55 ms; this includes command handling and external sampling, not
just the nominal 200 ms easing. It does not replace the eight-window budget test.

Detailed results are in [native integration](validation-integration.md),
[platform verification](validation-platform.md),
[placement comparison](validation-placement.md), and
[surfaces/providers](surfaces-and-providers.md). Update this ledger when those
documents gain measured results; never infer a pass from implementation alone.
