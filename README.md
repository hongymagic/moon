# Moon

Moon is a tiling window manager for Windows 11. It arranges your windows as you open them, shows your workspaces in a bar at the top of each monitor, and draws a border around the focused window. You can move between windows with the keyboard or click a workspace in the bar.

[Get started with Moon](https://moon.hongy.dev) for a visual installation guide, interactive keyboard walkthrough and screenshots.

## Install on Windows 11

Choose the installer that matches **System type** under **Settings → System → About**. [Download x64](https://moon.hongy.dev/download) · [Download Arm64](https://moon.hongy.dev/download?arch=arm64)

1. Double-click the setup EXE and choose **Install**. It installs Moon for your Windows account without PowerShell or administrator approval.
2. Exit any other tiling window manager, then select **Open Moon** on the finish page. If Settings needs WebView2, setup offers Microsoft's download.
3. Search for **Moon** in Start to open it again. You can turn sign-in startup on or off in Moon Settings.

The per-user edition manages ordinary windows. If you need to tile apps running as administrator or update an existing Program Files installation in place, choose the architecture-matched `-uiaccess.exe` on the release page. That edition uses Windows PowerShell and administrator approval and supports desktop handover. [Releases and checksums](https://github.com/hongymagic/moon/releases/latest) · [Installation and migration guide](docs/installation.md)

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

## Play a game

On a fullscreen game that Windows recognises, Moon gives the game its shortcuts and clears the bar, borders and tiling on that monitor. Use **Alt + Tab** to reach another app and press it again to return. Your ordinary Moon shortcuts work in the other app.

For a windowed game or a title Windows misses, open **Settings → Rules → Add game**, choose its executable and save. The tray also lets you start game mode for the last active app or end it for the current instance. [Gaming controls and examples](docs/gaming-mode.md)

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
