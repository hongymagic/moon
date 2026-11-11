# Windows integration validation

Validated on 12 November 2026 on the user's Windows 11 machine, from WSL,
using the native MSVC release build and four real `testwin` windows. Earlier
runs ran beside GlazeWM with `testwin` explicitly ignored; the final run had
GlazeWM stopped. It managed no other applications.

Run `just integration` to reproduce the scenario. On a Windows build machine:

```powershell
./scripts/integration.ps1 -BuildDirectory ./target/release
```

The script writes `target/integration-results.json` when `-BuildDirectory` is
used. Process logs are retained in the printed temporary directory. The recorded
local run is also copied to `target/integration-results.json` and
`target/integration-logs/`. Each run keeps its own evidence directory so an old
failure cannot be confused with a later pass. Evidence includes executable
SHA-256 hashes, OS version and UTC timestamps. Build outputs and machine-specific evidence are
intentionally excluded from source control.

## Passed native scenarios

| Scenario | Evidence checked |
|---|---|
| Background launch | The launcher exits, both redirected output streams close, and its independent child reports ready. The configuration path contains spaces. |
| Four windows tile | DWM visible-frame rectangles equal committed placements within one physical pixel. |
| Standalone extension | `examples/workspace-state.ps1` queries all ten workspaces using the documented named pipe and NDJSON protocol, without invoking the moon CLI. A close request with the wrong expected focus is rejected, preserving all four native HWNDs and the managed set. |
| Minimize and restore | A minimized window stops receiving placement effects, retains its persistent tree slot, and returns to tiling after restore. |
| Focus, move, resize and direction | Commands complete and their resulting native rectangles match the committed layout. |
| Float and fullscreen | State transitions and native rectangles agree, including return to tiling. |
| Native borderless fullscreen | A helper becomes captionless and covers its monitor; moon detects it without setting the manual fullscreen flag, then returns to tiling when its normal style is restored. |
| Workspace move and back-and-forth | Move-and-follow shows one destination window and hides three source windows; returning shows the three source windows. Hidden windows are checked through DWM cloak state or the documented parking fallback. |
| Live reload | An unknown field reports an error and leaves the previous configuration active; a valid inner-gap change applies without restarting. |
| Restart | A new process adopts the journal with identical workspace trees, active workspaces, focus and target rectangles. |
| Close | Closing the sole destination window removes it from state and reflows the surviving windows. |
| Forced termination | The watchdog restores every surviving window to its original visible frame and clears the recovery journal within one second. |
| Background resume | `start --background --resume` adopts a saved journal with identical trees, focus and target rectangles, then restores original frames on clean exit. |
| Startup failure | A deliberately conflicting named pipe makes resumed startup fail before the main loop. An inherited recovery journal restores an owned window parked offscreen and is removed after successful recovery. |

All fourteen passed in the complete local run using candidate
`300A014A860C530C328947947C671828EB360A6749D888D08054AB47E278C06B`,
started at `2026-11-11T22:23:21.5908320Z` (12 November local time). The test closes
only its own helper process and daemon; recovery runs in its cleanup path even
after an assertion failure.

A later candidate's rerun stopped at the native fullscreen scenario because an
elevated Settings window retained foreground. The owned helper had the correct
borderless monitor bounds, but was not the actual foreground window; moon
correctly did not classify it as foreground fullscreen. That failed run remains
under `target/integration-regressions/elevated-foreground`. The harness now checks
native foreground separately after its focus command and before fullscreen
mutation. A complete rerun of the final candidate remains required after that
owned elevated window is cleared.

The focused `-GuardOnly` run also passed on 12 November using candidate
`DC3B2A25CEEA758939C8EA07387AE6440BB817F780A502784C82C54DA7096594`.
It exercised actual named-pipe command rejection with a different live managed
HWND as `expected_focus`, checked the error response and independently verified
that no native helper or managed entry disappeared. Its three-case result and
raw response are retained in `target/guard-results.json` and the associated
integration evidence directory. This shorter mode exits through the same
recovery and owned-process cleanup as the full suite.

## Defects found and verified during integration

- The pipe server disconnected immediately after a buffered response write,
  discarding unread bytes for slower clients. The server now waits for client
  closure with a bounded, cancellable read. The standalone extension and a
  Windows regression test using a delayed 128 KiB response verify delivery.
- Stale foreground notifications could switch back to the previous workspace
  after a move-and-follow command. Foreground events are now checked against the
  current native foreground window before reduction. The workspace scenario
  passes with this check.
