# Windows platform verification

Recorded 2026-11-11 on the user's Windows 11 desktop from WSL. These are observed results, not completion claims for the full milestone or seven-day soak.

## Reproducible probe

`cargo xwin build -p moon-win --example platform_probe --target x86_64-pc-windows-msvc` builds the capability probe. Run `platform_probe.exe C:\path\testwin.exe`. It creates eight owned test windows, restores their geometry, closes the child process, and returns focus to the previous foreground window. It also briefly owns moon's session pipe, so run it with the daemon stopped.

The GlazeWM configuration gained one development-only ignore match for process `testwin`. The original file is backed up as `C:\Users\dhong\.glzr\glazewm\config.yaml.pre-moon-testwin-20261111`. Existing CRLF formatting and all other bytes were preserved. GlazeWM's `wm-reload-config` returned `success: true`.

## Observed results

| Check | Observation |
|---|---|
| Enumeration | One monitor and all eight owned test windows found |
| Placement | All eight visible DWM frames exactly matched target rectangles |
| Placement latency | Second cold probe: 18,426 microseconds for eight windows, including readback; initial run: 73,649 microseconds |
| Shell cloak | `IApplicationView::SetCloak` succeeded; `DWMWA_CLOAKED` became 2 |
| Uncloak | DWM cloak state returned to 0; the original rectangle was restored |
| Focus | 100/100 foreground changes across eight test windows after bounded verification |
| Hotkeys | Alt+H, Alt+Shift+1, Alt+Tab each matched 10/10 trials; target foreground stayed the test window |
| Hook timing | 200 callbacks in the second probe; whole-chain maximum 3,257 microseconds, p99 above the original 64-microsecond histogram ceiling while GlazeWM's hook also ran |
| IPC round trip | Medium-integrity same-logon client successfully sent and received NDJSON |
| Single pipe owner | A second server was refused |
| Low-integrity client | A child lowered its own integrity token, then received `E_ACCESSDENIED` when opening the pipe |
| Cross compilation | Platform, hooks, IPC and testwin built for Windows MSVC from WSL |
| Static checks | `cargo xwin clippy -p moon-win -p testwin --all-targets --target x86_64-pc-windows-msvc -- -D warnings` passed |

The initial focus probe checked `GetForegroundWindow` immediately after `SetForegroundWindow` and observed only 14/100 completed transitions despite successful API returns. The platform now checks the actual foreground HWND, waiting at most 10 milliseconds per accepted attempt before trying the checked fallback. The repeated probe passed 100/100.

Hook timing distinguishes moon's own work, including its ordered Alt mask, from the whole callback, which includes `CallNextHookEx` and any other installed hooks. Both histograms have 4,097 one-microsecond buckets; the last bucket is a lower bound. Masking time remains included rather than being hidden from the local-work metric.

The initial focus probe used multiple windows of one application. The subsequent [three-application scenario](validation-applications.md) passed S4 with 100/100 real hook-driven transitions across testwin, Windows About, and Paint, including confirmed WinUI 3 handling. Lock/unlock, elevated-window focus, and the under-50-microsecond whole-callback budget while moon is the sole window manager require their own live scenarios. The two cold placement samples do not establish median or p99 placement budgets.

## Boundaries and implementation decisions

