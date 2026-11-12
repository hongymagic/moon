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

## Performance investigation, 13 November 2026

The measurements below use eight owned test windows on the development
3840×2160 display at 125% scaling, with the default WARP renderer, bar and borders.
They do not establish acceptance of the signed installation or the seven-day
soak. Each report records the actual daemon's statistics; build manifests retain
the executable hashes.

### Controlled residency and resource reuse

`daemon_performance.exe moon.exe testwin.exe --memory-control` creates the same
eight windows as the resize workload, sends no shortcut input, waits its nominal
18.48-second workload interval, and observes a further ten seconds of idle time.
It verifies that the layout and native frames remain unchanged. The ordinary
mode sends 128 real resize chords and requires every resulting placement to
match. Both modes close their owned processes and restore the previous focus.

The probe classifies committed regions with `VirtualQueryEx` and queries valid
resident pages in bounded batches with `QueryWorkingSetEx`. It neither reads page
contents nor records addresses, faults pages in, or trims the working set.
Region observations are not atomic. The `Shared` bit describes shareable pages,
not exclusive ownership; their total is diagnostic evidence, not a substitute
for the process working-set acceptance metric. See Microsoft's
[VirtualQueryEx](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualqueryex),
[QueryWorkingSetEx](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-queryworkingsetex)
and [working-set block](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-psapi_working_set_ex_block)
documentation.

The diagnostic baseline is
`FE9DF83994064DF0DBF918C7E2368D4DAF0ACD9DB9AA306897542134E521A387`.
The resource-reuse candidate is
`B2ABB4179617CEAD58B5242457B742969D3B0E28C348970B203AE01E3E65810F`.
The latter retains one text format per canvas and one gradient brush per current
border slot, invalidating them when their relevant style changes. It retains no
history of layouts or removed windows.

| Candidate and workload | Working set before / after workload / after idle, bytes | External median / p99 | Gradient brushes created |
| --- | ---: | ---: | ---: |
| Baseline, no-input control | 46,051,328 / 51,412,992 / 51,417,088 | No input | 8 |
| Baseline, resize | 45,981,696 / 51,728,384 / 49,504,256 | 12.190 / 22.841 ms | 1,032 |
| Reuse, no-input control | 46,006,272 / 51,195,904 / 51,200,000 | No input | 8 |
| Reuse, resize | 45,846,528 / 56,434,688 / 55,042,048 | 11.789 / 38.915 ms | 8 |

Both resize runs completed 128 exact placements with no daemon error. Each reuse
run created one text format. All four runs used one graphics device and one
border atlas, with no recreation or failed draw. The new native resource test
also verifies font/size/color invalidation, ellipsis, gradient stops and gamma.

The no-input baseline's growth was mainly executable-image residency:
`moon.exe` rose from 3,141,632 to 8,740,864 resident bytes, while private-region
residency fell from 8,331,264 to 8,081,408. A snapshot taken about 0.16 seconds
after startup therefore is not a warmed steady-state baseline.

In the resize runs, the reuse candidate's higher final residency was primarily
anonymous mapped memory: 13,074,432 bytes versus 7,692,288. Private-region
residency differed by 172,032 bytes and executable-image residency was similar.
DirectComposition returned different backing-surface sizes across the runs.
Those temporary descriptors can refer to shared backing; their sizes cannot be
added to calculate live allocations or attributed to the eight cached brushes.
The resource-creation reduction is measured. Lower total memory or latency is
not established by this pair.

Four additional runs used the same immutable binaries in baseline/reuse/reuse/
baseline order. Every run completed all 128 exact placements without an error.
The initial summary wrapper rejected an absent optional field after saving the
first valid report and restored the ordinary daemon; the remaining three runs
then continued with fresh test processes.

| Repeat | External median / p99 | Working set after workload / idle, bytes | Brushes created |
| --- | ---: | ---: | ---: |
| Baseline 1 | 11.700 / 42.595 ms | 52,027,392 / 49,582,080 | 1,032 |
| Reuse 1 | 10.973 / 25.053 ms | 52,097,024 / 50,016,256 | 8 |
| Reuse 2 | 11.600 / 21.979 ms | 50,962,432 / 48,934,912 | 8 |
| Baseline 2 | 11.428 / 43.313 ms | 51,908,608 / 49,655,808 | 1,032 |

The earlier five-megabyte increase did not recur consistently. These small
samples support retaining the bounded resource reuse; they do not prove a
universal memory improvement, eliminate timing variability, or pass the median
latency budget. Reports are `target/abba-{1-baseline,2-reuse,3-reuse,4-baseline}.json`.

The resize candidates recorded own-hook p99 values of 105 and 97 microseconds,
respectively. Idle CPU samples were 0% and approximately 0.3125% of one core;
both no-input controls measured approximately 0.3125%. None establishes a
universal zero-CPU result. The failed latency and memory budgets remain open.

