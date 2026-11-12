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

## Product-polish candidate, 12 November 2026

Normal candidate
`EA798913C35FCB9CC962E08B9F4DB20719D660E502834DA1F66A97508781BBD0`
was measured after the Settings, bar interaction and workspace-animation changes.
The freshly rebuilt probe was
`DB3C0B5FD939FC17347115E6024FA79E14AED6714CB3BB1B34EBD28AD2F20CDA`.
The physical monitor was 3840×2160 at 125%; bar and borders used the default WARP
configuration, with all transient experiment overrides removed. Settings was
closed, and no builds or other agent desktop work ran during the sample.

| Measurement | Observed value |
|---|---:|
| Exact eight-window placements | 128/128 |
| Ready | 193.134 ms |
| Independent trigger-key-to-placement median / p99 | 13.226 / 35.112 ms |
| Internal key-to-observed-placement median / p99 | 12.924 / 34.980 ms |
| Working set before input | 45,535,232 bytes |
| Working set immediately after workload | 51,380,224 bytes |
| Working set after ten seconds idle | 49,192,960 bytes |
| Idle CPU, percentage of one core | 0.31248% |
| Hook own work p99 / whole callback p99 | 139 / 753 µs |
| Handles after idle / GDI objects | 348 / 8 |

Readiness meets the 300 ms budget. The idle working-set sample is below
50,000,000 bytes, but the workload sample exceeds it: this is not an unconditional
memory-budget pass. Placement and hook latency still exceed their specified
budgets, and literal zero idle CPU is not established. No daemon error was
reported. All owned processes exited and the recovery journal was absent after
cleanup.

Raw metadata, distributions and cleanup are in
`target/postpolish-performance/{metadata,full-daemon,cleanup}.json`. This candidate
still dynamically linked the Visual C++ runtime; the final statically linked
packaging candidate needs a separately identified measurement. This workload
resizes tiles inside a workspace and does not time the workspace transition.
The compositor probe's intentionally 200 ms motion is recorded separately in
[carousel validation](validation-carousel.md).

## Final static-runtime candidate, 12 November 2026

The current normal candidate is
`075E96CD3C7C95E24D04F07EC9BCFF7D53F74F60E2F6AC53B23B179C7DFBB134`.
It statically links the C runtime so first launch does not require a separate
Visual C++ redistributable. Fresh probe
`980A1DD971A3A35E0476571BAAD8BA3788B2E88AAF2DDF3E52D3535D713CA794`
used the same eight-window, 128-chord protocol, 4K/125% monitor and default
bar/borders configuration. No concurrent builds or agent desktop actions ran.

| Measurement | Observed value |
|---|---:|
| Exact eight-window placements | 128/128 |
| Ready | 129.901 ms |
| Independent trigger-key-to-placement median / p99 | 12.665 / 32.790 ms |
| Internal key-to-observed-placement median / p99 | 12.345 / 32.646 ms |
| Working set before input | 45,682,688 bytes |
| Working set immediately after workload | 54,898,688 bytes |
| Working set after ten seconds idle | 52,674,560 bytes |
| Idle CPU, percentage of one core | 0.62497% |
| Hook own work p99 / whole callback p99 | 140 / 719 µs |
| Handles after idle / GDI objects | 348 / 8 |

The final candidate passes readiness and placement correctness, but **fails**
the 50,000,000-byte working-set limit and the stated placement/hook latency
budgets. Literal zero idle CPU is not established. Earlier smaller working-set
samples do not override this current result. `last_error` was null throughout;
all owned processes exited and the recovery journal was absent afterward.
Raw evidence remains in
`target/postpolish-performance/static-crt/{metadata,full-daemon,cleanup}.json`.

### Scope of the regression diagnosis

