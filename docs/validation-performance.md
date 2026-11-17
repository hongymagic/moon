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

The normal candidate recorded at this milestone was
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

That candidate passed readiness and placement correctness, but **failed**
the 50,000,000-byte working-set limit and the stated placement/hook latency
budgets. Literal zero idle CPU is not established. Earlier smaller working-set
samples do not override that recorded result. `last_error` was null throughout;
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
profile (14.415/40.263 ms), that candidate's placement latency improved while still missing
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

The measurement probe now also records `working_set_bytes` and
`peak_working_set_bytes` from `GetProcessMemoryInfo` using
`PROCESS_MEMORY_COUNTERS_EX`, sampled once after each region walk on the same
process handle. These are Windows' current and peak working-set byte counters,
separate from the sum of classified resident pages. The peak is not reset at an
interval boundary; it can include earlier activity in that process and does not
establish a bound for future activity or other workloads. This reporting-only
addition supplies no new native measurement and changes no historical result.
See Microsoft's [counter structure](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
and [query contract](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getprocessmemoryinfo).

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
That D583 run records readiness 134.267 ms, external placement median/p99
12.183/43.941 ms, own/whole-hook p99 100/743 µs and approximately 0.3125% idle
CPU. Working-set samples are 46,780,416 / 51,515,392 / 49,446,912 bytes before
input, after input and after idle. Current-candidate budgets are tabulated in
[acceptance evidence](acceptance-status.md#performance-budgets); the failed limits
remain open. Raw D583 evidence is `target/workspace-labels-performance.json`, alongside
the native units, lifecycle, carousel and scoped pixel review for that candidate.

### Separate composition surface pools

The next candidate gives the bar and its feedback surface one retained
DirectComposition surface factory and the border atlas another. Both factories
use the existing DXGI device, Direct2D context and composition transaction. Atlas
geometry, rendering, window styles and pre-draw waiting are unchanged. Microsoft
documents allocation pooling per factory; this permits separation but does not
guarantee smaller backing allocations or lower working set.
[CreateSurfaceFactory](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice2-createsurfacefactory).

The immutable build identities in `target/surface-pools-build.json` are:

| Artifact | SHA256 |
| --- | --- |
| Ordinary candidate `moon.exe` | `E9A8438CBB39D662CD3C894C9E6F973B7F522E2BB0A87B5C1ECBC1C1670B5EB9` |
| `daemon_performance.exe` | `E3416FA3E52B2F011331FF7CCB5599DBB9788D6638468CB8F71B860C52626B63` |
| `testwin.exe` | `71425F3BB9E36060917863DEB9CBFAE386EE8B83A5A5B742734CDA93022A934D` |
| Native unit executable | `7243F19785630651027C53940EB5CB7CCA3EF9138E64367A310F16908FAE6023` |

The baseline is the D583 larger-font candidate identified above. Native units
pass 28/28, with four explicitly interactive tests excluded. The new test draws
through both factories using the shared context, then commits and waits through
the existing composition interface. It creates no window, so it establishes API
compatibility rather than visible output or a memory improvement. Evidence:
`target/surface-pools-native-units.txt`.

Four fresh processes ran in baseline/candidate/candidate/baseline order on the
same 4K, 125% display, followed by one no-input control for each binary. Each
resize run completed 128 exact placements with no daemon or renderer error.
Both controls preserved all eight native frames through the matched
18.481-second interval and ten seconds idle. These runs originated from the
Explorer context used by ordinary Moon, avoiding the previously observed host
AppData redirection. No user configuration was changed.

| Run | Working set before / after workload / after idle, bytes | External median / p99 | Idle CPU, % of one core |
| --- | ---: | ---: | ---: |
| Baseline 1 | 46,272,512 / 52,232,192 / 50,225,152 | 11.531 / 24.578 ms | 0.469 |
| Candidate 2 | 46,088,192 / 54,824,960 / 52,871,168 | 12.275 / 24.280 ms | 0.312 |
| Candidate 3 | 45,953,024 / 48,914,432 / 47,005,696 | 11.908 / 27.641 ms | 0.156 |
| Baseline 4 | 46,014,464 / 51,523,584 / 49,487,872 | 12.552 / 37.450 ms | 0.156 |
| Baseline no-input control 5 | 46,276,608 / 46,313,472 / 46,313,472 | No input | 0.156 |
| Candidate no-input control 6 | 46,239,744 / 45,998,080 / 46,006,272 | No input | 0.000 |

The separate region snapshots show a consistent reduction in anonymous mapped
**resident** bytes across these resize repeats:

| Run | Mapped resident bytes after workload / idle | Mapped committed bytes after workload / idle | Executable image resident bytes after workload |
| --- | ---: | ---: | ---: |
| Baseline 1 | 10,149,888 / 8,052,736 | 19,304,448 / 17,207,296 | 3,149,824 |
| Candidate 2 | 7,675,904 / 5,578,752 | 21,401,600 / 19,304,448 | 8,757,248 |
| Candidate 3 | 7,688,192 / 5,591,040 | 21,401,600 / 19,304,448 | 3,162,112 |
| Baseline 4 | 10,162,176 / 8,065,024 | 19,304,448 / 17,207,296 | 3,149,824 |

Mapped residency is approximately 2.47 MB lower in both candidate runs, while
mapped commitment is 2,097,152 bytes higher. This is a measured residency
tradeoff, not evidence that total allocation decreased. Candidate 2 also brings
its entire 8,757,248-byte executable image into residency; the other resize runs
retain approximately 3.15 MB. That accounts for the higher total despite lower
mapped residency. It does not identify why those pages became resident or
permit their subtraction from the process budget. The region snapshots remain
non-atomic and are not substitutes for `WorkingSetSize`.

Each candidate's 129 border draws receives a 1024×512 backing descriptor.
Both baselines additionally receive 1056×704 and 3842×704 backing descriptors.
The bar still receives 3842×704 descriptors during resize in both variants;
neither control does. Descriptor sizes are not allocation identities and must
not be added. Nominal surface bytes remain 2,865,156 throughout, with one device,
one border atlas, eight gradient brushes, one text format and no recreation.
The candidates report two factories. After ten seconds idle, GDI objects remain
eight and handles are 350 for the candidates versus 348 for the baselines.

The results support continuing visual and lifecycle verification of the two
factories. They **do not pass the strict 50,000,000-byte memory budget**: candidate
2 exceeds it both after input and after idle. Neither the favorable repeat nor
the 0% CPU control establishes a universal result. Median placement latency and
the remaining performance requirements also remain open. Candidate-specific
visual, lifecycle and carousel acceptance is not established by these profiles.

The initial wrapper stopped while displaying a timing field absent from
no-input reports after saving valid baseline control 5. A retry then flattened its argument
array incorrectly and could not find the probe executable. Both were harness
failures; cleanup restored the normal desktop, and the corrected retry produced
valid candidate control 6. They are not recorded as product failures. Retained
evidence includes `target/surface-pools-{1-baseline,2-candidate,3-candidate,4-baseline,5-baseline-control,6-candidate-control}.json`,
the profile/control cleanup reports and normal-desktop restoration reports.

The same E9 candidate subsequently passes all 14 native lifecycle scenarios in
`target/surface-pools-integration.json` and all ten workspace-carousel checks in
`target/surface-pools-carousel.json`. These include exact native placement,
reload, restart, recovery, workspace visibility and restoration. The visual
guard's six automated checks also pass. Review of all 13 PNG hashes and images
in `target/surface-pools-visual/` is **partial**, however: the initial focused
window's top ring is not visible in the saved pixels. Seven other ring captures
have continuous edges and corners; recoloring, focus changes, slot removal and
bar text pass the scoped inspection. The five bar captures and two full-width
survivor captures match the prior D583 image hashes. This observation does not
attribute the initial missing edge to the renderer; a temporary inspection-order
race remains to be checked. `visual-review.json`, `pixel-review.json` and the
capture report retain that incomplete result. The scope is one 125% display with
an explicit inspection-underlay order, not normal product Z-order or other
physical DPI settings.

### Observer timing and target-window callbacks

The first two surface-pool runs retain 128 observer records apiece, in trial
order. Probe `E3416FA3E52B2F011331FF7CCB5599DBB9788D6638468CB8F71B860C52626B63`
times its existing trigger injection and native/DWM-frame readback calls without
changing their order, short-circuit behavior, sleeps or completion criterion.
Every record matches the existing sorted completion arrays; call counts plus
short-circuit skips account for every attempted eight-target pass. The daemon
reports no error in either run.

| Run | External completion median / p99 | Native outer observation median / p99 | DWM-frame reads, cumulative per trial median / p99 | Native reads, cumulative per trial median |
| --- | ---: | ---: | ---: | ---: |
| D583 baseline 1 | 11.531 / 24.578 ms | 11.346 / 24.264 ms | 2.893 / 15.408 ms | 5.6 µs |
| E9 candidate 2 | 12.275 / 24.280 ms | 11.895 / 23.232 ms | 3.239 / 10.887 ms | 5.4 µs |

These percentiles describe separate populations and cannot be subtracted to
derive a placement duration. The paired candidate trial 30 is more informative:
external completion is 49.274 ms and native observation 48.941 ms, while the
DWM-frame wrappers consume 40.717 ms, including one 28.005-ms call. Sleeps consume
8.144 ms and trigger injection 0.393 ms. Native reads share this observer thread
and cannot run while its DWM-frame call is pending. Similar native and DWM
completion observations therefore do not independently exclude delayed
observation. Wrapper timings include the existing DWM-to-GetWindowRect fallback
and possible descheduling; they do not identify a kernel wait or prove when the
target geometry changed. Evidence: `target/surface-pools-1-baseline.json` and
`target/surface-pools-2-candidate.json`.

A separate opt-in target run keeps the same immutable E9 daemon
`E9A8438CBB39D662CD3C894C9E6F973B7F522E2BB0A87B5C1ECBC1C1670B5EB9`, with
probe `41E4AD28EE8789FE806509BEFA055255F9C8BFD8888AEF656937BF4379E271A8` and
helper `664F516E8D0BA321A912EE5EA62626D23B70AFA27CB041A3B61144097058B77B`.
`MOON_PERF_PLACEMENT_TIMING=1` enables six numeric properties per owned window:
an odd/even publication sequence, QPC timestamp and four actual outer edges.
The helper publishes after `DefWindowProc(WM_WINDOWPOSCHANGED)` and
`GetWindowRect`; the probe verifies ownership, consistent sequence, exact
rectangle and freshness. It reads metadata before a trial or after recording
all existing completion timings. Unchanged targets contribute no old timestamp.
The QPC frequency is 10,000,000 Hz on this host. These are same-system elapsed
comparisons, subject to Microsoft's one-tick cross-thread ordering uncertainty.
[QPC contract](https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps).

All 128 trials pass, with all eight targets changed in each trial and all 1,024
samples fresh and complete. The latest target callback per trial has median/p99
11.886/40.989 ms; the unchanged external completion metric is 11.837/40.759 ms.
In 86 trials the latest callback timestamp is later than the already-recorded
external observation. Trial 54 records callback 47.512 ms, external observation
46.260 ms and 37.182 ms inside DWM-frame reads. The 1.252-ms late callback is
direct evidence that this timestamp is an **upper bound**, not the instant the
rectangle first changed. The helper handles all eight windows on one GUI
thread, and callback timestamps follow their window order in every trial.

Microsoft documents that `WM_WINDOWPOSCHANGED` arrives after a position change,
and its default processing sends `WM_SIZE` and `WM_MOVE`. Our timestamp also
includes that default processing and any intervening scheduling. It is neither
a compositor visibility timestamp nor proof that observation alone explains the
remaining latency.
[Message contract](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-windowposchanged).

The smallest further discriminator, if needed, is an entry timestamp with the
same actual rectangle plus the existing post-default timestamp in this owned
helper. It would establish whether elapsed time lies inside default callback
processing. A late entry would still leave request delivery, target scheduling
and pre-callback window-manager work unresolved; attributing those requires a
correlated scheduler/wait trace rather than more priority or repaint-flag
experiments. [Windows CPU trace analysis](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/cpu-analysis).
No such follow-up or production optimization is claimed here. Existing latency
budgets remain unmet, and diagnostic overhead is included rather than deducted.

Build/test identities and successful portable correlation checks are retained
in `target/placement-observer-build.json`; the actual target run is
`target/surface-pools-target-placement.json`, with cleanup and desktop
restoration in `target/target-placement-{cleanup,restored}.json`.

### Final factory candidate, 13 November 2026

Ordinary daemon
`9608872A54F93B28221A918D48E2CB1B487884D10A2BCC1A19ECF4945D4FCE94`
passes all 128 exact placements with no daemon error in
`target/factory-final-performance.json`. Readiness is **120.659 ms**, external
median/p99 **12.348/44.500 ms**, own-hook p99 **104 µs**, and whole-callback p99
**938 µs**. The ten-second idle sample is **0.4687147%** of one core. Input
dispatch profiling and target-window callback profiling are both disabled;
the probe's existing aggregate readback timings remain included.

Working set is **45,961,216 / 49,188,864 / 47,472,640 bytes** before input, after
input and after idle. All three samples are below 50,000,000 bytes in this run.
They do not establish a continuous peak or universal memory bound: the earlier
E9 runs retained different executable-image residency and one exceeded the
limit. Readiness passes; placement latency, hook latency and literal-zero idle
CPU remain unmet. The exact final binary also passes 14 lifecycle and ten
carousel cases. Its per-target visual inspection passes all 13 reviewed PNGs:
eight complete rings, five bar hashes matching D583 and zero residual pixels in
the removed-split region. All 13 ordering brackets held on attempt one. This
covers explicit inspection ordering at 4K/125%, not normal Z-order or other DPI.
Reports are in `target/factory-final-target-visual/`; the earlier E9 and 960
lowest-helper partial reviews remain preserved. The recapture is consistent
with earlier fixture occlusion but does not identify an intervening window.
Build identities are in
`target/factory-{final,install,validation}-build.json`; the staged UIAccess image
`9ECFCE8BF68424B9DB46D344D4938445076FB1F886C9DD799D644C8815DC7040`
has not completed signed installation or runtime acceptance.

After these checks, the same ordinary 960 daemon was promoted through Explorer
and verified running as PID 10844 with EC22 Settings adjacent. Four original
window/process/birth identities, workspace assignments, exact visible frames,
prior focus and the personal configuration are preserved; its token is medium
integrity without UIAccess and no previous manager is running. This separate
live result is in `target/factory-final-live.json`; it is not another performance
measurement or completion of the pending signed-install and soak checks.

### Paired callback entry and post-default sampling, 13 November 2026

The follow-up discriminator uses the unchanged ordinary 960 daemon above,
with helper
`BE6550EB8FEAC0BDA113AEB0C9BA6EB39FCC5A59891493633090AA47077C6CBB`
and probe
`596DCBF54EF6D7A09A0916B2E2C3C13BEBAC9695348C6BB28B85155ADAA51D36`.
Settings remains EC22. `target/entry-observer-build.json` records these hashes,
source hashes, four passing portable tests, Windows-target Clippy with warnings
denied and the helper/probe release build. Neither daemon nor Settings was
rebuilt for this experiment. Native evidence is `target/entry-placement.json`.

The opt-in helper publishes eleven numeric `Moon.TestPlacement.v2.*` properties
under one odd/even sequence: paired timestamps and actual outer rectangles,
plus the sequence. It reads entry QPC **before** the entry `GetWindowRect`, then
retains the previous `DefWindowProc`, post-default rectangle read and QPC order.
Consequently the bracket includes both rectangle reads, default processing and
any intervening scheduling; geometry is verified at the reads, not at the QPC
instants. Default processing still sends `WM_SIZE` and `WM_MOVE` as documented.
[Message contract](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-windowposchanged),
[rectangle contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrect).

All 128 trials have eight changed targets and 1,024 fresh, complete paired
samples. Each target advances its even sequence by exactly two; all 1,016
between-trial sequence comparisons are continuous. Both rectangles must match
the expected target. Trial maxima agree with the individual records, and all
entry and post-default timestamps follow target order on the helper's single
GUI thread. The unchanged completion metrics agree with the corresponding
observer records. Deltas below are calculated within each trial or adjacent
pair before calculating quantiles, never by subtracting percentiles.

| Recorded interval | Samples | Median | p99 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Trigger QPC → first target entry | 128 | 2.571 ms | 5.102 ms | 8.099 ms |
| Trigger QPC → eighth target entry | 128 | 12.270 ms | 27.031 ms | 60.215 ms |
| First → eighth target entry, paired | 128 | 9.630 ms | 18.932 ms | 57.489 ms |
| Previous target post-default → next target entry | 896 | 1.326 ms | 4.193 ms | 21.949 ms |
| Entry → post-default, same callback | 1,024 | 1 µs | 2 µs | 2 µs |
| Existing external exact-placement completion | 128 | 12.248 ms | 26.752 ms | 59.979 ms |

QPC frequency is 10 MHz. Reported microseconds are truncated independently:
320 callback brackets report 0 µs, 690 report 1 µs and 14 report 2 µs. The
reported maximum therefore bounds the underlying bracket below 3 µs; zero does
not mean zero work. Subtracting the two rounded elapsed fields differs from
the directly converted bracket by at most 1 µs in every record. Same-thread
brackets avoid cross-thread ordering ambiguity; trigger-to-target comparisons
retain the documented one-tick uncertainty, 0.1 µs here.
[QPC precision and ordering](https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps).

In **82 of 128 matched trials**, the recorded latest entry exceeds the recorded
external completion by 89–1,349 µs. Each is the eighth target; the same 82 trials
also have a later post-default timestamp. The external `Instant` starts just
before the separate trigger-QPC read, so their origins are not identical and
the unrecorded gap cannot be removed. Moving the sample before default
processing nevertheless leaves the late-entry observation intact: it is not a
timestamp for when geometry first changed or when pixels became visible.

Zero-based trial 66 illustrates the remaining interval: first entry is
2.726 ms, eighth entry 60.215 ms, and external completion 59.979 ms. Target 2
post-default to target 3 entry spans 21.949 ms, while every callback bracket in
that trial reports at most 2 µs. The observer records 44 DWM-frame calls taking
51.1966 ms cumulatively, with a 20.2151-ms maximum; its 50 native rectangle
calls total 6.6 µs. These are overlapping observations in the same trial, not
proof of which thread or API caused the gap, and must not be added together.

This rules out millisecond-scale entry-to-post-default work in this owned
fixture. It does not distinguish pre-callback native work, queued delivery,
scheduling, other messages or the property publication performed **after** the
post-default timestamp. That publication remains diagnostic overhead and may
affect later targets. A correlated scheduler/wait trace is still needed for
causal attribution; these samples do not establish real-application processing
or compositor presentation latency.
[Windows CPU trace analysis](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/cpu-analysis).
No latency-budget pass or production optimization is claimed.

`target/entry-placement-cleanup.json` verifies zero owned daemon/helper
processes, no recovery journal and unchanged personal configuration after the
probe. `target/entry-placement-restored.json` verifies the same ordinary 960
daemon restored as PID 11416 with all four original window/process/birth
identities and workspace memberships preserved. This diagnostic did not
replace the installed or live product binaries.

### Bounded text-layout reuse and matched idle controls, 13 November 2026

This comparison keeps the ordinary 960 baseline
`9608872A54F93B28221A918D48E2CB1B487884D10A2BCC1A19ECF4945D4FCE94`
and tests ordinary candidate
`D973B039212A3A6DBE7727CF1FDE8C7E59BAE1BB716B27741BD1324450F75BC6`.
The candidate retains one analyzed text layout per current visible bar item,
keyed by text, font family, font size and clamped layout dimensions. Position
and color remain draw-time properties. A changed key replaces that item's
layout; shortened or hidden bars release removed entries, and canvas/device
recreation releases the associated layouts. There is no map of previous titles
or provider values. Retention is bounded by current items and strings, not a
fixed byte limit. Microsoft documents repeated layout drawing as a way to avoid
repeating text formatting and layout work; origin and the default fill brush
are supplied to the draw call.
[DrawTextLayout contract](https://learn.microsoft.com/en-us/windows/win32/api/d2d1/nf-d2d1-id2d1rendertarget-drawtextlayout).

`target/layout-cache-build.json` records the immutable build, source hashes,
Windows all-target Clippy with warnings denied, formatting and one passing
windowless native resource test. That test exercises repeated reuse, text/font/
size/bounds invalidation, UTF-16 symbol ranges and font collections, unchanged
trimming/wrapping/alignment, and shortened/empty/hidden-bar release. The ordinary
manifest remains `asInvoker` with UIAccess false. EC22 Settings was copied
unchanged; no signed candidate or package was rebuilt for this experiment.
The helper and probe are the same BE655 and 596D binaries identified above,
verified in `target/layout-cache-validation-inputs.json`.

The exact D973 candidate passes all 14 lifecycle cases and ten workspace
carousel cases in `target/layout-cache-{integration,carousel}.json`. All 13
original captures in `target/layout-cache-visual/` were visually reviewed and
hash-checked against their capture manifest. Ten whole PNGs, including every
bar and both full-width survivors, exactly match the previously accepted 960
captures. The other three window images differ in 19 pixels total, with no
channel difference greater than one. All rings, rounded corners, focus colors,
font changes, bundled glyphs and the complete Arial 18 workspace label `10`
remain correct. Both retired split-border samples contain zero nonwhite pixels
out of 57,800. All 13 before/after ordering brackets hold on attempt one.

These are per-target inspection-order captures at 4K/125%; they do not accept
normal product Z-order, other physical DPI scales or animation smoothness.
The five snapshots retain 12 current layouts, with cumulative creations
12 → 12 → 24 → 24 → 36. Resize/focus and border-slot removal reuse unchanged
layouts; font/style reload replaces the current entries. This fixture removes
a border slot, not a bar item; bar-item release is covered by the native unit.
One device and two factories persist, with no recreation, BeginDraw, descriptor
or daemon error. Detailed scope and pixel comparisons are retained in
`target/layout-cache-visual/{visual-review,pixel-review}.json`.

The default eight-window, 128-chord workload completes every exact-placement
check in `target/layout-cache-performance.json`. Readiness is **130.386 ms**,
external median/p99 **11.870/39.996 ms**, own-hook p99 **96 µs** and whole-callback
p99 **873 µs**. Input profiling and target callback profiling are disabled;
existing aggregate observation overhead remains included. Working set is
**46,407,680 / 49,393,664 / 47,632,384 bytes** before input, after input and after
ten seconds idle. All three samples pass 50,000,000 bytes in this run, without
establishing a continuous peak or universal bound. Nominal surface pixel bytes
remain 2,865,156, not a measure of all backing allocation or resident memory.
The ten-second idle reading is **0.1562473%** of one core. Placement/hook latency
and literal-zero idle CPU remain unmet; these samples do not establish that
layout reuse improved whole-process CPU or memory.

The longer controls use `scripts/validate-idle.ps1`, eight owned helpers, a
five-second warm-up and a requested 60-second quiet interval without IPC
requests. Default mode keeps changing widgets; Static substitutes representative
literal text while preserving widget order, icons, styles and width caps.
Optional provider presence and exact text widths can differ, so Static is a
no-scene-change discriminator rather than an identical pixel workload. Both
modes retain the default provider worker and ordinary keyboard hooks/surfaces.
All measured environment overrides are unset.

The harness retains the daemon process handle and checks its birth time at both
endpoints. State/counter queries occur outside the raw CPU interval. QPC endpoint
midpoints measure wall time; reported endpoint-read uncertainty is 27.45–31.8 µs.
CPU below sums process-wide kernel and user counters, in documented 100-ns
units, divided by wall time for one-core percentage. Those units do not imply
100-ns measurement precision: all observed time deltas here are multiples of
15.625 ms. Cycles are reported independently and are not converted to CPU time.
[GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes),
[QueryProcessCycleTime](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryprocesscycletime).

| Run, in execution order | Wall interval | Kernel + user CPU | One-core CPU | Process cycles |
| --- | ---: | ---: | ---: | ---: |
| 960 Default | 60.007556 s | 93.750 + 46.875 ms | 0.234345% | 575,956,060 |
| D973 Default | 60.007370 s | 62.500 + 109.375 ms | 0.286423% | 548,186,450 |
| D973 Static | 60.007333 s | 78.125 + 0.000 ms | 0.130192% | 430,756,220 |
| 960 Static | 60.013207 s | 78.125 + 31.250 ms | 0.182252% | 425,245,810 |

| Run | Render calls | Changed scenes / bar draws / commits | New layouts | Working set, before → after |
| --- | ---: | ---: | ---: | ---: |
| 960 Default | 88 | 48 / 48 / 48 | 1,152 | 46,092,288 → 46,010,368 bytes |
| D973 Default | 86 | 48 / 48 / 48 | 50 | 45,977,600 → 45,936,640 bytes |
| D973 Static | 92 | 0 / 0 / 0 | 0 | 46,186,496 → 46,206,976 bytes |
| 960 Static | 90 | 0 / 0 / 0 | 0 | 46,088,192 → 46,006,272 bytes |

Every run preserves world revision, exact native frames and planned placements,
and performs zero border redraws. Both Static runs satisfy the explicit
zero-scene qualification: no revision, changed scene, bar/border draw, layout
creation or commit. They still consume CPU and cycles, while render entry is
called 90–92 times. Thus unchanged rendering is skipped, but the full daemon
is not literally idle. These aggregate counters do not attribute that residual
work to an individual provider, maintenance check or native subsystem.

For the same 48 default redraws, new layouts fall from 1,152 to 50; D973 retains
24 layouts at both endpoints. In Static it retains 30 current layouts at both
endpoints. That is direct evidence of avoided reshaping and bounded current-item
retention. The higher default CPU-time result despite fewer cycles, and the
opposite ordering of static CPU time and cycles, do not justify a general CPU
improvement claim from one sample per condition. These four runs are sequential,
not randomized repetitions. All sampled working sets are below 50,000,000 bytes,
without resolving earlier workload/residency variation or proving a peak bound.
No result here applies to the separate provider-demand follow-up candidate.

Reports are
`target/layout-idle-v2-{1-baseline-default,2-candidate-default,3-candidate-static,4-baseline-static}/report.json`.
Each reports success, restored owned originals and no cleanup error.
`target/layout-idle-v2-cleanup.json` verifies no owned daemon/helper, no journal,
no previous manager and unchanged personal configuration.
`target/layout-idle-v2-restored.json` verifies the ordinary 960 daemon restored
with the original four process/birth/window identities and workspace membership.

The earlier failed fixture is preserved in
`target/layout-idle-1-baseline-default/report.json`: its `--only` launch disabled
surfaces and keyboard hooks, so the active-renderer precondition timed out
before any CPU measurement. This was a harness invocation error, not a product
regression or an idle result. The corrected v2 uses its owned ignore rule for
isolation without `--only`. The failed attempt's cleanup and normal desktop
restoration are retained in `target/layout-idle-{cleanup,restored}.json`; they
show no remaining owned process/journal and unchanged personal configuration.

### Provider demand selection: native validation and matched Static pair

Ordinary candidate
`43FD913681AD449967DEEBD2DD6BA38DAD59E37D384FD1142A30E9FC9F17E314`
adds configuration-derived provider selection to D973's retained-layout renderer.
The provider source is
`F7E0CF3AA788B7A9886134AF806A24C89AD6447F4F0DBDCA83DF14967CF2A72E`;
the renderer remains
`FC8D56EA18E6F61D6413235196A037990724D8CE411F14F678F1E1B8F5434BB6`.
`target/provider-demand-build.json` records the source and six staged file
hashes; `target/provider-demand-windows-inputs.json` independently verifies the
Windows copies. The ordinary Moon manifest remains `asInvoker`, `uiAccess=false`.

The same eight-window Static harness ran D973 first, then 43FD. Both runs
qualified: world revision, changed scenes, bar/border draws, new layouts and
commits all remained unchanged, as did the world, planned placements and native
frames. Neither interval contains IPC polling, and both reports have empty
cleanup-error lists and verified original-window restoration.

| Static run | Wall interval | Kernel + user CPU | One-core CPU | Process cycles | Render calls |
| --- | ---: | ---: | ---: | ---: | ---: |
| D973 baseline | 60.005637 s | 62.500 + 78.125 ms | 0.234353% | 429,596,270 | 94 |
| 43FD provider selection | 60.006920 s | 31.250 + 15.625 ms | 0.078116% | 99,043,820 | 80 |

This pair records less process CPU time and fewer cycles while the display
stayed unchanged. It is one sequential pair, with quantized CPU-time counters,
not a universal improvement or a literal-zero-CPU pass. The default 13-widget
configuration still requests every displayed local provider family; these
Static results must not be presented as a default-bar CPU improvement.
The regular maintenance and provider-worker cadence remain unchanged.
Reports: `target/provider-idle-{1-baseline-static,2-candidate-static}/report.json`.
`target/provider-idle-{cleanup,restored}.json` records no remaining owned process
or journal, unchanged personal configuration, no previous manager and restoration
of the ordinary desktop.

The same candidate passes 36 native unit tests with four interactive cases
explicitly ignored, the real provider probe, all 14 lifecycle cases and all ten
workspace-animation cases. The scoped visual review verifies all 13 images,
byte-identical to the retained comparison captures; it retains the explicit
inspection-order and single-DPI limits. Evidence is
`target/provider-demand-native-units.log`,
`target/provider-demand-{providers,integration,carousel}.json` and
`target/provider-demand-visual/{report,visual-review,pixel-review}.json`.

In `target/provider-demand-performance.json`, all 128 placements match.
Readiness is 118.223 ms; external median/p99 are **11.944/29.496 ms**. The median
budget still fails; the p99 passes in this run only. Own/whole hook p99 remain
**101/886 microseconds**, and the ten-second default idle CPU sample is
**0.156248%** of one core. Both optional profiling modes are disabled. Working
sets are 45,543,424 / 48,816,128 / 46,981,120 bytes before input, after input and
after idle. These three samples are below 50,000,000 bytes, without proving a
peak bound or resolving earlier variability. No daemon error was reported.