Local evidence: `target/{memory,reuse-memory}-{control,resize}.json`,
`target/{performance-diagnostics,render-reuse}-build.json`, corresponding native
unit logs and cleanup reports. The diagnostic baseline passed 25 native unit
tests; the reuse candidate passed 26. Four interactive hook cases were excluded
from these unit runs and retain their separate earlier evidence.

### Alt-mask dispatch attribution

The baseline resize run observed exactly one tagged callback inside every one
of 128 internal `SendInput` calls, with no absent, multiple or out-of-call tagged
callbacks. Median/p99 durations were 71/135 microseconds for `SendInput`, 16/38
before callback entry, 0/1 in the tagged callback handler, and 55/119 after its
handler returned. These percentiles are separate populations and are not
additive. The last interval includes diagnostic bookkeeping and Windows call
completion; it does not prove how much time was spent running versus waiting.
Existing own/whole-hook measurement boundaries still include this work.

`MOON_PROFILE_INPUT=1` enables optional dispatch profiling on Moon's own hook
thread, with zero hardware counters. Paired reads around the internal mask
record cumulative context-switch deltas and the union of interval wait reasons.
Nested reads are skipped so they do not reset the outer sample's bitmap. Errors
are visible in `hooks.phases.input_profile`; disabling the option makes no
profiling API calls. The thread-owned guard disables profiling when it exits.
Profiling overhead remains inside the existing hook measurements, so an
instrumented run is diagnostic evidence rather than an uninstrumented budget
comparison.

The implementation follows Microsoft's
[EnableThreadProfiling](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-enablethreadprofiling)
and [PERFORMANCE_DATA](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-performance_data)
contracts. Context-switch counts are cumulative; the wait-reason bitmap covers
the interval since the previous read. CPU cycle counts are deliberately unused.
A separate ordinary-token availability probe successfully enabled, read and
disabled dispatch profiling without requesting elevation. This establishes
availability on this host, not the duration or cause of a particular wait.

The opt-in daemon run used ordinary candidate
`8B7FC844DE555C337ACC2BF5630FF7E00EA90CC46126479DD980259AC326C5CB`.
All 128 internal masks had at least one context switch: 195 switches in total,
at most two per call, and aggregate wait-reason bitmap `0x08000000`. Enable and
read errors were zero; no nested samples were skipped. This establishes that
these calls yielded the hook thread. It does not measure the wait duration or
identify a contending thread. The in-handler median/p99 remained 0/1 microsecond;
the after-handler interval was 56/103 microseconds. All 128 native placements
matched, and the daemon reported no error. The profiling run's external timing
is deliberately excluded from the uninstrumented comparison above.

In Microsoft's [WDK 10.0.28000.2526 package](https://www.nuget.org/packages/Microsoft.Windows.WDK.x64/10.0.28000.2526),
`wdm.h` defines bit 27 as `WrResource`; the package/header hashes and extracted
values are retained in `target/wdk-kwait-reason.json`. This is the sole reason in
the observed bitmap, not a measurement of resource-wait duration, its owner or
subsequent scheduling delay. Higher priority cannot run a thread whose wait is
still unsatisfied ([Windows context-switch contract](https://learn.microsoft.com/en-us/windows/win32/procthread/context-switches)),
so this result does not justify changing Moon's thread priority.

Evidence: `target/dispatch-profile.json`, `target/input-profile-build.json` and
`target/render-profile-cleanup.json`. The cleanup report verifies no owned test
process or journal remained, the user's configuration hash was unchanged and
no previous window manager was running before the tested ordinary Moon build
was restored. This candidate passes 27 native unit tests; the unit command still
excludes the four explicitly interactive cases. Those four cases subsequently
passed in a separate isolated run with `MOON_PROFILE_INPUT=1`: 256 trigger
commands across 128 contiguous chords, all 52 default routes, 22 renewal
triggers, and key-pair ownership across repeats/modifier changes. Positive
controls verified that bare Alt+Shift actually changed the helper's keyboard
layout; consumed shortcuts retained it. Ordinary F24 reached the downstream
hook, while internal mask events did not leak. Report:
`target/input-profile-interactive.txt`. Its timing is correctness-test evidence,
not an uninstrumented performance measurement.

### Final larger-font correction

Visual review subsequently found that a fixed-width workspace button shortened
workspace 10 to `1…` at Arial 18. The corrected scene sizes workspace buttons for
their displayed text/icon and font, retaining the existing minimum and configured
width limit. The resulting ordinary daemon is
`D5831ED9F4C2DD5676D79EF1CD511D4EC379E582C5C4FFF7DADD6BBE5D006F4A`.
Its final uninstrumented run again completes all 128 exact placements with no
renderer or daemon error, one graphics device/atlas, eight gradient brushes and
one text format. Input profiling is explicitly disabled in the recorded stats.
The current readiness, latency, CPU and working-set results are tabulated in
[acceptance evidence](acceptance-status.md#performance-budgets); the failed limits
remain open. Raw evidence is `target/workspace-labels-performance.json`, alongside
the native units, lifecycle, carousel and scoped pixel review for that candidate.
