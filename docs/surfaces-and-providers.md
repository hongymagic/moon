# Native surfaces and providers

The main thread owns `Surfaces`. It renders immutable `moon-core::scene` values
using one shared Direct3D 11 / Direct2D / DirectComposition device and DirectWrite
text. Each monitor has a bar and a click-through border underlay. Bar clicks only
enqueue commands. Notification-area controls use that same queue. Unchanged
scenes do not redraw. GPU device loss recreates the graphics resources once and
reports any subsequent failure to the daemon.

DirectComposition surfaces retain the composited image without an animation
loop. Only drawing updates acquire a DXGI render target. Font bytes are embedded
and loaded through a private in-memory DirectWrite collection, so icon glyphs do
not depend on the user's installed fonts. The asset source, pinned release,
checksum and MIT license are in `crates/moon-win/assets/`.

Each monitor has one layered underlay HWND and one reusable border atlas.
Long edge strips are split into slices no longer than 1,024 pixels, with vertical
slices transposed into horizontal atlas rows. Longest-first shelf packing bounds
unused space. Child visuals reference clipped regions of that shared atlas;
their transforms put every slice back into physical monitor coordinates. Atlas
capacity grows only when needed. Global gradient coordinates stay consistent
across slices, including rounded corners. A provider-only bar update does not
redraw borders. All surfaces share one main-thread Direct2D device context.
Its reusable texture-cache limit is zero; this does not limit live
DirectComposition textures or total process working set.

