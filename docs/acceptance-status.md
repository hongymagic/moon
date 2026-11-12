# Acceptance evidence

This ledger maps the checks in `.claude/plans/moon-v1.md` to observed evidence.
It was reviewed on 13 November 2026 (Sydney). Implemented event handling and passing
simulated tests do not establish that the corresponding physical Windows event
has been exercised.

## Current polish candidate, 13 November 2026

The ordinary Windows daemon is
`E050F021CFA3643CAF33B0EE007525FD1676575D688A73706FC14A8979512BE5`;
Settings is
`EC22F24B07DFAD2ABE6198B519016E26E2C119843351FE81AEAC13EE9799727C`.
The corresponding UIAccess installation image is
`B7188D60E72E40ECB662E72E2D344E9EE840FEF8333BC5058941E3F9EFA75E25`.
Build identities are recorded in `target/polish-native-build.json` and
`target/polish-uiaccess-build.json`.

| Check | Observed result |
| --- | --- |
| Settings frontend | 31 browser scenarios pass, including initial normal/error-page readiness, dialog ownership, invalid-field recovery, keyboard save/focus, themes and narrow/short viewport reflow. |
| Portable and native units | 89 portable tests and 22 Windows native unit tests pass. Four interactive hook tests are explicitly ignored in this run; their earlier separate evidence is unchanged. |
| Native desktop behavior | 14 integration, four startup-assignment and ten carousel checks pass on this daemon. Carousel checks include focusing a hidden workspace, same-workspace focus without a slide, rapid switches and restoring original frames. |
| Native Settings | Eight checks pass with this exact Settings/daemon pair: save/apply, comment preservation, conflict retention, reload, Appearance and different-file isolation. The personal configuration is unchanged and cleanup reports no errors. |
| Setup | 32 isolated state/handler checks and 14 recovery checks pass. Install/Uninstall WPF views compile. Visual preview invokes neither Install nor Remove and requests no UAC prompt. |
| Build | Formatting, frontend type checking/build, and Windows-target Clippy with warnings denied pass. |

Reports: `target/polish-final-{integration,startup,carousel,desktop}.json`,
`target/polish-final-settings/report.json`, `target/polish-native-unit-results.txt`,
`target/settings-polish/report.json`, `target/setup-polish-validation.json`,
`target/setup-polish-recovery-validation.json` and `target/setup-polish-ui/report.json`.

The ordinary daemon is running as the sole window manager. Actual visits to
workspaces 1, 2 and 10 verified Arc, Windows Terminal, and ChatGPT plus Claude,
respectively, with exact visible native frames and correct hidden-window state.
The visits returned to workspace 10. See
`target/polish-final-workspace-visits.json`. No previous manager was restarted.

Settings now reveals after its initial page or error UI has been built. Both
native opening scenarios pass and close cleanly using owned temporary files.
Immediate `PrintWindow` captures can still precede WebView compositor drawing;
these checks do not prove a zero-blank-frame presentation. Evidence is retained
in `target/settings-opening-ready-after/` and
`target/settings-opening-invalid-after/`.

This candidate has not completed a signed installation. The physical resilience
matrix, performance budgets and seven-day soak below remain open; this polish
run does not supersede their measured results.

## Earlier SID-fix candidate, 12 November 2026

The earlier normal-permission daemon is
`DBADD0C08FA4C78AA1EF567520B593B7D3F68297BD3724C641CEDB082ECBCCAE`;
the matching Settings image is
`6A633A06B872765EA30AEDCBC9839F856FD3EA70BA5D855AC1BD85CCC8670E92`.
The UIAccess installation candidate is
`521E8BD9E926AF8108A722564452DF3970E5A935D496A7EF22D37AE865C1E219`.
Both applications and the installer now derive the IPC endpoint from the logon
SID. The measured linked UAC tokens had different authentication LUIDs but the
same logon SID; the earlier endpoint derivation separated those clients.

Four native IPC checks, including the linked-token identity check, and all 14
normal-daemon integration scenarios pass on this source/build. Evidence:
`target/sid-ipc-native-tests.txt` and
`target/live-sid-integration-results.json`. The matching Settings image also
passes all eight native checks with the normal daemon: save/apply, conflict
retention, reload confirmation, Appearance and different-file isolation. The
user's configuration was unchanged and cleanup recorded no errors. Report:
`target/sid-settings-ui/report.json`.