The renderer reports exactly 2,865,156 nominal surface bytes in every phase of
both polish runs; handles and GDI counts also match. The new hover-feedback
surface contributes four nominal bytes. Workspace animation never activates in
this resize workload, and Settings is a separate, closed process. The static
candidate starts only 147,456 bytes above the dynamic candidate before input,
but ends 3,481,600 bytes higher after idle. Those samples locate the growth after
the workload; they do not establish that static linking itself caused the
retained allocation. No working-set trimming was performed.

Command reduction, journaling and submission remain small: the static candidate's
128 command transactions have median 1.696 ms and p99 3.427 ms, with median native
apply submission 1.341 ms. Input-to-dispatch median is 0.280 ms. Independent
native outer-frame completion is about 12.419 ms median and 32.604 ms p99, close
to DWM visible-frame completion. The remaining delay is therefore not explained
by late daemon observation alone. Compared with the earlier pre-polish default
profile (14.415/40.263 ms), current placement latency improved while still missing
the required budget. The data does not identify a new avoidable animation loop
or a concrete allocation fix; another measurement should test a specific cause
rather than select a favorable sample.

Separately, six four-window workspace transitions with bar and borders enabled
were measured on the same candidate. The observed peak working set was
47,575,040 bytes, and direct-command-to-owned-overlay-disappearance ranged from
251.642 to 269.553 ms. These are bounded visual transitions, not the hotkey
placement workload or proof of a universal memory bound. Their timing definition,
sampling limits and correctness evidence are in
[carousel validation](validation-carousel.md).

## Hook phase diagnosis, 12 November 2026

Diagnostic normal candidate
`91B77064A36650FEA7E55F02305DF5BD307DBF05C580DB71A0AA63438CC56E91`
and probe
`B16929FF513D9B928B9B56FE94C995E756DD4EE62F0E5116F886F9EA5CD0CE69`
retain the existing callback timing boundaries and add bounded aggregate phase
timings. Instrumentation overhead remains included in the overall measurements;
no key codes, scan codes or character contents are recorded. All phase counters
were zero before the 128-chord workload.

| Phase | Samples | Median | p99 | Maximum |
|---|---:|---:|---:|---:|
| State/ownership lookup and binding match | 768 | 1 µs | 2 µs | 3 µs |
| Synchronous Alt-mask `SendInput` call | 128 | 72 µs | 93 µs | 108 µs |
| Enqueue and consumed-key bookkeeping | 128 | 5 µs | 9 µs | 9 µs |

The mask call is the dominant measured phase. Its duration includes Windows and
reentrant input dispatch; it is elapsed time, not a measurement of Moon CPU time.
Overall own-work p99 was **94 µs**, and whole-callback p99 was **691 µs**, across
896 callbacks. These populations differ from the phase populations, so their
percentiles cannot be added or subtracted. The lower total than the earlier
140 µs run does not establish an optimization: this change only adds diagnostics.

All **128/128** eight-window placements were exact, with readiness **118.496 ms**.
Independent trigger-to-placement median/p99 was **11.905/25.367 ms**: this run
meets the 30 ms p99 criterion but still misses the 10 ms median criterion. Both
hook timings exceed the 50 µs budget. Working set was **45,936,640 bytes** before
input, **60,436,480** afterward and **58,507,264** after ten seconds idle, failing
the 50,000,000-byte limit. Idle CPU was **0.31248%** of one core. Nominal surface
storage stayed at 2,865,156 bytes; idle handles/GDI counts were 348/8. No daemon
error was reported, and cleanup verified no owned Moon/testwin process or
recovery journal remained.

Evidence: `target/hook-phase-build.json`, `target/hook-phase-performance.json`
and `target/hook-phase-performance-cleanup.json`. The data supports investigating
time inside the mask call before changing binding lookup or command delivery.
A targeted context-switch trace can distinguish thread execution from waiting
and readiness delays; Microsoft documents this in
[CPU Usage (Precise)](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/cpu-analysis#cpu-usage-precise-graph).
It does not justify deferring the mask, which previously failed rapid-chord
language-switching checks, or attributing the remaining memory growth to a
specific allocation without further evidence.