- `windows` 0.62.2 is pinned exactly after checking the crates.io registry. Generated bindings and supporting crate source were inspected during implementation. The current supporting `windows-result` exposes `Error::from_thread`, not the older `from_win32` name.
- Window movement uses `SetWindowPos` with `SWP_ASYNCWINDOWPOS`; the daemon verifies eventual visible-frame rectangles. This avoids synchronously waiting on a target application's message processing. The [400-batch comparison](validation-placement.md) supports retaining this path: deferred placement had a worse p99 and blocked the caller longer.
- The undocumented shell interface is isolated in one function, queried by its interface identifier, and each cloak change is checked using the documented DWM attribute. Failure to cloak falls back to parking. Failure to uncloak is returned as an error so recovery records remain available.
- Recovery captures the original `WINDOWPLACEMENT`, preserving maximized state as well as normal geometry. `DWMWA_BORDER_COLOR` is documented for setting, and its getter failed on this host. Recovery uses a readable original when available; otherwise it resets `DWMWA_COLOR_DEFAULT` so moon's tint cannot remain after watchdog recovery. An application's unreadable custom original tint cannot be reconstructed, so exact custom-color preservation is not claimed. [Microsoft documents the default-reset value](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute).
- The hook thread compares and queues commands. An accepted Alt chord first sends one tagged, unassigned-key release so fast modifier releases cannot switch input language. Only that exact internally injected event bypasses downstream remappers; ordinary input follows the hook chain. Lower-integrity injected input cannot trigger commands. No key data is retained in logs, traces or IPC.
- IPC uses a logon SID ACL, a medium-integrity mandatory label prohibiting both low-integrity reads and writes, `PIPE_REJECT_REMOTE_CLIENTS`, overlapped I/O cancellation, a 64 KiB request limit, an 8 MiB response limit and at most 15 active clients plus the listening instance. Client response queues are bounded at 16 frames.
- `platform::token_capabilities(pid)` reports observed integrity, UIAccess, elevation, AppContainer status and Windows session ID for doctor and privilege-inheritance checks.
- Admission compares the caller's and target's actual integrity labels. Without UIAccess, a higher-integrity target is excluded; a token-query failure also excludes it. The two native admission tests passed on 12 November, including a read-only check that the ordinary caller (RID 8192) rejected the existing elevated PowerToys runner (RID 12288). Synthetic cases cover equal/lower integrity, medium-plus, high integrity and UIAccess. The evidence is in `target/process-management-tests.txt`; Windows library/test clippy also passed.