- The PowerShell harness now starts native clients with explicit redirected
  streams, handles JSON arrays consistently in Windows PowerShell 5.1, and
  identifies helper windows by class rather than counting a console window. Native geometry assertions explicitly select per-monitor DPI v2 so PowerShell cannot scale physical pixel requests a second time.
- Background children inherited unused copies of the launcher's redirected
  streams, so callers waiting for EOF hung after the launcher had exited. Internal
  process creation now clears inheritance on the original standard handles before
  Rust creates the selected child streams. The background scenario waits for both
  process exit and stream closure.
- A terminated process with another open handle still exposed its creation time.
  Treating that identity match as proof of liveness made crash recovery refuse
  to run. Process matching now checks the process handle's signaled state before
  comparing creation time; the repeated forced-kill scenario passes.
- A resumed process could claim its journal before its replacement watchdog
  existed, then fail during hook, pipe or graphics startup. A recovery guard now
  covers startup errors while the instance lock remains held, and the watchdog
  starts before ownership is saved. A real pipe-collision failure verifies that
  inherited parked windows are restored.
- Moving checkpoint writes off the message loop exposed Windows file-sharing
  collisions with observers reading the journal. Readers now allow atomic
  replacement, and replacement retries only sharing/access failures for a
  bounded interval. A failed restart handoff terminates its waiting successor
  before normal recovery. The failed run is retained separately in
  `target/integration-regressions/journal-sharing/`; the corrected candidate
  passed every scenario, including exact restart state.

The current writer keeps one active write and at most one pending latest
snapshot. Original-window changes pass a durable barrier before native effects;
explicit restart flushes the complete latest state. Shutdown joins the writer
before restoring windows or removing the journal. The integration fixture waits
for its asynchronous checkpoint to match the queried world before copying it.

## Core and parser checks

The Linux-native core suite passed 51 tests after the parity audit,
including 100,000 randomized command sequences that assert membership, focus,
positive in-bounds rectangles and non-overlap. Tests cover configuration merging,
strict unknown-field rejection, comment-preserving migration, alias-cycle
rejection, stock rules, workspace changes, display removal, suspension and
restart serialization. The parity audit also checks per-monitor and per-workspace
overrides at 125% scaling, original dialog sizes, explicit monitor changes during
reload, every default binding, provider-pill snapshots, custom widget output,
partial editor schemas, light/system palette behavior and independent native-fullscreen suppression.

A read-only comparison of the live GlazeWM configuration found **46 active
chords**, all present in moon's 52 defaults, with **zero normalized command
mismatches**. Equivalent aliases and GlazeWM's two-command move-then-focus
sequence were compared with moon's single move-and-follow command. Moon adds
four directional monitor-move chords, pause and the cheat sheet. This checks
configuration parity; the separate live-hook acceptance remains explicit in
the acceptance ledger.

Twelve ordinary Windows-native regression tests have passed: hotkey mapping, invalid
hotkey rejection, delayed large responses, stalled-reader cancellation,
subscription framing, and process liveness while a terminated process object
still has open handles, plus temporary and permanent atomic-replacement sharing
conflicts. The additional tests cover cached COM apartment lifetime, border-atlas
pixel coverage, canonical modifier release and preserving the still-held side
when the other modifier is released. The sharing tests verify retry success after
a reader closes and preservation of the original bytes when the conflict persists.
The separately invoked interactive hook test also passes all 22 modified/plain
trigger checks across initial held modifiers and ten renewals, without changing
the helper's input language; see [platform evidence](validation-platform.md).

Nine portable CLI/journal tests pass. Five exercise the background writer's
durable barrier, coalescing, write-error propagation, worker-panic wakeup and
shutdown ordering, including proof that a queued snapshot cannot write after
the writer has joined.

Criterion measured reducer plus layout for 50 windows at **22.59–22.93 µs**
(30 samples, one-second warm-up and two-second measurement), below the plan's
100 µs budget on this WSL host. This is a core benchmark, not a claim about full
native key-to-placement latency.

The protocol fuzzer completed **1,610,171 runs in 31 seconds** and the
configuration fuzzer completed **6,845 runs in 31 seconds**, without a failure.
The repository contains both targets and a pinned nightly CI job. Fuzzing is
bounded evidence, not proof for every possible input.

## Remaining time-dependent acceptance

This run does not establish seven days of daily-driver stability, a clean
Hyper-V first-run result, mixed-DPI multi-monitor visual review, RDP behavior,
or every row of the plan's physical resilience matrix. Those acceptance checks
must retain their actual measured status. In particular, a short integration
run is not the required week-long soak.
