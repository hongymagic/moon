# moon

A native Windows 11 window manager with an integrated bar, window borders, and configurable tiling and animated workspace transitions. The Rust daemon has no WebView, network listener, or scripting runtime. A separate settings window uses the shared Windows WebView2 Runtime only while it is open.

## Build and run

From WSL, `just bootstrap` installs the pinned build tools and caches Microsoft's SDK/CRT. This accepts their redistribution/build-tool terms. `just build` cross-compiles the Windows binaries; `just test` tests the portable core on Linux. `just check` runs formatting, both-target lint checks, tests, dependency licences and vulnerability checks.

`just run doctor` inspects Windows capabilities, monitors, competing window managers and configuration. The executable also runs directly on Windows. With no configuration file, its built-in defaults apply.

```text
moon start --only testwin --no-hook
moon query world
moon focus left
moon move right
moon resize width +2%
moon toggle-direction
moon workspace 2
moon move-to-workspace 1 --follow
moon restart
moon exit
```

`--only testwin` restricts all management to test windows and disables keyboard interception and surfaces. `--surfaces` opts into bar/border testing in that mode. `--no-hook` is also available during debugging. An existing manager must ignore `testwin` before both run together.

`moon start` refuses a competing window manager. `moon start --takeover` asks GlazeWM to exit cleanly before adopting windows. The emergency recovery key is **Ctrl+Alt+Shift+F12**; it is registered independently of the keyboard hook. `moon restore` recovers a crashed manager's windows from its journal.

## Configuration

The default location is `%USERPROFILE%\.config\moon\config.toml`; override it with `--config PATH`. The file is optional and only needs overrides:

```toml
config-version = 1

[gaps]
inner = 16
outer = 32

[bindings]
"alt+enter" = ["exec wt.exe"]

[[rules]]
commands = ["move-to-workspace 2"]
matches = [{ process = { equals = "notepad" } }]
```

Bindings merge by chord, rules append, and other lists replace. Invalid reloads retain the last valid configuration and show an error on the bar. An `exec` widget explicitly opts into running its command through the ordinary desktop shell, with bounded output and execution time. A weather section explicitly opts into Open-Meteo HTTPS requests; no location means no weather requests.

`moon config default`, `show`, `validate`, `schema`, `reference`, `path` and `migrate` inspect the live schema and configuration. Migration preserves comments and creates a backup before writing. `moon cheatsheet` opens searchable current shortcut help in Settings; a standalone executable retains text help.

Open **Moon** from Start, choose **Settings** in the tray menu, or run `moon settings`. The settings pages cover window behavior, themes, border colors and gradients, workspaces, shortcuts, app rules, and advanced configuration. General shows where your windows are assigned. Changes remain a draft until saved; **Undo** restores the last draft change, and invalid changes or conflicting external edits leave the existing file intact. **Reload** reads the latest saved file after confirmation. Save feedback distinguishes stored settings from settings loaded by Moon. Settings respects Windows contrast and reduced-motion preferences.

**Ctrl+Alt+Space** opens Overview to find a window across workspaces, see its app
icon and an available live preview, move it by dragging its handle or choosing
**Move**, and create an app rule from its current details. Focusing a window on
another workspace uses the workspace carousel. **Ctrl+Alt+Z** undoes the last
eligible layout change; this is separate from undoing a Settings draft.
Appearance includes a theme gallery, while Workspaces includes named saved
setups. Both stage changes before saving and support sharing through Windows
file dialogs. See [themes and saved setups](docs/desktop-library.md).

The settings frontend uses pinned Bun 1.4.4 and TypeScript 7.0.2. `just settings` checks and rebuilds its bundled assets; `just settings-test` runs its browser interaction tests. Generated assets are checked in so a native Windows build does not require a web toolchain. See [settings persistence](docs/settings-editing.md) for exact merge and save behavior.

## IPC and recovery

The public API is version 1, newline-delimited JSON over Windows named pipes restricted to the current logon session. Clients verify the server process on a single-instance endpoint before sending requests. Remote, low-integrity and AppContainer clients are refused. `moon query`, ordinary commands and `moon subscribe` use the same API. No keystrokes are exposed by the protocol or recorded in traces. See [the protocol example](examples/README.md).

Before changing a window, moon saves its original native placement, border colour and process identity under `%LOCALAPPDATA%\moon`. The watchdog restores this state when the manager exits unexpectedly. Restart transfers workspace/tree state to the successor. Handle/PID reuse is checked against process creation time before recovery.

## Installation

The Windows package opens through **Install Moon.cmd**. Its guided setup handles first installation or update, progress, cancellation, and recovery. It adds a Start entry and an Installed Apps listing. See [installation and recovery](docs/installation.md).

`just install` builds the UIAccess daemon and ordinary-permission Settings companion, requests Windows elevation, creates or reuses a non-exportable signing certificate, verifies signatures, installs under Program Files and registers a least-privilege logon task. It preserves existing PATH entries. Removal restores windows and removes the installation's owned entries, files and certificate; personal configuration is retained.

UIAccess and resilience acceptance must be tested on the actual interactive Windows account. Elevated CI jobs cannot establish those results. The seven-day daily-driver soak in [the build plan](.claude/plans/moon-v1.md) is an elapsed-time acceptance condition, separate from compilation and automated tests. Current measured results are recorded in `docs/validation-*.md`.

## Structure

- `moon-core`: pure state, commands, layout, configuration, scenes and protocol.
- `moon-win`: all Win32/COM calls, hooks, pipes, native rendering and providers.
- `moon`: CLI, single-writer event loop, write-ahead journal and watchdog.
- `moon-settings`: ordinary-permission settings host and atomic configuration persistence.
- `apps/settings`: bundled TypeScript settings interface and interaction tests.
- `testwin`: dedicated windows for integration and capability tests.

This repository remains private. No project redistribution licence has been selected. Third-party asset notices are kept beside the assets. Research links and compatibility decisions are in [docs/research.md](docs/research.md).
