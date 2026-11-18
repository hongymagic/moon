# Moon

Moon is a tiling window manager for Windows 11. It arranges your windows as you open them, shows your workspaces in a bar at the top of each monitor, and draws a border around the focused window. You can move between windows with the keyboard or click a workspace in the bar.

[Get started with Moon](https://moon.hongy.dev) for a visual installation guide, interactive keyboard walkthrough and screenshots.

## Install on Windows 11 x64

1. [Download Moon for Windows](https://moon.hongy.dev/download) and double-click the setup EXE from the account where you’ll use Moon.
2. Choose **Install Moon** and approve the Windows prompt. Setup installs Moon under Program Files and signs the window manager with a certificate on your PC so it can manage elevated windows.
3. Choose **Open Settings**. If setup offers **Get WebView2**, install Microsoft's Evergreen Runtime and return to setup. To reopen Settings later, search for **Moon** in Start.

Prefer a ZIP? Choose the `.zip` asset on the release page, use **Extract All**, then double-click **Install Moon.cmd**. Each download has a matching `.sha256` file you can compare with `Get-FileHash` in PowerShell. [Latest release and checksums](https://github.com/hongymagic/moon/releases/latest)

If you already use GlazeWM, Zebar or tacky-borders, setup can stop them and back up their startup configuration during the switch. Read the [handover and recovery guide](docs/installation.md) before installing on that desktop.

## Try it

Open two ordinary app windows on the same workspace, then try these default shortcuts.

| Action | Shortcut |
| --- | --- |
| Focus a neighboring window | **Alt + arrow** |
| Move the focused window in that direction | **Alt + Shift + arrow** |
| Switch to workspace 2 | **Alt + 2** |
| Move the focused window to workspace 2 and follow it | **Alt + Shift + 2** |
| Find a window across all workspaces | **Ctrl + Alt + Space** |
| Undo your last layout change | **Ctrl + Alt + Z** |

Click the crescent in the bar for your current shortcuts. To stop Moon and restore your windows, press **Ctrl + Alt + Shift + F12**.

## Make it yours

Open Settings to change shortcuts, workspace names and icons, gaps, or border colors. Choose **Save changes** to apply your changes. Under **General**, you can also choose whether Moon launches when you sign in.

You can edit the same settings as TOML in `%USERPROFILE%\.config\moon\config.toml`. For example, this sets the space between tiles to 12 pixels at 100% display scaling.

```toml
config-version = 1

[gaps]
inner = 12
```

Run `moon config path` to find the file and `moon config validate` to check it. See the [configuration reference](docs/config-reference.md), [themes and workspace setups](docs/desktop-library.md), and [CLI and IPC example](examples/README.md).

To update, run the newer setup EXE. To remove Moon, open **Settings → Apps → Installed apps → Moon → Uninstall**. Removal restores managed windows and keeps your personal configuration.

## Project

[Contribute or build from source](CONTRIBUTING.md) · [Report a bug](https://github.com/hongymagic/moon/issues/new/choose) · [Report a security issue](SECURITY.md)

Moon's original code, artwork and documentation use the [MIT license](LICENSE). The bundled font and software dependencies have their own [third-party notices](docs/third-party-notices.md).
