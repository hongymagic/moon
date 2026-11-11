# Full-application performance evidence

The performance probe starts a release daemon with eight owned test windows,
the real keyboard hook, providers, bar and borders. Its temporary configuration
ignores every other process. It refuses an existing daemon and stops input if
foreground leaves an owned helper. Cleanup exits the daemon and closes its
helpers. No working-set trimming or subtraction of shared pages is used.

`daemon_performance.exe moon.exe testwin.exe` sends 128 alternating built-in
Alt+Shift+Minus/Plus chords, exercising both width and height commands. It records
the daemon's trace, actual working set/handles/GDI counts, readiness, and CPU over
ten seconds after the input workload. The latest probe also independently measures
injection-to-exact-frame completion and records resources before input. Those
measurements distinguish actual placement from when the main loop observes it.

## First full run: budget failures, 12 November 2026

Binary SHA-256:
`e4e2b18984f00e212dd45e7bb76bef0823cc91f1aec8cbfb048fbfad8cef1e03`.
This candidate used serial WARP, bounded composition completion, a shared D2D
context and a noncopying embedded-font reference, but still recreated border
surfaces during resize and synchronously checkpointed every command.

| Measurement | Observed value |
|---|---:|
| Input chords completed | 128/128 |
| Internal key-to-observed-placement median | 48.867 ms |
| Internal key-to-observed-placement p99 | 402.353 ms |
| Ready | 366.478 ms |
| Working set after input | 106,438,656 bytes |
| Working set after idle interval | 100,311,040 bytes |
| Idle CPU, percentage of one core | 0.625% |
| Handles after input / after idle | 367 / 357 |
| GDI objects | 0 |
| Hook compare/enqueue work p99 | 9 µs |
| Complete hook chain p99, including downstream hooks | 1,026 µs |

The command trace contained 256 commands with a 20.464 ms median transaction and
a 366.780 ms maximum. This failed the latency, readiness and memory budgets.
It prompted asynchronous bounded checkpoints, a single durable initial adoption,
an apartment-scoped virtual-desktop cache, and retained border atlas storage.
New original-window records still require a completed durable write before any
native effects; restart waits for its complete handoff record.

Raw first-run evidence is in `target/first-full-daemon-results.json`. Later
measurements must identify their binary and cannot inherit a pass from this run.

## Cross-process input transparency

The actual transparent underlay was deliberately placed above an owned test
window. Physical mouse input at the left, right, top and bottom ring positions
reached that separate process exactly once each. `WindowFromPoint` also returned
the underlying receiver for all four positions. The probe verifies styles and
the intended recipient before sending input and restores the pointer afterward.
Raw evidence is in `target/first-full-input-results.json`.

The rejected disabled/non-layered experiment did not deliver cross-process input;
the production renderer retains the documented layered/transparent combination.

## Retained atlas and coalesced resizes, 12 November 2026

Candidate `73da24dff21c1e9149c60b6468e5b297ceba2e983217bac418949b2510beb207`
completed all 128 chords, with all eight exact frames after each chord. Startup
was 120.019 ms. Working set was 43,929,600 bytes before input and 48,893,952 bytes
after the idle interval, below the strict 50,000,000-byte budget. Nominal RGBA
surface storage stayed at 2,865,152 bytes throughout; this diagnostic excludes
compositor copies and caches. Handles ended at 342, with zero GDI objects.
Idle CPU was 0.469% of one core. Hook matching/enqueue p99 was 9 microseconds;
the entire callback including downstream third-party hooks was 817 microseconds.

Latency still failed: independent trigger-key-to-exact-placement median/p99 was
15.322/55.868 ms. Whole-chord median/p99, including modifier preparation, was
16.226/56.796 ms. Internal observation was 21.895/55.220 ms. Command application
itself had a 1.404 ms median. Raw evidence is
`target/coalesced-full-daemon-results.json`.

The probe now prepares modifiers separately and reports both trigger-key and
whole-chord timings. Inter-trial intervals range deterministically from 95 to
195 ms, covering refresh phases rather than synchronizing every trial to nine
60 Hz frames. Earlier measurements used a single injected chord and fixed
150 ms intervals; they are preserved and must not be compared as identical
benchmark protocols.

This phase-distributed workload exposed a real maintenance race: three checks
coincided with freshly posted moves and incorrectly floated a responsive helper.
`SWP_ASYNCWINDOWPOS` posts work to the owning thread, so a successful call does
not mean its frame has settled. Maintenance now grants a newly posted move
100 ms before counting refusals, resets the count for a changed target, and
retains the bounded retries for genuinely unresponsive windows. The failed
trace is retained in `target/perf-failure.json`; the 128-trial run above passes
with the correction.

Reference: [Microsoft SetWindowPos flags](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos).