Admission now also requires the same Windows session and excludes system-integrity
targets. Synthetic rejection cases were added during the subsequent source review;
that extension passed in the final 16-test native library run. Signed high-integrity operation
remains an S8 measurement: Microsoft's [UIAccess security policy](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/user-account-control-only-elevate-uiaccess-applications-that-are-installed-in-secure-locations)
describes foreground control, input injection and hook access across integrity
levels, while its [UI Automation overview](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview)
limits medium-plus/high behavior according to the launching user's administrative
membership. The [SendInput reference](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
states the ordinary UIPI restriction, and SetWindowPos does not document a specific
UIAccess exception. The existing trusted Glaze successfully closed the exact owned
high-integrity Settings window, but that proves neither moon's installed token nor
its elevated placement and focus. No privilege capability is inferred from the
manifest alone.

## Actual Explorer restart

The corrected final candidate passed an actual Explorer restart on 12 November
2026, with SHA-256
`1A7F502085F9CD067C18975E76E454E75F60F28FC75C809E1541DA86F33FC5CF`.
The desktop/taskbar process changed from PID 20716 to PID 34496 while the original
daemon PID 47060 continued running. Four owned helpers retained their complete
workspace/tree/focus/floating-state fingerprint, exact visible frames and hidden
membership. The AppBar work area returned to y=50, height=2110, and workspace
switching still changed the actual foreground window correctly.

`Shell_NotifyIconGetRect` again resolved the daemon's exact notification identity.
Its returned rectangle had zero height on this host, so this establishes tray
registration recovery rather than visible-pixel positioning. Cleanup restored all
four original helper frames and the original y=0, height=2160 work area, then
closed only the owned helper and daemon processes. All five preexisting visible
application HWND/PID/creation-time identities survived. The complete passing
report is `target/explorer-restart-results.json`.

`scripts/explorer-restart.ps1` restarts only the process currently owning both
`GetShellWindow` and `Shell_TrayWnd`, after checking its exact image path and
creation time through the same handle used for termination. Before each restart,
ShellWindows COM must contain no user folder/control-panel window in that process,
and native enumeration must find no titled application window there. The existing
Power Options window is in a separate Explorer process and remains untouched.

Two earlier actual restarts exposed a fullscreen-classification defect. The
bounded diagnostic rerun captured all failed predicates before cleanup:

| Predicate | Observed result |
|---|---|
| Desktop/taskbar process | Recreated from PID 17628 to PID 20716, with a different recorded creation time |
| Daemon and managed windows | The same daemon PID 47604 survived with four windows and no reported error |
| Visible placement and hidden workspace | All three visible frames matched targets; the fourth helper remained shell-cloaked |
| Tray | `Shell_NotifyIconGetRect` succeeded after the shell restarted |
| AppBar reservation | Failed: native work area changed from y=50, height=2110 to y=0, height=2160 and remained there for 30 seconds |
| Layout semantics | Trees, active workspace and focus matched; dormant floating rectangles changed during the transient work-area updates |
| Cleanup | All four original helper frames and the original work area were restored; helpers and daemon exited |
| Existing applications | All five initially uncloaked user HWND/PID/creation-time identities survived, including Power Options |

The diagnostic world identified the cause: `native_fullscreen` held desktop HWND
197440, which exactly matched the newly created shell desktop. The fullscreen
detector had mistaken its borderless monitor-sized surface for an application,
so normal fullscreen handling deliberately removed the bar reservation. This was
not a failure to re-register the AppBar. `native_fullscreen` now excludes the
documented [GetShellWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getshellwindow)
and [GetDesktopWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdesktopwindow)
handles before testing application styles or geometry. The passing rerun above
confirms the correction also preserves the complete layout fingerprint.

The first restart also removed two previously shell-cloaked ApplicationFrameHost
background frames. Their lifecycle is recorded separately from the five visible
application identities; the harness does not claim that every raw shell HWND
survives a shell restart. The original failures remain at
`target/explorer-restart-first.json` and `target/explorer-restart-diagnostic.json`.

The checks use the documented [TaskbarCreated notification](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar),
[Shell.Windows collection](https://learn.microsoft.com/en-us/windows/win32/shell/shell-windows)
and [Shell_NotifyIconGetRect](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyicongetrect).

`SWP_DEFERERASE` and `SWP_NOCOPYBITS` were tested through temporary opt-in flags in two full-pipeline comparisons. Neither met the placement budgets; no-copy also exceeded the working-set limit. Both switches were removed, leaving the normal flag set unchanged. [The exact results and evidence paths](validation-placement.md#repaint-flag-experiments) record the measurements and remaining repaint limitations. [Microsoft's SetWindowPos reference](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos) distinguishes those flags from `SWP_NOREDRAW`, which would make the caller responsible for invalidating affected areas.

## Primary references

- [Microsoft: CreateNamedPipeW](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew)
- [Microsoft: named-pipe security and access rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)
- [Microsoft: low-level keyboard callback](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc)
- [Microsoft: SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow)
- [Microsoft: DWM window attributes](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)
- [windows crate registry](https://crates.io/crates/windows)

## Transport regression tests

A separate .NET client exposed a real truncation bug: disconnecting the server immediately after `WriteFile` discarded bytes the peer had not consumed. The implementation now waits for client EOF with a cancellable five-second deadline; it does not call unbounded `FlushFileBuffers`.

Five Windows-native library tests passed in 0.05 seconds after this fix:

- Every compiled-in hotkey maps to a native virtual key, including OEM punctuation aliases.
- Malformed hotkeys fail before hook installation.
- A 128 KiB JSON response survives a deliberately delayed reader and subsequent disconnect.
- A client that never reads a 2 MiB response cannot prevent cancellation and server shutdown.
- 200 subscription frames remain intact when several frames arrive in a single pipe read.

The native test executable was `target/x86_64-pc-windows-msvc/debug/deps/moon_win-34d76143ebca4ae3.exe`; test pipes use private per-process names, independent of the running daemon.

## Event intake

The input thread now owns an invisible top-level notification window. It receives display/settings broadcasts, WTS session lock/unlock, suspend/resume and Explorer's `TaskbarCreated` notification. Unlock and resume trigger immediate hook renewal; periodic renewal remains a fallback because Windows documents no query for a silently removed low-level hook.

Native desktop membership is queried through the documented `IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop`. Cursor following is opt-in: its mouse hook stores only the latest point and queues at most one pointer notification until the main thread consumes it. The default configuration installs no mouse hook. These additions compile and pass clippy; actual sleep, lock, display-hotplug and native-desktop acceptance scenarios are recorded separately when exercised.

Additional references: [Microsoft virtual-desktop membership](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ivirtualdesktopmanager-iswindowoncurrentvirtualdesktop), [WTS session notifications](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsregistersessionnotification), [power broadcasts](https://learn.microsoft.com/en-us/windows/win32/power/wm-powerbroadcast).

## Glaze enumeration and stranded-window recovery

On 12 November, `scripts/compare-glaze.ps1 -RestoreGlazeForCapture` temporarily
started GlazeWM through ordinary ShellExecute, captured its actual query response
alongside the native inventory, and issued its normal `wm-exit` command. No moon
daemon or test window was active. Full records remain locally in
`%LOCALAPPDATA%\moon-dev\glaze-enumeration.json`; window titles are kept out of
repository fixtures.

All three Glaze-managed HWNDs appeared in moon's native inventory. The seven
additional raw entries were individually accounted for:

| Additional entries | Observed reason |
|---|---|
| Two ordinary application windows | Already shell-cloaked before Glaze started, on the current native virtual desktop; Glaze did not adopt them. |
| Four `ApplicationFrameWindow` entries | Already shell-cloaked before the probe; two titled UWP frames and two empty shell frames. They remained untouched. |
| One tacky-borders dialog | A transient duplicate-instance dialog; moon's default ignore rule excludes its process. |

This is a complete comparison of the sampled native and Glaze sets, including
the intentional distinction between a raw candidate inventory and daemon
adoption. Startup adoption skips unowned cloaked windows so it cannot expose
background shell windows merely because they have `WS_VISIBLE`.

The probe also confirmed that this Glaze build's normal exit left its two hidden
ordinary windows cloaked. The `recover_windows` example then restored only four
explicitly selected ordinary HWNDs from the pre-probe inventory, checking PID,
process, class and current virtual desktop before any change. All four returned
`DWMWA_CLOAKED=0` and exactly regained their recorded visible rectangles. The four
UWP/shell frames were not exposed. The before/after records are
`window-inventory.json` and `window-inventory-after-recovery.json` in the same local
directory. GlazeWM, Zebar and tacky-borders had exited when the desktop was handed
back to the integration run.

## Virtual-desktop query cost

`window_on_current_virtual_desktop` now retains one `IVirtualDesktopManager` per
owned STA thread. `Apartment` cannot move across threads; nested guards share the
cache, and the final guard releases its interface before `CoUninitialize`.
Interfaces initialized outside this guard retain the uncached behavior because
their shutdown lifetime is not owned here. A disconnected shell connection is
discarded and retried once, while ordinary invalid-window errors leave a healthy
cache intact. Explorer's restart notification explicitly invalidates the cache.

The read-only `virtual_desktop_probe` compared 256 batches of eight actual window
queries per implementation on 12 November:

| Implementation | Median per eight queries | p99 per eight queries |
|---|---:|---:|
| Activate COM object for each query | 5.3861 ms | 6.3102 ms |
| Reuse apartment interface | 0.3149 ms | 0.4716 ms |

The isolated saving was 5.0712 ms at the median. Both phases finished with 154
handles and a 16,306,176-byte working set; this result does not establish an
equivalent reduction in whole-daemon memory. The probe modified no windows and
also exercised nested guards, explicit invalidation and COM reinitialization.
The native cache-lifetime regression test passed, as did Windows clippy for the
library, tests and probe. Raw results are in
`target/virtual-desktop-cache-results.json`.

This follows Microsoft's [STA ownership and reentrancy rules](https://learn.microsoft.com/en-us/windows/win32/com/single-threaded-apartments)
(updated July 2026), [balanced COM shutdown requirements](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-couninitialize)
and [documented virtual-desktop interface](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager).

## Session and secure-desktop guards

Startup can now read the current session before its first lock notification.
`session_state()` uses `WTSSessionInfoEx` and returns the session ID, actual lock
flag and whether the logged-on session is actively connected. Unknown flags or
query failures return errors. `input_desktop_available()` opens the input desktop
with read-only access, requires its name to match this thread's ordinary
`Default` desktop, and closes the opened handle. An inaccessible secure desktop
returns unavailable. `session_interactive()` combines both checks.

This distinction matters for RDP: Microsoft documents that `OpenInputDesktop`
can succeed in a disconnected session. The session query therefore remains
necessary. Console/remote disconnect notifications now suspend effects; connect,
logon and desktop-ready notifications request a state recheck rather than
pretending that connection implies unlock. Actual lock/unlock stay distinct.

The read-only `session_probe` ran on 12 November. It observed session 1 as
connected and unlocked, with the ordinary input desktop available. Sixty-four
repeated WTS and desktop checks retained exactly 98 process handles. The report
is `target/session-guard-results.json`. No lock, desktop switch, UAC prompt or
sleep was triggered; their physical acceptance scenarios remain separate.

References: [WTS extended lock flags](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/ns-wtsapi32-wtsinfoex_level1_w),
[WTS connection states](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/ne-wtsapi32-wts_connectstate_class),
[OpenInputDesktop](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openinputdesktop),
[desktop name queries](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getuserobjectinformationw)
and [session notifications](https://learn.microsoft.com/en-us/windows/win32/termserv/wm-wtssession-change).

## Journal replacement under reader locks

Two Windows-native `runtime::atomic_replace` regression tests passed on
12 November. Each created only its own temporary source/destination files and
opened the destination with read/write sharing but without `FILE_SHARE_DELETE`.

- A reader released after 30 ms allowed replacement after 31.936 ms. The
  destination contained the complete new bytes and the source no longer existed.
- A persistent reader produced a bounded access/sharing failure after 503.935 ms.
  Both the original destination and replacement source remained byte-for-byte
  intact. The test also bounds its wait independently and releases the reader
  before joining a worker that exceeds that bound.

Windows clippy passed for the library and tests. Raw output is
`target/atomic-replacement-results.txt`. The installer now reads its rollback
journal snapshot with `FileShare.ReadWrite | FileShare.Delete`, permitting the
daemon to publish the next journal while that snapshot is being copied.

The test uses the documented sharing behavior of Rust's
[Windows OpenOptionsExt](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.share_mode)
and Windows [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw).

## Modifier renewal and input-language regression

On 12 November, the native library suite passed all twelve ordinary tests. The
separate interactive regression then passed 22/22 trigger checks using one owned
helper: an Alt+Shift shortcut with modifiers already held at hook startup, ten
forced renewals while those modifiers were held, and an unmodified shortcut after
every release. The helper's keyboard layout stayed `0x04120412` throughout; two
input layouts were installed. The helper closed and the preceding foreground
window was restored. Raw outputs are in `target/hook-regression`.

The test exposed a layout-sensitive assumption before the interactive run:
Korean input maps `VK_RCONTROL` and `VK_RMENU` to no reverse scan-code translation.
Generic Ctrl/Alt events now select their side using `LLKHF_EXTENDED`; Shift uses
its scan code. Initial and renewed state retain only sided modifiers, so releasing
a sided key cannot leave an independent generic modifier latched. Regression
coverage includes generic and sided releases for all six Shift/Ctrl/Alt sides and
releasing one side while the other remains held.

This establishes forced renewal and unchanged input-language behavior. It does
not replace an actual lock/unlock scenario. The interactive test is explicitly
ignored in unattended runs and requires `MOON_TESTWIN_PATH` plus an isolated
desktop; run it by its full test name
`hooks::tests::held_alt_shift_renewal_preserves_hkl_and_plain_trigger`.

References: Microsoft's [extended-key and physical scan-code documentation](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input#extended-key-flag),
[MapVirtualKeyW translation contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-mapvirtualkeyw)
and [sided GetAsyncKeyState queries](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate).

## Rapid chords and final native checks

The initial test waited for command intake before releasing modifiers. A later
positive-control test proved that bare Alt+Shift changes the helper's input
language, then reproduced a race when a complete matched chord arrived in one
burst: the main-thread mask ran after Alt was already released. Forwarding only
the trigger's key-up and using `RegisterHotKey` both failed when modifiers were
released first, and exposed trigger key-ups to the helper.

The final hook sends one tagged release of Microsoft's unassigned key `0xE8`
before enqueueing an Alt command. No state borrow spans that reentrant input
call. Only the exact injected, tagged, zero-scan-code E8 key-up bypasses downstream
hooks; other input follows the chain. A refused mask executes no command, and a
previously consumed down remains consumed through repeat failures and release.
No physical keys are buffered or replayed, and language preferences are unchanged.

The native suite passed **16 ordinary tests**, followed by these four explicit
interactive checks:

- Initially held modifiers and ten renewals: **22/22** expected triggers.
- **128** complete chords with both release orders: **256/256** modified/plain
  triggers, unchanged HKL, no trigger events or added character messages in the
  helper. A calibration message proves the character counter actually works.
- All **52/52** default chords: independently mapped physical inputs produced
  the exact complete command payloads, with unchanged foreground, HKL and native
  menu state and no stuck modifiers. This checks routing without executing any
  daemon commands; semantic integration remains a separate result.
- Key ownership across initial installation and renewal: app-owned presses kept
  all four down/repeat events and two releases; consumed presses stayed hidden
  across renewal and modifier release. Same-chord repetition and the next fresh
  press produced exactly three commands. Each consumed phase preserved its
  immediate HKL. Forwarded input retains ordinary Windows language switching.

Each interactive check also verified ordinary F24 delivery to a downstream hook
while internal masks stayed out of that hook. The helper and observer closed and
the preceding foreground was restored. Test artifact hashes and raw outputs are
in `target/hook-regression/final/`; that test executable SHA-256 is
`B50A2E2A80923B6E73929C5FF46DBC7BFB8E7FDA449B751369B5AB2345C08869`.
The final focused key-pair check, including a consumed key held across renewal,
is in `target/hook-regression/key-pairs-final/`, from test executable
`AEDDD9FC97D7000B5464838BDBA0C22067CB03832D6530B96799B949DDFF46AD`.
The preceding failed test incorrectly required forwarded input not to switch
language; its raw result is retained separately. The correction compares each
consumed phase against its immediate layout without changing production code.

The pressed-key snapshot retains held nonmodifier keys during installation and
renewal. The initial down determines whether repeats and the matching up belong
to the application or moon; modifier changes cannot transfer that ownership.

Correctness passes, but the callback budget does not: the final burst run measured
**135 µs local-work p99** (including masking), **157 µs local maximum**, and
**741 µs whole-callback p99**. The earlier 9 µs matching-only observation predates
this correction and does not describe the final callback.

References: [Microsoft virtual-key assignments](https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes),
[SendInput ordering](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
and [the low-level hook contract](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc).


## Input logging evidence

A bounded source and saved-output review on 12 November 2026 found **no raw
keyboard-event logging path**. The production hook has no direct output calls.
Its in-memory pressed/swallowed state is neither serialized nor sent to the
main thread. Keyboard notifications carry configured commands and a monotonic
receipt time; hook statistics contain only counts and elapsed-time aggregates.
The daemon's stdout/stderr, background log redirection, error reporting,
transaction history, statistics, IPC/query replies and journal serialization
were traced. Configured binding names can appear in validation errors, the
requested cheat sheet and configuration snapshots; those are configuration,
not recorded key events. The helper counts character messages without storing
their contents.

The saved native, integration and performance evidence snapshot contained
**231 files**, **2,964,421 bytes**, including **150 parsed JSON documents**.
Checks found **zero** raw-key/scan-code labels, raw-input structure dumps,
character-content fields or pressed-state dumps, with no read failures. Observed
input-related output consists of aggregate timing and trigger counts, configured
commands/bindings, known test labels and input-layout identifiers. The file
hash manifest, source hashes and count-only findings are retained under
`target/input-logging-review/`; no user log contents were reproduced.

This establishes the no-key-data logging item for the inspected source and these
owned-input runs. It is not a continuous runtime assertion, an exhaustive
encoding detector or a seven-day soak result. Window/configuration metadata and
ordinary error text may contain other application or user-provided information;
this check concerns raw keyboard events and character capture.
