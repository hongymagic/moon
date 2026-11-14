# Native carousel presentation validation

Measured on the existing Windows 11 desktop on 12 November 2026. These checks
created only owned fixture windows, preserved the previous cursor and foreground,
restored original fixture placement and closed every fixture process. The
standalone fixtures ran with GlazeWM's previously verified explicit `testwin`
ignore rule; full-daemon integration ran with the previous manager stopped.
No UAC prompt or installation was requested.

## Interruption refinement, 15 November 2026

The isolated polish implementation retains visible workspace offsets when a new
switch arrives during the 200 ms transition. Its portable tests cover a third
destination, reversal without a position jump, 9,998 successive interruptions
with at most three retained pages, and immediate committed focus, visibility,
tree order and exact geometry for both tiles on each selected workspace. These
three tests pass; the complete core suite also passed 83 tests before the final
geometry assertions were strengthened and rerun.

The native implementation reuses the popup and mouse hook during compatible
retargets, releases offscreen thumbnail relationships, and limits each monitor to
512 relationships. It rechecks reduced motion only during active presentation.
A hidden-owned-popup regression covers cancellation, monitor removal, preference
change, expiry, stale sources, a subsequent presentation and final destruction.
The updated interactive probe now expects a rapid reversal to retarget and settle.
Those Windows regressions have **not been executed** for this refinement; the
historical nine-case result below describes the prior cancellation behavior.

## DWM source lifecycle

`crates/moon-win/examples/thumbnail_probe.rs` used a separate ordinary Win32/GDI
source process and three owned destination styles. Each phase changed the source
paint color and checked the actual composed destination pixel. All 18 samples
matched exactly in `target/thumbnail-probe-shell/report.json`.

| Destination | Visible repaint | Verified shell cloak (`2`) | App cloak (`1`) | Both restore paths |
|---|---|---|---|---|
| Ordinary redirected popup | Pass | Pass | Pass | Pass |
| Layered popup, constant alpha 255 | Pass | Pass | Pass | Pass |
| No-redirection popup | Pass | Pass | Pass | Pass |

The ordinary and layered popup backgrounds rendered the requested RGB(32,32,32).
The no-redirection popup's corner instead showed the desktop RGB(3,4,8): the
thumbnail worked, but its GDI background was absent. Production therefore uses an
ordinary opaque popup. No foreign window style, parent or region is changed.

The earlier `target/thumbnail-probe/report.json` used a no-activate source without
a shell view and reached the parking fallback. Only the later shell report above
establishes live thumbnails from a verified shell cloak.

Microsoft documents that the thumbnail destination must belong to the caller and
that relationships must be unregistered by that same process. The owned popup
and RAII cleanup follow that contract. Cloaked repaint compatibility above is an
observed result, not an additional API guarantee. See
[DwmRegisterThumbnail](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmregisterthumbnail).

## Presentation and input

`crates/moon-win/examples/carousel_probe.rs` exercised the production `Carousel`
with four windows from a separate `testwin.exe` process, grouped into two tiled
workspaces. All nine cases passed in
`target/workspace-carousel-probe/report.json`. Its metadata records the probe
SHA256 `68FFE5865705DC4272D67DBBCCE1DAB9420CED31039946946928C527AF571BF4`,
helper hash and successful cleanup. The earlier seven-case
`target/carousel-probe/report.json` is retained as input/lifecycle evidence from
the superseded single-window presentation, not the selected workspace behavior.

| Case | Observed result |
|---|---|
| Grouped clipped transition | Two side-by-side outgoing tiles and two stacked incoming tiles move within an 840×620 viewport, retaining their gaps. Incoming hidden windows start at 390×280 and resize asynchronously to 800×280; both final native frames are exact. Three composed frames were captured. |
| Rapid repeat | A repeated prepare removes the active visual and settles immediately without a stale overlay. |
| Mouse button already held | Preparation is suppressed, and the original application receives the matching release. |
| First click during presentation | The overlay dismisses; zero downs and zero ups reach the later observer or underlying fixture. |
| Following click | The actual focused fixture receives exactly one down and one up. |
| Pointer movement | Visuals cancel. |
| Stale source owner | A mismatched expected process ID refuses presentation. |
| Empty incoming workspace | The outgoing group moves away, leaving the owned background; all four application windows remain cloaked. |
| Empty outgoing workspace | Both incoming tiles appear and settle at their exact native rectangles. |

