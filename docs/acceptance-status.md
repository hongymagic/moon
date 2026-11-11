# Acceptance evidence

This ledger maps the checks in `.claude/plans/moon-v1.md` to observed evidence.
It was reviewed on 12 November 2026. Implemented event handling and passing
simulated tests do not establish that the corresponding physical Windows event
has been exercised.

## Milestone checks

| Check | Observed evidence | Acceptance still requiring a measured result |
|---|---|---|
| S1: enumeration | Native probe finds all eight owned helpers. The same-desktop Glaze comparison accounts for every Glaze-managed HWND and explains ignored/shell-cloaked differences; four already-stranded ordinary windows were recovered by exact identity. | Covered on this Windows build; see platform evidence. |
| S2: cloak | Native shell cloak becomes `DWMWA_CLOAKED = 2`; uncloak returns zero. Integration verifies workspace visibility and forced-death restoration. | Covered on this Windows build. |
| S3: hotkeys | Alt+H, Alt+Shift+1 and Alt+Tab each match ten trials with the helper retaining foreground. Initially held modifiers plus ten forced hook renewals pass 22/22 modified/plain trigger checks; the Korean HKL remains unchanged with two installed layouts. | Repeat after actual lock/unlock and verify no menu or task-switcher activation. |
| S4: focus | 100/100 foreground changes across helper windows. | Repeat across three different applications, including the hook-driven path. |
| S5: placement | All 400 repeated batches have eight exact DWM frames. Async settled median/p99: 9.186/12.178 ms; deferred: 7.587/21.096 ms. The async path retains faster submission and better tail latency. | Full key-to-placement distribution and a comparative visual flicker assessment are separate checks. |
| S6: border underlay | Eight attributed rings have continuous corners and gradients at physical 125%; four physical clicks across the actual underlay reached a separate receiver process exactly once each. | Complete physical 100% and 150% visual checks. |
| S7: text | Native DirectWrite bar and Nerd Font glyphs render on the development display; font data is embedded. | Exercise a Windows installation without a system Nerd Font and retain the visual result. |
| M1: core and sandbox | Core invariants, reducer benchmark and all fourteen native integration scenarios pass, including crash recovery and injected startup failure. | Covered by the recorded core and integration evidence. |
| M2: config and workspaces | All 46 active GlazeWM chords have equivalent moon defaults; none are missing. Real rule fixtures, command parsing, live reload, cloak, restart, public IPC extension and parser fuzzers pass. Eight native regressions include terminated-process liveness, bounded IPC and atomic-replacement sharing. Five portable writer tests cover durable barriers, coalescing, error handling and shutdown. | Exercise every binding through the live hook while moon is the only manager; capture the no-key-data logging check. |
| M3: bar | Pure scene tests, provider probes, native rendering, centered pills, custom theme and default glyphs pass. | Record side-by-side visual parity and the full-process idle/performance measurements. |
| M4: borders | Pure ring geometry and physical 125% rendering pass; floating native tint is implemented. | Complete physical 100% and 150% scaling and multi-monitor visual checks. |
| M5: installation | Installer, rollback, signature, scheduled-task and UIAccess verification scripts exist. | Retain the actual signed-install, running-upgrade and rollback results; script presence is not an installation test. |
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
| Explorer restart | Not exercised. | Restart Explorer; AppBars and tray icon return without duplicate reservation or window loss. |
| Display added/removed | Core display-removal tests pass; physical hotplug not exercised. | Attach/remove a monitor with visible and hidden workspaces; all windows land on a surviving display. |
| Resolution and DPI changes | Geometry tests include 125%; physical changes not exercised. | Change resolution and scale, checking AppBar reservation, physical frame bounds and ring/text alignment. |
| UAC prompt | Not exercised. | Enter and leave the secure desktop; no unwanted focus, normal bindings resume after the prompt. |
| RDP session | Session-scoped journals implemented; RDP not exercised. | Connect/disconnect RDP, verify separate session ownership/recovery and restored local display geometry. |
| Native virtual desktop switch | Suspension and persistent-tree tests pass; physical switch not exercised. | Switch native desktops with managed windows on both; no forced switch back, off-desktop uncloak or membership loss. |
| WSLg windows | Host `.wslconfig` explicitly sets `guiApplications=false`; the distro has no X11/Wayland display. WSLg 1.0.79 and `xmessage` are installed, but enabling GUI support requires restarting WSL, including this build environment. | Tile, focus, hide, restore and close a real WSLg application. |
| WinUI 3 windows | Not exercised. | Tile, focus, hide, restore and close a real WinUI 3 application, including its transient dialogs. |
| Fullscreen games | Native borderless helper transition passes. | Exercise an actual fullscreen game and return to tiling without a persistent bar/border or manual-fullscreen flag change. |
| Multi-monitor actions | Pure monitor/workspace movement tests pass; physical multi-monitor not exercised. | Move windows and workspaces between real displays, including differing DPI; verify cursor jump and per-monitor overrides. |

## Performance budgets

| Budget | Evidence available |
|---|---|
| Hook callback p99 under 50 µs | Moon matching/enqueue p99 is 9 µs. The full callback including downstream third-party hooks is 817 µs; the literal whole-callback budget is not established. |
| Eight-window key-to-placement median under 10 ms, p99 under 30 ms | 128/128 exact placements pass. Latest independent median/p99 is 14.149/55.182 ms; latency remains above budget. |
| 50-window reducer plus layout under 100 µs | Passed: 22.59–22.93 µs in the recorded Criterion run. |
| Idle CPU 0%, no timer faster than 1 Hz | Latest full-process idle sample is 0.312% of one core over ten seconds. Providers run at 1 Hz and unchanged scenes do not redraw; literal zero CPU is not established. |
| Working set under 50 MB, bar and borders on one 4K monitor | Full release process with eight windows, bar and borders: 48,029,696 bytes after the workload/idle interval. This passes for the serial WARP/composition-wait profile; final defaults still need selection. |
| Ready in under 300 ms | Full eight-window release candidate measured 120.019 ms. The actual signed installation still needs its own result. |

Detailed results are in [native integration](validation-integration.md),
[platform verification](validation-platform.md),
[placement comparison](validation-placement.md), and
[surfaces/providers](surfaces-and-providers.md). Update this ledger when those
documents gain measured results; never infer a pass from implementation alone.
