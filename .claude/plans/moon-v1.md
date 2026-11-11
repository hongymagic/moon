# moon: build plan

Status: implementation and Windows acceptance in progress, 2026-11-12. Source is committed on `codex/moon-v1`; measured results and remaining acceptance conditions are tracked in `docs/acceptance-status.md`. M5 and the seven-day sole-manager soak are not complete.

## Intent

moon is one native Windows 11 program that tiles windows into workspaces and draws its own top bar and window borders. It replaces GlazeWM, zebar and tacky-borders with one process and one config file. It also runs with no config file at all, because the built-in defaults reproduce today's setup.

v1 is done when moon has been the only window manager on this machine for a week, with today's keybindings, rules, bar and borders, and nothing from the old stack starts at logon.

## Why build rather than adopt

- **Nothing mature does this in one native binary.** Seelen UI is a WebView desktop environment. Whim (.NET, in-process bar) has stalled since late 2025. komorebi is four processes under a personal-use licence. The integrated attempts (MoraWM, Illium, AkuWM) are new and unfinished.
- **GlazeWM has stalled.** The latest release is 3.10.1 (March 2026), with no commits on main since April 2026 and 40+ open PRs. 3.10.x also breaks pause mode (#1370), so upgrading from 3.9.1 is not free.
- **Three processes keep three copies of the truth.** Zebar pulls WM state over a local WebSocket. tacky-borders chases windows through WinEvents and polls the foreground window every 100 ms. The tacky-borders log on this machine holds 2,278 "access is denied" and 820 "handle is invalid" errors.
- **The top unmet GlazeWM requests are design choices, not patches.** Tabbed/stacked layouts (#301), moving one window to another monitor (#973, noted in the current config), lost workspace assignments after a crash or Explorer restart (#1241), and slowdown after days of uptime (#1225).

## Qualities

These are requirements. Each has a check, and a milestone that breaks one is not done.

**Sane defaults.** moon runs with no config file. The defaults compiled into the binary reproduce today's setup: the `alt` keybindings, ten workspaces, the gaps, the stock ignore and float rules, the bar's layout and look, the border style and the Genesis Crest palette. Your own config holds only what is personal to this machine: which app goes to which workspace, the workspace icons and the weather location. That is a few dozen lines against roughly 350 in today's GlazeWM config alone.
*Check:* started with no config, moon passes a scenario that exercises every binding in today's GlazeWM config. `moon config default` prints the built-in file, which is parsed and tested like any other.

**Easy to use.** One binary. `moon doctor` says what is wrong: a second WM running, a missing capability, a bad config. Config errors appear on the bar with file, line and a suggestion. A keybinding cheat sheet is one hotkey away and is generated from the live bindings. A notification-area icon offers pause, reload, open config and quit. moon refuses to start over a running GlazeWM or komorebi unless told to take over.
*Check:* on a clean Hyper-V VM, going from a copied exe to tiled windows takes one command and no config.

**Customisable.** Everything in the defaults can be overridden: keys and binding modes, rules, gaps, workspaces, bar widgets and their styles, borders and the palette, per monitor or per workspace where that makes sense. Keys merge by chord, rules append, lists replace. Changes apply on save, with no restart.
*Check:* every option in the schema has a test that changes it. The reference docs are generated from the same schema, so they cannot drift.

**Extensible.** Three layers, cheapest first:
1. **Config.** Bar widgets are format templates over provider data, so a new widget is a config entry. `exec` widgets, event hooks and command aliases cover scripts.
2. **IPC.** A versioned, documented protocol (query, command, subscribe) that any language can use. This is the public API.
3. **Code.** Layouts, widgets and providers sit behind traits.

There is no in-process scripting or plugin runtime in v1 (D13).
*Check:* an example extension built only on the IPC ships in `examples/` and runs in CI.

**Performant.** Budgets, measured by the built-in trace and by benchmarks:
- hook callback under 50 µs at p99;
- key press to windows placed: median under 10 ms, p99 under 30 ms with eight windows;
- reducer plus layout under 100 µs for 50 windows;
- 0% CPU when nothing changes, and no timer faster than 1 Hz while idle;
- under 50 MB working set with bar and borders on one 4K monitor;
- ready in under 300 ms from start;
- after a week of uptime, latency within 10% of day one, with flat handle, GDI and memory counts.

*Check:* `moon query stats`, criterion benchmarks in CI, and the M5 soak.

**Easily updatable.** An update never costs you your layout: `moon restart` hands windows, workspaces and focus to the new binary. The config, the IPC protocol and the state file are each versioned, and old configs are migrated with their comments kept. For now an update is `just install` (build, sign, install, restart). A public release adds signed GitHub releases, winget and `moon update`.
*Check:* an upgrade test starts the previous build with a layout, installs the new one and asserts the same layout. A fixture config from every past `config-version` still loads.

**Secure.** See "Security model". In short: no network listener, nothing moon launches inherits its privileges, keys are never recorded, and there are no network requests or telemetry by default.

**Modern.** Windows 11 conventions throughout: per-monitor DPI v2, DirectWrite text, rounded corners that match the system radius, an optional Mica or acrylic bar backdrop, and an option to follow the system's light/dark setting. The handling is modern too: live reload, GPU-composited surfaces, event-driven everywhere. The visible half of "modern" (animations, gestures, an overview) comes straight after v1.
*Check:* visual review at 100%, 125% and 150% scaling.

**Reliable.** No window is ever lost: restore on exit and panic, a watchdog, a journal and `moon restore` (D12).
*Check:* kill tests at every milestone and the M5 resilience matrix.

## What v1 replaces

Read from the live configs under `~/.glzr` and `~/.config/tacky-borders` on the Windows side.

| Area | Today | v1 |
|---|---|---|
| Layout | GlazeWM manual split tree with toggle direction | Same semantics |
| Commands | Focus and move in four directions, resize ±2%, fullscreen, centred float toggle, close, reload, exit | Same commands on the same `alt` keys |
| Workspaces | Ten, named 1–10: focus, move and follow, back-and-forth on `alt+tab`, move workspace to next monitor | Same, plus move a single window to another monitor |
| Rules | Ignore, float, or assign to a workspace by process, class or title (equals, regex, not-regex) | The same rules, ported |
| Hiding | Cloak; every window stays in the taskbar | Same |
| Gaps | Inner 16, outer 32, top 64 to clear the bar | Inner 16, outer 32; the bar reserves its own space |
| Bar | zebar on WebView2 (settings schema 3.1), 40 px, every monitor: logo, workspaces (icon and colour each), clock, window title, paused/mode/direction, media, keyboard layout, network, memory, CPU, battery, volume, weather. No tray | Native bar with the same widgets and look. No tray |
| Borders | tacky-borders: 4 px, focused 45° gradient `#d17a4f` to `#a8613f`, unfocused `#29292e` | Same for tiled windows. Floating windows get the 1 px native border in the focus colour (D7) |
| Theme | One palette copied into three configs | One palette in one file |

Not in v1: a system tray in the bar, animations, scrolling/dwindle/tabbed layouts, scratchpad, launcher or overview, a settings GUI, in-process scripting, hiding the Windows taskbar (it stays on auto-hide), and packaging for other people. These are listed under "After v1".

## Environment

Measured on 2026-11-11.

- **Windows host:** Windows 11 Pro Insider build 26300.9974, x64. One 3840×2160 monitor at 125% scaling, 60 Hz, RTX 3070 Ti. Taskbar on auto-hide. Smart App Control off. Hyper-V present, Windows Sandbox not enabled. No Visual Studio, Build Tools or Rust on the Windows side.
- **WSL2 (Ubuntu 24.04):** rustc and cargo 1.99.1 with only the Linux target, plus `just`, `gh` and .NET SDK 10.0.403. No `cargo-xwin` yet.
- **Running today:** GlazeWM 3.9.1 (35 MB working set) and tacky-borders (44 MB). Zebar was not running when measured.

## Decisions

"Your call" marks a choice with a real alternative that only you can weigh. "Verify" names the spike that proves it on this machine.

**D1. Language: Rust, with `windows` 0.62 pinned and every Win32 call in one crate.** *Your call.*
There is no garbage collector on the keyboard-hook path, the binary is small, and it cross-builds from WSL. Both serious Windows tiling WMs are Rust, and GlazeWM v3 was a rewrite from C#. The cost is slow compiles and a bindings crate in transition: `windows` has had no release since October 2025 while its supporting crates moved to 0.100 with a different API. Keeping Win32 in one crate makes a later move a contained change.
Runner-up: C# on .NET with NativeAOT. Faster edit-run loop and the best debugger, but a GC pause can land on the hook thread and AOT cannot be built from WSL.

**D2. Shape: one process with a single-writer pipeline.** *Settled.*
This is OmniWM's design. Bar and borders are derived from WM state in the same pass that moves windows, so they cannot drift or lag. See "Architecture".

**D3. Layout: pure engines behind one trait, chosen per workspace. v1 ships the manual split tree.** *Your call.*
It matches today's bindings. Scrolling columns (OmniWM's default) and tabbed containers slot in later without touching platform code.
Runner-up: start with scrolling columns. That means new muscle memory and no parity milestone.

**D4. Hiding workspaces: cloak through the shell's undocumented application-view interface.** *Verify: S2.*
`DWMWA_CLOAK` is refused across processes, so GlazeWM, komorebi and Seelen UI all use this interface, and its IIDs have been stable since Windows 10. It works on this build today: windows on GlazeWM's hidden workspaces report as shell-cloaked. Every call is checked with `DWMWA_CLOAKED`, with parking windows off-screen as the fallback, per app if needed.
Shell cloaks outlive the process that set them, and GlazeWM's watcher does not undo them (#1358). moon journals cloaked windows on disk so `moon restore` and the watchdog can uncloak them.
Runner-up: `SW_HIDE` or minimise. Both trigger Windows' own animations and some apps misbehave.

**D5. Hotkeys: a `WH_KEYBOARD_LL` hook on a thread that only matches and enqueues.** *Verify: S3.*
It is what GlazeWM uses for today's bindings, `alt+tab` included. Windows silently removes a hook that stalls, and every keystroke on the system waits on that thread, so it does no other work. An emergency restore key is registered separately with `RegisterHotKey` so it survives a dropped hook. A `--no-hook` mode exists for debugging, because a breakpoint anywhere in the process freezes input.

**D6. Elevated windows: `uiAccess` in installed builds, never an elevated WM.** *Verify: S8, in M5.*
Without it the hook goes blind while an elevated window has focus, and focus cannot be taken from one. `uiAccess` needs a signature that chains to this machine's Trusted Root store and an install under Program Files; a self-signed certificate in that store meets the documented rule. The GlazeWM installed here already runs this way: its manifest carries the flag. Dev builds run without it and leave elevated windows alone. Users of GlazeWM's installer build still report elevated windows that will not move (#867), so this is a spike, not an assumption.
Runner-up: run the WM elevated from Task Scheduler. Everything it launches then inherits admin rights.

**D7. Borders: one click-through underlay per monitor, with rings drawn in the gaps.** *Verify: S6.*
Tiled windows never overlap, so a transparent window underneath them can draw every border in the gap around each window. There is no per-window z-order to track, which is the part that breaks for overlay borders (tacky-borders #111, reported on a 26H2 Insider build), and a layout change is one commit. The inner gap must be at least the border width (16 against 4 today).
The cost: floating windows are not in the gaps. In v1 they get the 1 px native border in the focus colour. A per-window overlay can follow if that is missed.
Runner-up: an overlay window per border, as tacky-borders and komorebi do.

**D8. Bar: a native popup per monitor, drawn with Direct2D and DirectWrite on DirectComposition.** *Settled; text quality verified by S7.*
Widgets are format templates over provider data, declared in the config and laid out with flexbox (Taffy). An `exec` widget covers the long tail. Icons come from a bundled symbols font, so the bar renders on a machine with no Nerd Font installed (licence check in M3). Idle cost is near zero and it shares one graphics device with the borders. WebView2 costs 100+ MB per view and has leaked GDI handles in zebar (#264).
Runner-up: egui. Quicker to a first bar, weaker text shaping.

**D9. Config: one strict TOML file layered over built-in defaults.** *Settled.*
The defaults are a real config file compiled into the binary. `moon config default` prints it and `moon config show` prints the merged result. Unknown keys are errors that come with a suggestion. The file carries a `config-version`; migrations are pure functions, and `moon config migrate` rewrites the file with comments intact (`toml_edit`). A generated JSON Schema gives editor completion and the reference docs. Live reload keeps the last good config and shows the error on the bar. One command grammar serves keybindings, rules, IPC and bar clicks, so `focus left` means the same thing everywhere.
Runner-up: KDL, as niri uses. Nicer nesting, weaker editor support.

**D10. IPC: a named pipe for the logon session, speaking newline-delimited JSON. It is the public API.** *Settled.*
`moon <command>` is the client, so there is one binary. The protocol offers query, command and subscribe, is versioned with a published schema, and only grows within a major version. There is no TCP or WebSocket listener. GlazeWM's localhost WebSocket has no authentication or origin check and exposes `shell-exec`.

**D11. Build: cross-compile in WSL for `x86_64-pc-windows-msvc` with `cargo-xwin`.** *Your call, on licence grounds.*
Nothing needs installing on Windows, the linker already ships with rustup, and komorebi cross-builds the same way. `cargo-xwin` downloads Microsoft's CRT and SDK, which means accepting the Build Tools licence terms. Microsoft has not said whether that use on Linux is covered.
Runner-up: the `x86_64-pc-windows-gnullvm` target with llvm-mingw. No Microsoft licence, but DWARF-only debugging.

**D12. Safety nets from the first milestone.** *Settled.*
Restore every window on exit and on panic; a watchdog process that does the same if the WM dies; an on-disk journal of managed and cloaked windows with a `moon restore` command; and a sandbox mode (`--only <process>`) that manages nothing but test windows.

**D13. Extensibility: config first, IPC second, traits third. No in-process scripting in v1.** *Your call.*
Extensions that run outside the process cannot crash or slow the WM, work in any language, and need no second API. Embedded scripting is powerful (OmniWM uses Luau for bar widgets; Hyprland moved its config to Lua), but it freezes an in-process API early and widens the attack surface of a program that sees every keystroke. Revisit once the IPC has settled. WASM is the candidate then, because it can be sandboxed.
Runner-up: embed a scripting runtime from the start.

**D14. Updates: a restart that keeps state, versioned formats, package managers later.** *Settled.*
`moon restart` writes the world to the journal and the new binary adopts it. With `uiAccess` the binary lives under Program Files, so an install needs one elevation prompt. That rules out silent self-patching and points to an installer through winget once moon is public. Until then, `just install`.

**Code hygiene.** komorebi's licence allows reading for reference but no derived works, so nothing is ported from it. GlazeWM is GPL-3.0, so porting its code would make moon GPL-3.0. tacky-borders, wiri, Whim and zebar's `systray-util` are MIT.

## Architecture

```
transports            intake               world                effector              surfaces
──────────────────    ─────────────────    ─────────────────    ──────────────────    ────────────────
WinEvent hooks    ┐   one ordered queue,   the only writer:     pure layout engine    bar and borders
keyboard hook     │   drained once per     reduce(event) is     gives target rects;   derived from the
IPC pipe          ├─▶ loop turn, bursts ─▶ pure and returns  ─▶ diff against last  ─▶ world, compared
display, power    │   coalesced            a plan; invariants   applied; one pass;    by value; only
timers, providers ┘                        checked, traced      stale plans dropped   changes drawn
```

Rules the code must keep:

1. **One writer.** Only the world stage changes WM state, and its reducer is a pure function from state and event to new state and plan.
2. **Layout engines never touch windows.** They map a workspace, a monitor rectangle and gaps to a rectangle per window.
3. **Only the effector moves, hides or focuses windows.** It applies a diff in one pass, skips windows already in place, and records what it did, so the events its own actions cause are recognised as echoes rather than user input.
4. **Surfaces hold no state of their own.** The bar and border scenes are computed from the world after each commit.
5. **Every undocumented API sits behind a capability** with a start-up probe and a named fallback.
6. **Everything is observable.** `moon query` returns the world and a ring of recent transactions as JSON. Tests and agents assert on that, not on screenshots.

Crates:

```
crates/
  moon-core/   no OS dependency and no unsafe: world, reducer, layout engines, command
               grammar, config schema and migrations, rule matching, bar and border
               scene derivation
  moon-win/    the only crate that touches Win32: transports, effector, renderer,
               providers, pipe server
  moon/        the binary: daemon, CLI client, watchdog, restore
  testwin/     helper exe that opens N plain windows for integration tests
```

Threads: the hook thread (match and enqueue only); the main thread (message loop, intake, world, effector, surfaces); and worker threads for providers and the pipe, which only post events. No async runtime to start with. Cross-process calls that can block (the cloak interface lives in Explorer) move to a platform worker if S5 shows the main thread stalling.

## Security model

moon is a sensitive program. Its hook sees every keystroke, an installed build can drive elevated windows, it starts at logon, and it launches programs. The commitments:

1. **No network listener.** IPC is a named pipe that rejects remote clients and is limited to the logon session. A medium-integrity `moon` client can reach it; low-integrity and AppContainer processes cannot.
2. **Nothing moon launches inherits its privileges.** Programs started from bindings, rules or `exec` widgets run as ordinary medium-integrity processes without `uiAccess`, started through the shell.
3. **Keys are compared, never recorded.** The hook matches against the binding table and discards. No key data reaches logs, traces or IPC.
4. **Quiet by default.** No network requests and no telemetry. A widget that fetches (weather) exists only if your config adds it, and update checks are opt-in.
5. **A small trusted base.** `moon-core` forbids `unsafe`; all of it lives in `moon-win`. The config and IPC parsers are fuzzed. Dependencies are few, the lockfile is committed, and `cargo-deny` and `cargo-audit` gate CI.
6. **Signing.** Personal builds are signed with a code-signing-only certificate whose private key is non-exportable and stays on this machine. Public releases are signed in CI, and `moon update` verifies the signature before installing.
7. **Local logs.** Logs stay under your profile, and window titles appear only at debug level.

Items 1 and 2 interact with `uiAccess`: an installed build may run at high integrity for an admin account, which changes both who can open the pipe and what a child process inherits. Both are measured in S8, not assumed.

## Milestones

Each milestone is a slice that can be demonstrated on its own. "Checks" are what must pass before it counts as done.

### M0. Skeleton and spikes

Prove the toolchain and the riskiest mechanisms on this machine before designing around them.

- Scaffold the workspace with hygiene from the first commit: a pinned toolchain, a committed lockfile, and formatting, clippy and `cargo-deny` behind `just check`.
- A `justfile` (`check`, `test`, `build`, `run`, `restore`) and the cross-build.
- Settle how the manifest gets embedded when linking from Linux. `embed-manifest` looks unusable on this route (read from its source, not run), so the candidates are linker flags, a `.res` file from `build.rs`, or patching the exe after the link.
- Run each spike as a small throwaway binary and write down pass or fail. Each one that passes stays on as a `moon doctor` probe.

| Spike | Passes when |
|---|---|
| S1 Enumerate | The list of manageable windows matches `glazewm query windows` for the same desktop |
| S2 Cloak | A test window cloaks and uncloaks through the application-view interface on build 26300, with `DWMWA_CLOAKED` confirming each state |
| S3 Hotkeys | The hook swallows `alt+h`, `alt+shift+1` and `alt+tab` with no menu activation and no task switcher, and survives lock and unlock |
| S4 Focus | Focusing a window from the hook-driven path succeeds 100 times out of 100 across three apps |
| S5 Placement | Eight test windows move with per-window async `SetWindowPos` and again with `DeferWindowPos`; flicker and hook-to-done latency are recorded and one method is chosen. No shipping WM uses `DeferWindowPos`, so it has to earn its place |
| S6 Underlay border | A 4 px gradient ring surrounds a tiled test window, is click-through, stays underneath, and is correct at 125% scaling |
| S7 Text | DirectWrite draws JetBrainsMono Nerd Font text and icon glyphs crisply in a transparent popup |

Checks: `just test` runs on Linux; `just run` starts `moon.exe` on Windows from WSL and prints the monitor list back to the shell; every spike has a recorded result, and a failed spike has switched to its fallback before M1 starts.

### M1. First tile

One monitor, one workspace, driven over IPC, running beside GlazeWM in sandbox mode.

- Core: world, reducer, split-tree engine and the command grammar for focus, move, resize, toggle direction, close, fullscreen and float.
- Platform: WinEvent intake, the manageable-window filter, the effector, restore on exit, the journal and `moon restore`.
- IPC: the pipe, `moon query`, `moon <command>` and `moon doctor`.

Demo: with GlazeWM running and ignoring `testwin`, start `moon --only testwin` and open four test windows. They tile with 16 px gaps. `moon focus left`, `moon move right`, `moon resize width +2%` and `moon toggle-direction` behave as GlazeWM does. Closing one reflows the rest. Killing moon with `taskkill /F` leaves all four visible and usable.

Checks: a property test runs 100,000 random command sequences and keeps the invariants (every window appears exactly once, no empty containers, rectangles stay in bounds and never overlap, focus points at a live window or nothing). An integration scenario asserts that the real window rectangles equal the ones `moon query` reports, within 1 px. The reducer and layout benchmark meets its budget.

### M2. Workspaces, keys, rules, config

After this moon can take over from GlazeWM.

- Ten workspaces: switch, move and follow, back-and-forth, hiding by cloak.
- Hotkeys through the hook thread, binding modes, the emergency key.
- Window rules: ignore, float, assign.
- The built-in defaults and the layered TOML config: `moon config default`, `show` and `validate`, live reload, and the first migration test.
- IPC protocol v1 with its published schema, and one example extension built only on it.
- `moon restart` with state handed to the new process, the watchdog, and the guard that refuses to start over another WM.

Demo: `just takeover` exits GlazeWM and zebar and starts moon with no config file. Every current keybinding works. Add the short personal config and apps land on their assigned workspaces. `alt+shift+r` picks up an edit, and a typo keeps the old config and reports the error. `moon restart` leaves every window where it was. After `taskkill /F`, the watchdog uncloaks everything within a second. tacky-borders keeps running through this stage.

Checks: the no-config scenario covers every binding in today's GlazeWM config. The rule tests use the real rules from that config. An integration scenario asserts cloaked state per workspace. The hook and key-to-placed budgets are met, read from the built-in trace. The config and IPC parsers have fuzz targets that run in CI. A test confirms no key data reaches the log.

### M3. Bar

Retires zebar. This is the largest milestone.

- The shared graphics device, text, a popup per monitor, the widget tree and theme.
- The default bar reproduces today's zebar layout and look, with widgets as format templates over provider data.
- Widgets that need only WM state first: workspaces (click to switch), clock, title, mode, paused and direction.
- Then the providers: CPU, memory, network, battery, volume, media, keyboard layout and `exec`.
- AppBar registration plus inset replaces the 64 px top gap. The bar drops out of the way for fullscreen windows.
- Config errors on the bar, and the keybinding cheat sheet.

Demo: with no config, the bar matches today's zebar bar in a side-by-side screenshot, apart from the workspace icons and weather that the personal config adds. Workspace state updates in the same frame as a switch.

Checks: the bar scene is derived from world state by a pure function, so it has snapshot tests. The idle CPU and working-set budgets are met. The bar renders correctly on a machine with no Nerd Font installed.

### M4. Borders

Retires tacky-borders.

- The per-monitor underlay with a ring per tiled window: gradient when focused, solid otherwise.
- The native 1 px edge tinted or hidden so the ring reads as one border. Floating windows use the native edge in the focus colour.

Demo: with no config, borders match today's tacky-borders settings. They move in the same pass as their windows on move, resize and workspace switch, and vanish for fullscreen windows.

Checks: ring geometry is a pure function of the layout and has snapshot tests. Visual check at 100%, 125% and 150% scaling.

### M5. Daily driver

- **Install and update.** `just install` builds a release, signs it with the code-signing-only certificate, installs under Program Files and restarts in place. moon starts at logon.
- **Spike S8.** Hotkeys and placement still work while an elevated window has focus. A program launched by moon runs at medium integrity without `uiAccess`. The `moon` client reaches the daemon and a low-integrity process is refused.
- **Ease of use.** The notification-area icon. A first-run check on a clean Hyper-V VM: a copied exe, one command, no config, tiled windows.
- **Resilience matrix.** Sleep and wake, lock and unlock, Explorer restart, display added or removed, resolution and DPI change, a UAC prompt, an RDP session, a native virtual-desktop switch, WSLg and WinUI 3 windows, fullscreen games.
- **Multi-monitor.** Workspaces per monitor, moving a workspace or a window between monitors, cursor jump.
- **Soak.** A week as the only WM, with every papercut logged and the uptime budget measured at the end.

Done when: seven consecutive days without falling back to GlazeWM, no lost windows, every row of the resilience matrix passes, and the upgrade test passes. Then GlazeWM, zebar and tacky-borders come out of startup.

## Dev loop

Not yet run on this machine; M0 confirms it.

```bash
# one-off
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
XWIN_ARCH=x86_64 cargo xwin cache xwin   # downloads Microsoft's CRT and SDK

# every change
cargo test -p moon-core                                   # Linux-native
cargo xwin build --target x86_64-pc-windows-msvc
# copy moon.exe to %LOCALAPPDATA%\moon-dev and launch it through WSL interop;
# stdout and the exit code come back to the WSL shell
```

- **Sandbox mode** (`moon --only testwin`, no hook, driven over IPC) runs beside GlazeWM. It needs one ignore rule for `testwin` in the GlazeWM config.
- **Takeover mode** needs GlazeWM exited, not paused: both would install a keyboard hook and move the same windows. The launcher runs `glazewm command wm-exit` first.
- **Windows-only tests** run on the real desktop from WSL by overriding cargo's runner for the Windows target.
- **CI later:** GitHub's `windows-latest` is Windows Server 2025 with an interactive session. It can run the integration suite but cannot reproduce UIPI behaviour, because jobs run elevated.

## Risks

| Risk | What goes wrong | Mitigation | Proved by |
|---|---|---|---|
| Cloaking fails, or strands windows after a crash | Workspaces stop hiding, or windows stay invisible | Check every call, park off-screen as fallback, journal cloaked windows for `moon restore` and the watchdog | S2 |
| The keyboard hook is dropped or goes deaf | Hotkeys die | Enqueue-only hook thread, health check and re-install, separate emergency key, `uiAccess` | S3, S8 |
| Windows refuses a focus change | Focus commands fail silently | The empty-input trick GlazeWM and komorebi use, checked after each call, with a bounded `AttachThreadInput` fallback | S4 |
| The underlay misbehaves at the bottom of the z-order | Borders vanish or cover the desktop | Fall back to the native 1 px colour border | S6 |
| Apps fight placement or freeze when managed (minimum sizes, late windows, WSLg, WinUI 3) | Holes, overlaps, dead windows | Read back each frame and retry, feed size limits back to the layout, float after repeated refusal, per-app rules | M1, M5 |
| Developing a WM on the machine in use | Windows lost mid-session | Sandbox mode, journal and `moon restore`, watchdog, emergency key | M1 |
| The signing key for personal builds is stolen | Anything signed with it is trusted on this machine | Code-signing-only certificate, non-exportable key, removed on uninstall; public builds are signed in CI | M5 |
| Extensibility pulls in a plugin runtime early | An in-process API frozen too soon, and a larger attack surface | The IPC is the public API in v1; revisit after it settles (D13) | n/a |
| windows-rs moves from 0.62 to 0.100 | Bindings churn | One platform crate, pinned version | n/a |
| Scope creeps toward everything OmniWM does | v1 never ships | v1 is parity plus the quality bar; the backlog opens after the soak | n/a |

## After v1

Animations come first, because they are the visible half of "modern". The rest is unordered; pick after the soak.

- Animations through DWM-thumbnail proxies. The effector boundary is where they slot in; nothing is built for them in v1.
- Tabbed and stacked containers (the most requested GlazeWM feature).
- A scrolling-columns engine, as in OmniWM and niri.
- Scratchpad.
- Modifier-drag move and resize with drop zones.
- Overlay borders for floating windows.
- A system tray in the bar, behind a flag.
- A window switcher or overview.
- A plugin runtime (WASM), or external layout generators over IPC, if the IPC proves too limiting.
- A settings GUI that writes the same TOML file.
- An importer for GlazeWM configs.
- A public release: licence, CI signing through SignPath, an MSI on winget, `moon update`.

## Assumptions you can veto

The plan proceeds on these unless you say otherwise.

1. Rust, not C# (D1).
2. `cargo-xwin`, which downloads Microsoft's CRT and SDK under their Build Tools terms (D11). Starting M0 is what accepts those terms.
3. Today's manual split tree is the v1 layout; scrolling columns come later (D3).
4. Personal settings (app-to-workspace assignments, workspace icons, weather location) live in your config, not in the built-in defaults.
5. Floating windows get the 1 px native border in v1 (D7).
6. No in-process scripting in v1; the IPC is the extension API (D13).
7. Animations come after v1.
8. One ignore rule for `testwin` goes into your GlazeWM config so tests can run beside it.
9. The repo stays private for now; the licence is chosen before any public push.
10. The binary is called `moon`, which is also the CLI name of moonrepo and MoonBit. Neither is on PATH on this machine.

## Sources

- OmniWM architecture: https://github.com/OmniNull/OmniWM (`website/src/content/docs/developers/architecture.md`)
- GlazeWM issues: https://github.com/glzr-io/glazewm/issues/973, /301, /1241, /1225, /867, /1358, /1370
- `uiAccess` requirements: https://learn.microsoft.com/en-us/previous-versions/dotnet/articles/bb625963(v=msdn.10)
- Launching an unelevated process from an elevated one: https://devblogs.microsoft.com/oldnewthing/20131118-00/?p=2643
- `CreateNamedPipe` and rejecting remote clients: https://learn.microsoft.com/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew
- Named pipe security: https://learn.microsoft.com/windows/win32/ipc/named-pipe-security-and-access-rights
- Zebar GDI leak: https://github.com/glzr-io/zebar/issues/264
- tacky-borders on 26H2 and cost: https://github.com/lukeyou05/tacky-borders/issues/111, /110
- windows-rs direction: https://github.com/microsoft/windows-rs/issues/5246, /4867
- Low-level hook timeout: https://learn.microsoft.com/windows/win32/winmsg/lowlevelkeyboardproc
- DWM window attributes: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute
- WSL file systems: https://learn.microsoft.com/windows/wsl/filesystems
- cargo-xwin: https://crates.io/crates/cargo-xwin
- komorebi licence: https://lgug2z.com/software/komorebi