The renderer reuses one linear-gradient brush per current border slot. A color
change replaces that slot's brush and gradient stops; geometry changes update
the existing brush's start and end points. Removing borders drops the excess
cached brushes. Each canvas also keeps one DirectWrite text format and ellipsis,
replacing them when its requested font family or size changes. Text layouts stay
fresh for each draw, with current bounds and the existing symbol-font overrides.
These caches contain no history or least-recently-used list. Canvas teardown and
shared-device recreation release them. This follows Microsoft's guidance to
[reuse drawing resources](https://learn.microsoft.com/en-us/windows/win32/direct2d/improving-direct2d-performance#reuse-resources);
it reduces repeated object creation, not necessarily working-set residency.

### Renderer diagnostics

The `query stats` response includes bounded, cumulative `renderer` diagnostics.
Render-call, changed-scene and successful-commit counts distinguish repeated
scene evaluation from drawing submissions. Other counters record successful
device, canvas, atlas, gradient-brush, text-format and text-layout creation, plus
device recreation attempts after render failures.

For bar, border and pointer-feedback draws separately, diagnostics call
`IDXGISurface::GetDesc` on the temporary surface returned by `BeginDraw`. Each
category records draw calls/failures, descriptor failures, the largest requested
pixel count, the largest observed backing-surface pixel count, and at most
16 distinct width/height/sample-count combinations with observation counts.
Additional combinations increment an overflow counter; the maximum continues
to update. No surface pointer, virtual address or pixel content is retained.

These observations are **not** unique allocation counts, simultaneously live
buffer counts, allocation bytes or physical memory measurements. Separate draws
may receive views of shared backing storage, and the same dimensions can refer
to different storage. Summing their sizes would therefore be misleading.
`surface_pixel_bytes` continues to describe only the renderer's nominal RGBA
canvas/atlas/feedback pixels. It excludes backing-buffer padding, compositor
buffers, caches, executable pages and the rest of the process.

The native reuse regression test verifies stable COM-object reuse and correct
font/size/color invalidation, including ellipsis, gradient colors, gamma and
extend mode. Matched no-input and resize runs confirm that the eight-window
workload retains eight gradient brushes and one text format while layouts are
still rebuilt. Whole-process working-set results remain variable; neither
resource reuse nor fixed nominal pixel storage establishes a memory-budget pass.
The measured components and run identities are recorded separately in
[performance validation](validation-performance.md).

`scripts/validate-render-reuse.ps1` provides an isolated visual guard for an
ordinary build. Give it `-BuildDirectory` containing `moon.exe` and `testwin.exe`,
and an empty `-OutputDirectory`. It refuses existing Moon/test windows, other
desktop managers and an outstanding recovery journal. Two owned helper windows
and a temporary configuration exercise resize/focus, light-theme and font changes,
gradient changes, border-slot removal, and restoration of the original styling.
Every capture waits for native frames to match the committed placements and saves
renderer counters with the scene and actual screen pixels. The capture includes
layered windows through
[BitBlt's CAPTUREBLT flag](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-bitblt).
Normal cleanup requests the exact owned daemon's exit, checks frame restoration,
and closes only its retained helper process's windows.

For pixel inspection with ignored background apps still open, pass
`-InspectUnderlay`. Before each capture, the harness verifies process/window
ownership and places only its own layered underlays immediately below its lowest
helper window, without moving, resizing or activating them. This uses
[SetWindowPos's relative Z-order](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos).
The report labels the override and records the affected handles. It isolates
rendered pixels from background-window occlusion; it does **not** verify the
product's normal Z-order. Teardown destroys those owned underlays with the daemon.

`-CompileOnly` checks the PowerShell script and compiles its native helper without
opening windows. That check and both fixture TOML checks passed; a full native
run is separate. An automated run leaves `visual_review_status` as `pending`:
the saved images still need review for gradient continuity, corners, focus colors,
ellipsis and bundled symbols. Capturing screenshots is not visual acceptance.

The final corrected D583 candidate completed this run on 13 November. All 13
image hashes were verified and the images were reviewed: complete four-edge
gradients and rounded corners, focus and theme changes, resize behavior, removal
of the old split border, readable larger-font workspace labels and bundled
symbols passed at 3840×2160/125%. The review is explicitly scoped to the
inspection Z-order described above. Reports:
`target/workspace-labels-visual/{report,visual-review,pixel-review}.json`.

The default is WARP software rasterization with
`D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS`, which avoids
WARP's additional raster-worker pool for this small, infrequently updated scene.
DirectComposition still composes the visuals. Before drawing a changed scene,
moon waits for the previous commit to finish processing, then draws and commits
once. This bounds outstanding commits while returning immediately to the message
loop after submitting the current scene. The first scene has no preceding commit
to wait for. Unchanged scenes neither draw nor wait. Explicit
`MOON_GRAPHICS_DRIVER=hardware`, `MOON_WARP_SERIAL=0`, and
`MOON_COMPOSITION_WAIT=0` overrides remain available for controlled diagnostics;
`MOON_COMPOSITION_WAIT=after` (or the legacy value `1`) restores waiting after
submission, and `before` explicitly selects the default.
If explicitly requested hardware initialization fails, creation falls back to
WARP. The embedded font has a retained lifetime owner, avoiding DirectWrite's
otherwise unnecessary copy of its static bytes.

Actual cross-process mouse injection proved that
`WS_EX_LAYERED | WS_EX_TRANSPARENT` passes clicks through, while a disabled,
nonlayered HWND did not. Compact per-edge HWNDs did not materially improve memory
in the comparison, so the implementation retains the plan's one-underlay-HWND
design. The major additional allocation came from WARP's lazy raster-worker
pool and queued composition frames.

The AppBar reserves the configured height and removes that reservation on drop.
It re-registers when Explorer restarts. The AppBar and tray allow only their
required callback and Explorer-restart messages through the per-window message
filter when running with UIAccess. Fullscreen hides the bar and releases
its reservation. Native DWM edges are suppressed for ringed tiled windows and
colored for floating windows. Shutdown restores a captured original color when
Windows permits reading it; otherwise it resets to Windows' default border color.
`DWMWA_BORDER_COLOR` is documented as a setter attribute, and this machine
rejects its getter, so exact restoration of an application's custom color cannot
be guaranteed through the documented API.
Each native-edge record includes its expected process owner. Capture, color
updates, retirement, and shutdown restoration verify the current owner before
accessing a window, discarding stale records when a handle changes owner. This
uses the same PID identity boundary as other native effects; a recycled handle
within the same process is not distinguishable by PID alone. The recovery
journal independently preserves readable original colors for crash recovery.
The `system` backdrop maps to Windows' automatic backdrop selection; `mica` and
`acrylic` map to the main-window and transient-window backdrops respectively.

Bar interaction uses press-and-release activation: releasing over a different
action, dragging outside, or losing capture cancels the press. Hover feedback
uses one reusable pixel surface and one finite compositor opacity curve per
bar, with an 83 ms response and a 167 ms entrance. There is no animation timer
or idle polling. New input replaces the existing curve rather than queuing
animations. Active workspace buttons also have a short selection underline.
Human-readable labels appear in native tooltips, with text and rectangles
updated only when they change. Text padding, selection indicators, and icon
sizes use the window's actual DPI.

`system_accessibility()` reads the Windows client-area animation preference and
high-contrast state. Reduced motion makes interaction changes immediate. High
contrast uses Windows' semantic background/text/highlight pairs, disables
backdrops, and replaces gradients with solid focus colors. Appearance messages
invalidate these preferences without introducing a polling timer. The tray
uses the original Moon icon family, exact DPI sizes, contrast-aware monochrome
variants, and committed Pause/Resume state. Its Settings command opens the
separate normal-token companion; it does not embed a web runtime in the daemon.
The icon assets and deterministic Windows icon/version resource generator are
documented in `crates/moon-win/assets/icons/README.md`.

The following performance measurements precede this interaction polish. Its
one-pixel feedback texture adds four nominal bytes per bar, but that does not
account for DLL, tooltip, icon, or compositor overhead. The full-process memory
budget must be measured again against the completed application.

A separate provider worker samples local CPU, memory, network, battery, volume,
keyboard language and clock values no faster than once a second. Configuration
changes select the built-in provider families needed by bar templates, including
subkeys and error values. Literal templates do not poll unrelated providers.
Formats on `exec` widgets still count because they also control optional-widget
visibility. Direct `exec` widget IDs retain their provider dependencies and the
existing fallback/override order when an ID overlaps a built-in value, even
with the bar disabled. Explicit custom commands keep their configured schedule.

Media metadata uses pending WinRT operations and cancellation after five seconds;
it never waits for a player on the message-loop thread. When media becomes
unused, Moon requests cancellation and releases its retained requests, manager
and metadata. Re-enabling media permits acquisition on the next sample without
waiting for the previous retry deadline. Re-enabled CPU sampling starts with a
fresh baseline. Missing hardware stays absent; failed providers expose
`<provider>.error` instead of fabricated values.
`system.theme` follows Microsoft's documented `UISettings` foreground-luminance
test and supplies `dark` or `light` to both the bar palette and native floating
window border colors. It remains sampled for the system theme setting or an
explicit theme-value dependency, including when the bar is disabled. These
selection rules do not change the regular maintenance loop or establish a CPU
performance improvement by themselves.

`exec` widgets run through the medium-integrity desktop shell. The capture
worker joins a kill-on-close Windows job before executing the command. Output
is bounded to 4 KiB and runtime to five seconds. Results are cached for each
widget's configured interval. Reloading a changed command invalidates its cache.

Weather makes no request unless a location is explicitly configured and a
template or direct `exec` ID needs a weather value. It uses
Open-Meteo HTTPS with Windows certificate validation, finite request timeouts,
a bounded response and a location-specific cache (15 minutes by default).
Open-Meteo weather data is attributed by `weather.source`. No API key is needed.

## Reproducible Windows probes

Build these examples for the Windows MSVC target with `cargo xwin build -p
moon-win --examples --target x86_64-pc-windows-msvc`. They are Windows-only; Linux
builds expose a diagnostic entry point without importing Windows APIs.

- `providers_probe.exe` asserts actual local provider output, weather absent by
  default, ordinary shell command output, timeout termination and output bounds.
  `--weather` explicitly fetches at the public test coordinate 0,0, without
  transmitting a personal location.
- `surface_smoke.exe 45` renders for 45 seconds, prints resource counts and then
  releases its windows and AppBar reservation. It manages only already-open
  `testwin` windows and restores their original placement through an owning
  guard. Run `testwin.exe 2` before it to inspect rings.
- `memory_probe.exe providers` and `memory_probe.exe graphics` isolate the
  local-provider worker and initialized graphics device. Setting
  `MOON_GRAPHICS_DRIVER=hardware` selects the hardware Direct3D device for comparison.
- `daemon_performance.exe moon.exe testwin.exe` measures the actual daemon with
  eight owned windows, 128 injected resize chords, and a ten-second idle period.
  It reports whole-process memory, startup, hook and observed placement latency.
  `MOON_PERF_NO_SURFACES=1` disables the bar and borders for an isolation run.
- `input_transparency_probe.exe testwin.exe` injects clicks at all four ring edges
  into an owned window in another process and verifies the receiver counted them.

Visual tests should isolate moon from tacky-borders to attribute the displayed
rings. They should also cover real monitor scaling at 100%, 125% and 150%; a
simulated scene scale does not prove the monitor DPI behavior.

The isolated release probes on the development machine (one 3840×2160 monitor,
125% scaling, RTX 3070 Ti) measured 19,886,080 bytes for providers, 45,486,080 bytes
for hardware graphics initialization, and 22,093,824 bytes for WARP graphics
initialization. These are whole process working sets, not private-byte figures
or estimates with shared pages subtracted. They do not by themselves establish
the full application's 50 MB budget.

The current packed-atlas release profile with serial WARP and composition waits
was also measured in the full eight-window daemon: 48,893,952 bytes working set
after 128 resize chords, approximately 120 ms startup, and 2,865,152 nominal
surface bytes before, after, and following idle. Nominal surface bytes count
allocated RGBA surface capacity, not compositor copies or total memory. The
working-set result satisfies the strict 50,000,000-byte budget on this machine;
it is not a promise for other driver, monitor, or window configurations.

The latency requirement remains unmet: a subsequent full run measured external
key-to-placement median 14.149 ms and p99 55.182 ms. The documented measurement
polls native frame rectangles at one-millisecond intervals. No latency acceptance
or soak completion follows from the memory result.

Controlled comparisons used the same immutable daemon (SHA-256
`c9e1394f210ced1bd8b35b7f06b6e35eb3ea1ca57e38f9edd2f8340f6622916a`),
updated measurement probe, eight owned windows, 128 resize chords, ten-second
idle interval, and identical one-millisecond frame polling. All 128 chords
completed in each profile without daemon errors. Figures below are one run per
profile, not a statistical performance guarantee.

| Profile | Ready | External median / p99 | Working set after input / idle | Idle CPU, one core |
| --- | ---: | ---: | ---: | ---: |
| Serial WARP, no composition wait | 117.895 ms | 14.901 / 29.653 ms | 49,065,984 / 46,923,776 B | 0% measured |
| Hardware, composition wait | 304.175 ms | 14.673 / 27.674 ms | 72,699,904 / 73,089,024 B | 0.469% |
| Bar and borders disabled | 51.630 ms | 13.855 / 35.245 ms | 23,793,664 / 23,851,008 B | 0% measured |

The hardware profile exceeds the startup and memory budgets. Similar external
median latency with surfaces disabled indicates that removing graphics alone
does not resolve the latency requirement. No-wait stayed within the memory
budget in that real-key run, but this does not hold for rapid updates. Raw
comparison JSON is recorded under `target/performance-ab-20261112/` during local
validation. Polling cadence should be calibrated before interpreting sub-frame
placement timing as the renderer's cost.

A matched renderer-only burst comparison used the current packed-atlas smoke
binary (SHA-256
`b481a639e9854f70b00afb14ca969f8a21efdf83366beb10d01032472ae50e39`),
eight owned windows, ten focus warmup updates, and 128 immediate resize updates.
The profiles differed only in composition waits. The probe allowed one second
for composition to settle before starting providers, then ran providers for ten
seconds. Surface capacity remained exactly 2,865,152 bytes in both profiles.

| Composition wait | Before burst | Immediately after / one-second settled | Final, with providers | Peak working set |
| --- | ---: | ---: | ---: | ---: |
| Disabled | 54,157,312 B | 62,357,504 / 62,357,504 B | 65,662,976 B | 74,162,176 B |
| Enabled | 33,796,096 B | 33,116,160 / 33,116,160 B | 40,853,504 B | 42,913,792 B |

Both profiles used five graphics threads and zero GDI objects. The peak is
Windows' process peak working set, sampled from the live process during the run.
Median render/update duration was 2.394 ms without waits and 16.621 ms with waits;
p99 was 3.790 ms and 17.525 ms respectively. This isolated rendering benchmark
does not measure real key-to-window placement. It confirms that the current
atlas still exceeds the memory budget without completion waits, so the default
retains them. Raw data and runner are in `target/performance-burst-20261112/`.

The selected `MOON_COMPOSITION_WAIT=before` mode moves completion waiting to the
start of the next changed scene, before drawing resources are reused. It still
allows at most one outstanding commit, and submits the current scene without
waiting afterward. Microsoft's completion contract refers to processing the
previous commit; it does not promise that the frame is already displayed. The
same eight-window, 128-update workload with this diagnostic measured a
42,819,584-byte peak, 33,054,720 bytes after the burst and after one second, and
40,718,336 bytes with providers. Update median/p99 was 16.704/17.737 ms, with
unchanged nominal pixel capacity. This confirms the isolated memory bound.

A subsequent full-daemon comparison on the same candidate measured the following
with 128 injected resize chords. The pre-draw wait mode is now the default.

| Wait placement | External median / p99 | Working set | Key dispatch p99 |
| --- | ---: | ---: | ---: |
| After current commit | 13.488 / 51.547 ms | 53,616,640 B | 5.969 ms |
| Before next changed scene | 14.922 / 35.367 ms | 48,619,520 B | 0.450 ms |

The pre-draw profile started in 134 ms. It preserves the burst memory bound and
returns to event dispatch sooner after submitting a scene, with no renderer
thread or accumulating work queue. These are individual controlled runs; the
key-to-placement latency requirement still fails and no latency acceptance is
claimed. Raw full-daemon evidence is in `target/predraw-ab-20261112/`.

A subsequent full run of the selected defaults on candidate
`fb29ac339173a37b48b5fca1d43f40340d7f8ea22b7c9388d7ef0c6f43f5ed35`
passed all 128 placements, measuring 14.415/40.263 ms external median/p99,
48,586,752 bytes after idle, 114.233 ms readiness and 0.156% of one CPU core
over ten seconds. Run-to-run tails vary; both full pre-draw runs remain above
the placement budget. Raw evidence is in `target/final-validation-20261112/`.

Actual visual inspection at 125% monitor scaling confirmed eight window rings,
continuous gradients, rounded corners, the bar, pills, and bundled icon glyphs.
The four-edge cross-process click test passed. An atlas packing test verifies
pixel-area preservation and nonoverlapping source and destination slices across
mixed horizontal and vertical strips. Physical 100% and 150% monitor scaling
remain to be verified; simulated geometry is not equivalent evidence.

## Sources checked on 2026-11-12

- [DirectComposition initialization](https://learn.microsoft.com/en-us/windows/win32/directcomp/initialize-directcomposition)
- [DirectComposition surface drawing lifetime and offsets](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositionsurface-begindraw)
- [Visual clipping and transforms](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositionvisual-setclip(constd2d_rect_f_))
- [Composition completion](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice-waitforcommitcompletion)
- [Direct3D device flags and WARP raster workers](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_create_device_flag)
- [Direct2D texture-cache limit](https://learn.microsoft.com/en-us/windows/win32/api/d2d1_1/nf-d2d1_1-id2d1device-setmaximumtexturememory)
- [In-memory font ownership](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_3/nf-dwrite_3-idwriteinmemoryfontfileloader-createinmemoryfontfilereference)
- [DirectWrite factory and custom font collections](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_3/nn-dwrite_3-idwritefactory3)
- [Windows AppBar registration and notifications](https://learn.microsoft.com/en-us/windows/win32/shell/application-desktop-toolbars)
- [Per-window message filtering](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-changewindowmessagefilterex)
- [Notification-area versioning and Explorer behavior](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw)
- [Media session manager API](https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssessionmanager.requestasync)
- [Windows light/dark theme detection](https://learn.microsoft.com/windows/apps/desktop/modernize/apply-windows-themes)
- [Window process ownership lookup](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid)
- [Reading original DWM attributes](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmgetwindowattribute)
- [System backdrop selection](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type)
- [Windows motion guidance](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/motion)
- [High-contrast semantic colors](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/high-contrast-themes)
- [Animation and contrast preferences](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow)
- [Common-control class initialization](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/ns-commctrl-initcommoncontrolsex)
- [WinHTTP finite timeouts](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpsettimeouts)
- [Open-Meteo current-weather parameters](https://open-meteo.com/en/docs)
- [Nerd Fonts v3.5.1 release](https://github.com/ryanoasis/nerd-fonts/releases/tag/v3.5.1)

Exact Rust method signatures were checked against the downloaded `windows`
0.62.2, `windows-future` 0.3.2 and `windows-numerics` 0.3.1 crate sources before
cross-compilation. These compatible versions are pinned in Cargo.toml/Cargo.lock.

Additional renderer references checked on 2026-11-13:

- [Direct2D resource reuse and command lifetime](https://learn.microsoft.com/en-us/windows/win32/direct2d/improving-direct2d-performance)
- [Linear-gradient brush lifetime and coordinates](https://learn.microsoft.com/en-us/windows/win32/api/d2d1/nn-d2d1-id2d1lineargradientbrush)
- [DirectWrite text-layout inputs](https://learn.microsoft.com/en-us/windows/win32/api/dwrite/nf-dwrite-idwritefactory-createtextlayout)
- [DXGI surface descriptions](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgisurface-getdesc)
