# Gaming with Moon

Moon starts a game session when the foreground app covers a monitor in native fullscreen and Windows reports Game Mode. The signal is device-wide. Moon associates it with the foreground window, checks its monitor bounds and tracks that window across focus changes. Windows may not report a game in windowed mode. [Windows effective power notifications](https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powerregisterforeffectivepowermodenotifications), [fullscreen optimizations](https://devblogs.microsoft.com/directx/demystifying-full-screen-optimizations/).

## Choose which games to recognise

Open **Settings → Rules → Add game**, then **Choose an executable** or enter its process name. Choose the game itself if you launch it through Steam or another launcher. Use the title or class matchers when several windows share a process. **Always for this app** also works in windowed mode. **Never for this app** keeps Moon’s usual shortcuts and placement. With overlapping rules, the last matching explicit game preference wins. Save changes to apply the rule to a running game.

From **Settings → General → Gaming**, turn **Detect fullscreen games** off to use explicit rules only. A browser video entering fullscreen can hide the bar using Moon’s ordinary fullscreen behaviour. Its shortcuts continue working unless a game rule or Windows Game Mode also applies. You can toggle a window made fullscreen by Moon with the usual shortcut.

```toml
[gaming]
automatic = true
pass-every-key = false

[[rules]]
game-mode = "always"
matches = [{ process = { equals = "ExampleGame" } }]

[[rules]]
game-mode = "never"
matches = [{ process = { equals = "ExampleLauncher" } }]
```

For one session, focus the game, open Moon’s tray menu and choose **Use game mode for the last app**. **End game mode** stops recognition for that window until it closes. The equivalent commands are `moon game-mode start` and `moon game-mode end`. Run `moon query games` to inspect the tracked windows, monitors and recognition source.

## Switching away and returning

| Player action | What Moon does |
| --- | --- |
| Focus the game | Passes bindings such as **Alt + 1** and **Alt + arrow** to the app, leaves its window size alone, hides Moon’s bar and borders on that monitor and releases the top strip. |
| Press **Alt + Tab** | Windows chooses the next app. Both **Alt + Tab** and **Alt + Shift + Tab** continue to use the Windows switcher while the game is open, including a separate press to return. |
| Use a desktop app | Ordinary Moon shortcuts work again in that app. A fullscreen game still visible on a second monitor keeps its display clear. The bar returns on a display where the game minimises or a foreground desktop app covers it. |
| Close the game | Moon resumes the existing workspace layout and restores the configured task-switcher bindings after the last session ends. An existing manual pause remains in effect. |

**Ctrl + Alt + Shift + F12** still exits Moon and restores windows by default. To give the game that chord too, enable **Pass every key to the game**. Use **Quit moon** from the tray or `moon exit` to stop the running manager. After an interrupted shutdown, `moon restore` can recover the saved window placements.

## Verification

The Windows integration suite starts a release daemon, its real keyboard hook and two native windows. It measures the game’s frame, desktop work area and bar visibility, counts key-down and key-up at the game, switches out and back through Windows, and checks desktop shortcuts and recovery. The core suite covers a second monitor and the automatic classifier’s fullscreen and Windows Game Mode inputs. [Windows integration](../scripts/game-mode-integration.ps1), [two-monitor and detection tests](../crates/moon-core/src/gaming.rs).

## Prior art

| Windows manager | Behaviour and lesson for Moon |
| --- | --- |
| GlazeWM 3.10.1 | Manual pause disables management and ordinary shortcuts together. A player's request for automatic pause describes a game changing the display resolution and GlazeWM rearranging a second screen at launch and again on return. [Default configuration](https://github.com/glzr-io/glazewm/blob/v3.10.1/resources/assets/sample-config.yaml#L213-L216), [resolution-change report](https://github.com/glzr-io/glazewm/issues/729). |
| komorebi 0.1.41 and whkd 0.2.10 | komorebi can ignore an application window. Its hotkey daemon has a separate per-process `Ignore` for individual shortcuts and a global pause. In Moon, one app preference can control both placement and input. [Window rules](https://github.com/LGUG2Z/komorebi/blob/v0.1.41/docs/common-workflows/ignore-windows.md), [whkd configuration](https://github.com/LGUG2Z/whkd/blob/v0.2.10/README.md#L18-L37). |
| Seelen UI 2.8.11 | Seelen can suspend its webviews during Windows Game Mode. An Age of Empires II player reported that the bar and dock disappeared while Seelen continued routing `Alt+Tab` to its suspended switcher. [Game Mode setting](https://github.com/eythaann/Seelen-UI/blob/v2.8.11/libs/core/src/state/settings/mod.rs#L535-L536), [player report](https://github.com/eythaann/Seelen-UI/issues/1891). |