The ordinary live startup then adopted all four expected application windows:
Arc in workspace 1, Windows Terminal in workspace 2, and ChatGPT plus Claude in
workspace 10. At the settled observation, workspace 10 was active, its two
visible frames matched their placements, and Arc/Terminal were shell-cloaked.
The daemon remained medium integrity without UIAccess. See
`target/live-workspace-after.json` and `target/live-workspace-settled.json`.
Actual visits to workspaces 1, 2 and 10 then verified each expected visible set
and exact native frames, returning to 10; see `target/live-workspace-visits.json`.
Four isolated startup checks also pass for pre-existing windows' individual
workspace rules, native reveal/hide and original-frame restoration:
`target/startup-assignment-native-results.json`.

Installation is authorized. Earlier accepted installation attempts failed;
the diagnostic retry recorded rollback, and the retained diagnostic logs report
a signed-daemon readiness timeout. The latest SID-fix attempt was canceled at the Windows consent prompt;
the corrected launcher preserved cancellation code 1223 and handover recorded
`rolled_back_stopped`. Evidence is in
`target/live-install-sid-cancellation-fixed.log` and
`target/live-install-sid-cancellation-fixed-state.json`. This UIAccess candidate
has not completed installation or signed runtime
verification. [Installation evidence](installation.md#live-installation-attempts-and-sid-fix)
records the distinct outcomes. The requested no-fallback mode keeps the previous
desktop processes stopped during rollback. Startup reliability, signed privilege
checks, the physical resilience matrix and the seven-day soak remain open.

## Earlier product polish evidence, 12 November 2026

The recorded normal-permission Windows candidate is
`075E96CD3C7C95E24D04F07EC9BCFF7D53F74F60E2F6AC53B23B179C7DFBB134`.
Settings is
`7901563646D88335A3882DC0604691F5868D5F4BEA625D9C0D1479368A7BE050`.
The corresponding UIAccess installation image is
`9B85EBEEFD8404AC8E1FCB2F5395883D81CF1ADD8E7CCFE79488F7DD503B1546`;
its application evidence below predates the subsequent installation attempts.
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

These measurements do not establish signed-install, elevated-window, physical
resilience or seven-day-soak acceptance. Rows below retain their candidate
identity; they are not promoted to measurements of the SID-fix binary.

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
| M5: installation | The initial cancellation had verified full rollback. Later accepted attempts failed; the diagnostic retry recorded rollback and a readiness timeout. The latest SID-fix consent cancellation retained code 1223 and recorded `rolled_back_stopped`. [Attempt evidence](installation.md#live-installation-attempts-and-sid-fix) keeps these outcomes separate. | Complete installation and signed runtime verification of the current SID-fix candidate, then verify a running upgrade/rollback. Installation authorization exists; Windows consent is still required for an elevated attempt. |
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
| UAC prompt | Actual consent prompts were entered during accepted and canceled installation attempts; controlled before/after hook and focus checks were not recorded. | Verify no unwanted focus and that normal bindings resume after leaving the secure desktop. |
| RDP session | Session-scoped journals implemented; RDP not exercised. | Connect/disconnect RDP, verify separate session ownership/recovery and restored local display geometry. |
| Native virtual desktop switch | Suspension and persistent-tree tests pass; physical switch not exercised. | Switch native desktops with managed windows on both; no forced switch back, off-desktop uncloak or membership loss. |
| WSLg windows | Host `.wslconfig` explicitly sets `guiApplications=false`; the distro has no X11/Wayland display. WSLg 1.0.79 and `xmessage` are installed, but enabling GUI support requires restarting WSL, including this build environment. | Tile, focus, hide, restore and close a real WSLg application. |
| WinUI 3 windows | Paint 11.2609.401.0 passed exact tile/float placement, hook-driven focus, hide/show, original-frame restoration and owned-window cleanup on final candidate 1A7F5020…. Its loaded Microsoft.UI.Xaml.dll version3 and medium token were measured; [application evidence](validation-applications.md) records both. | Application-window behavior is covered; WinUI 3 transient dialogs require a separate measured scenario. |
| Fullscreen games | Native borderless helper transition passes. | Exercise an actual fullscreen game and return to tiling without a persistent bar/border or manual-fullscreen flag change. |
| Multi-monitor actions | Pure monitor/workspace movement tests pass; physical multi-monitor not exercised. | Move windows and workspaces between real displays, including differing DPI; verify cursor jump and per-monitor overrides. |

## Performance budgets

These are the last measured budgets for candidate `075E96CD…`, not measurements
of the current SID-fix candidate. The IPC correction does not establish a change
to any failed performance budget.

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