The helper's real `WM_LBUTTONDOWN` and `WM_LBUTTONUP` counters establish delivery
in addition to the independent low-level observer. Only tagged button events
were injected, while the actual foreground was verified as an owned fixture.
No arbitrary input forwarding or application-content capture exists in the
production carousel; screenshots belong only to these validation examples.

The saved `frame-0.png`, `frame-1.png` and `frame-2.png` were visually inspected.
They show the outgoing tile and both stacked incoming native title bars and
content moving together across the opaque background, with the expected clipped
edge and gaps. Incoming text and caption controls retain normal dimensions after
the asynchronous source resize. Fixture and probe processes were absent after
cleanup.

## Full daemon integration

The earlier dynamic-runtime normal candidate
`EA798913C35FCB9CC962E08B9F4DB20719D660E502834DA1F66A97508781BBD0`
passed all ten checks in `target/carousel-integration-results.json` using
`scripts/carousel-integration.ps1`. An active animation was observed. Four owned
windows retained ordinary tiling; moving two to a second workspace produced two
native tiles on each workspace. Both workspace directions settled to exact
frames and cloak state. Focusing a hidden window activated its containing
workspace, while changing focus within a visible workspace kept the same canvas.

Rapid workspace changes, empty workspaces and disabling animation preserved the
native layout. Restart retained trees, membership, focus and geometry. Exit
restored every original frame and visibility. This establishes the daemon's
workspace semantics independently of the fixture's visual/input checks above.

The final static-runtime normal candidate
`075E96CD3C7C95E24D04F07EC9BCFF7D53F74F60E2F6AC53B23B179C7DFBB134`
subsequently passed the same ten checks with `-Measure`, bar and borders enabled.
Its result is `target/postpolish-performance/static-crt/workspaces.json`.

## Full 4K workspace measurement

The static candidate's four helpers occupied two tiled workspaces on a
3840×2160 monitor at 125%. The bar reserved 50 physical pixels, and each observed
overlay covered exactly `(0,50)–(3840,2160)`. Six alternating workspace commands
used the direct named pipe. After each command response the harness sampled only
the owned overlay HWNDs and daemon working set externally; it did not send
per-frame IPC queries that would wake and redraw the daemon.

| Measurement across six switches | Observed value |
|---|---:|
| Successful transitions and overlay observation | 6/6 |
| Direct command response | 19.764–32.434 ms |
| Command start to owned overlay disappearance | 251.642–269.553 ms |
| Median command start to overlay disappearance | 266.176 ms |
| Peak working set observed during sampled transitions | 47,575,040 bytes |
| Largest interval between observations | 35.439 ms |

The requested polling interval was 10 ms; actual scheduling intervals were
recorded rather than assumed. These clocks include the command transport,
preparation and final overlay cleanup. Overlay HWND disappearance does not prove
the precise DWM presentation frame, and command response is not key-to-placement
latency. The six samples establish bounded completion in this workload, not a
p99 distribution. No process memory trimming was used. The ten subsequent
correctness checks and original-frame/visibility restoration passed; independent
cleanup found no Moon/testwin process or recovery journal.

## Timing and scope

The configured duration is 200 ms. The probe completed the sampled transition in
240.879 ms, including three synchronous composed-screen captures. Its working set
changed from 16,801,792 to 23,568,384 bytes; these samples include screenshot
allocations and cover an 840×620 viewport. They do **not** establish a 4K daemon
memory budget or a frame-time distribution.

Preparation includes one `DwmFlush` before final native apply, so animated mode
must be reported separately from instant placement. The daemon leaves the normal
application HWND at its committed location throughout the visual animation. The
temporary hook survives a visual cancellation only when needed to consume its
own matching release, and uses the ordinary event-driven pump during that wait.
Session suspension clears that old-desktop sequence and removes owned overlays
and hooks. Reduced motion or high contrast suppresses presentation.

These results cover the owned GDI fixture and mouse scenarios above. WinUI 3
presentation, protected content, touch/pen, physical multiple displays and session
transitions require their own recorded evidence;
the pure layout and clipping checks do not establish those native results. The
implementation never retries protected content through another capture API.

See [layout semantics](carousel-layout.md) and
[Windows API decisions](windows-canvas-research.md).
