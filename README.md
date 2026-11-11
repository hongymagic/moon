# moon

A native Windows 11 tiling window manager with an integrated bar and window borders. Built in Rust; no WebView, network listener, or scripting runtime.

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

`moon config default`, `show`, `validate`, `schema`, `reference`, `path` and `migrate` inspect the live schema and configuration. Migration preserves comments and creates a backup before writing. The binding cheat sheet comes from the current bindings (`moon cheatsheet`).

## IPC and recovery

The public API is version 1, newline-delimited JSON over a named pipe restricted to the current logon session. Remote, low-integrity and AppContainer clients are refused. `moon query`, ordinary commands and `moon subscribe` use the same API. No keystrokes are exposed by the protocol or recorded in traces. See [the protocol example](examples/README.md).

Before changing a window, moon saves its original native placement, border colour and process identity under `%LOCALAPPDATA%\moon`. The watchdog restores this state when the manager exits unexpectedly. Restart transfers workspace/tree state to the successor. Handle/PID reuse is checked against process creation time before recovery.

## Installation

`just install` builds the UIAccess variant, requests Windows elevation for installation, creates or reuses a code-signing-only non-exportable certificate, signs the binary, verifies the signature, installs under Program Files and registers a least-privilege logon task. It adds its directory to the user's PATH for new Windows terminals, preserving existing entries. The Windows installation script also works directly. `scripts/uninstall.ps1` restores windows and removes its owned PATH entry, task, installed files and signing certificate; personal configuration is retained.

UIAccess and resilience acceptance must be tested on the actual interactive Windows account. Elevated CI jobs cannot establish those results. The seven-day daily-driver soak in [the build plan](.claude/plans/moon-v1.md) is an elapsed-time acceptance condition, separate from compilation and automated tests. Current measured results are recorded in `docs/validation-*.md`.

## Structure

- `moon-core`: pure state, commands, layout, configuration, scenes and protocol.
- `moon-win`: all Win32/COM calls, hooks, pipes, native rendering and providers.
- `moon`: CLI, single-writer event loop, write-ahead journal and watchdog.
- `testwin`: dedicated windows for integration and capability tests.

This repository remains private. No project redistribution licence has been selected. Third-party asset notices are kept beside the assets. Research links and compatibility decisions are in [docs/research.md](docs/research.md).
