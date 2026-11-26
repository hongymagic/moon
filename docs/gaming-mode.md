# Gaming mode for Moon

**Proposal, 26 November 2026.** A game should own its window and receive the player's keys. Tab out to Discord and Moon's shortcuts should work there. Press `Alt+Tab` again and Windows should take you back to the game.

## Current behaviour

- Moon recognises a captionless foreground window whose visible frame matches all four monitor edges within one pixel. It hides that monitor's bar and tiled borders and requests a zero-height AppBar reservation. Because detection follows the foreground, clicking a second monitor can bring the bar back over a still-visible borderless game. [Fullscreen detection](../crates/moon-win/src/platform.rs#L428), [scene](../crates/moon-core/src/scene.rs#L99), [AppBar reservation](../crates/moon-win/src/surfaces.rs#L1323).
- The defaults assign `Alt+Tab` to the recent workspace and `Alt+Shift+Tab` to moving a workspace. The keyboard hook can consume a chord and mask Alt before the daemon sees it. A window's fullscreen status has no effect on the binding table. An `ignore` rule excludes the game from tiling alone. [Default shortcuts](../crates/moon-core/src/default.toml#L55), [keyboard hook](../crates/moon-win/src/hooks.rs#L582), [binding updates](../crates/moon/src/daemon.rs#L798), [window rules](../crates/moon-core/src/world.rs#L739).
- A managed native-fullscreen window still gets a placement targeting the whole monitor. Applying it can restore a maximised window and call `SetWindowPos`. The maintenance loop can retry the move. Manual pause (`Alt+Shift+P`) stops the placement plan across the desktop, leaves the bar enabled and retains several shortcuts, including reload, exit and overview. [Placement plan](../crates/moon-core/src/world.rs#L271), [native placement](../crates/moon-win/src/platform.rs#L375), [maintenance](../crates/moon/src/daemon.rs#L1264), [pause bindings](../crates/moon-core/src/help.rs#L37).

## Prior art

| Windows manager | Behaviour and lesson for Moon |
| --- | --- |
| GlazeWM 3.10.1 | Manual pause disables management and ordinary shortcuts together. A player's request for automatic pause describes a game changing the display resolution and GlazeWM rearranging a second screen at launch and again on return. [Default configuration](https://github.com/glzr-io/glazewm/blob/v3.10.1/resources/assets/sample-config.yaml#L213-L216), [resolution-change report](https://github.com/glzr-io/glazewm/issues/729). |
| komorebi 0.1.41 and whkd 0.2.10 | komorebi can ignore an application window. Its hotkey daemon has a separate per-process `Ignore` for individual shortcuts and a global pause. In Moon, one app preference can control both placement and input. [Window rules](https://github.com/LGUG2Z/komorebi/blob/v0.1.41/docs/common-workflows/ignore-windows.md), [whkd configuration](https://github.com/LGUG2Z/whkd/blob/v0.2.10/README.md#L18-L37). |
| Seelen UI 2.8.11 | Seelen can suspend its webviews during Windows Game Mode. An Age of Empires II player reported that the bar and dock disappeared while Seelen continued routing `Alt+Tab` to its suspended switcher. [Game Mode setting](https://github.com/eythaann/Seelen-UI/blob/v2.8.11/libs/core/src/state/settings/mod.rs#L535-L536), [player report](https://github.com/eythaann/Seelen-UI/issues/1891). |

## Game recognition

Choose **Automatic** as the default. Start a session when an external foreground window enters native fullscreen and Windows reports `EffectivePowerModeGameMode`. Subscribe through `PowerRegisterForEffectivePowerModeNotifications` with `EFFECTIVE_POWER_MODE_V2`. It reports device-wide state at registration and on changes. Moon supplies the window and monitor identity and rechecks when either signal arrives. Once identified, keep the session tied to that game across foreground changes. Moon's pinned Windows bindings already expose this API. [Power notifications](https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powerregisterforeffectivepowermodenotifications), [power modes](https://learn.microsoft.com/en-us/windows/win32/api/powersetting/ne-powersetting-effective_power_mode), [Moon's Win32 features](../crates/moon-win/Cargo.toml).

Windows can present a fullscreen game as an optimised borderless window. The shell's `QUNS_RUNNING_D3D_FULL_SCREEN` signal covers exclusive Direct3D. Its AppBar notification carries an opening or closing flag. Frame size alone also matches a browser playing fullscreen video. In that case Moon should hide its bar and keep its desktop shortcuts. A window made fullscreen by Moon's own `Alt+F` should accept the same shortcut to leave it. [Microsoft's fullscreen explanation](https://devblogs.microsoft.com/directx/demystifying-full-screen-optimizations/), [shell notification states](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state), [AppBar notifications](https://learn.microsoft.com/en-us/windows/win32/shell/abn-fullscreenapp).

In Settings, **Always** or **Never** for a particular app should override detection. The player can select a windowed game or a game Windows misses before its first launch. Match the game executable, with optional class or title for shared executables, so Steam's library keeps its own rules. A tray action can apply game mode to the last external window for one session. **End game mode** should take effect until that game instance exits. [Rule matcher](../crates/moon-core/src/config.rs#L208).

## During play

| Player action | Proposed Moon behaviour |
| --- | --- |
| Play on either monitor | Pass ordinary shortcuts through, including `Alt+1` and `Alt+arrow`. Hide Moon's bar, borders, tooltips and workspace animation on the playing monitor and release the reserved space. The game controls its frame, including in windowed mode. Suppress pointer jumps, focus-follows-cursor and focus requests from newly opened background windows. |
| Open Steam's overlay or Xbox Game Bar | Forward input to the overlay and keep the game's display clear. Recheck a transient or unknown foreground before showing the bar. When a desktop app takes focus, enable its ordinary shortcuts immediately. |
| Click Discord on another monitor or use `Alt+Tab` | Restore ordinary Moon bindings for the desktop app. Keep `Alt+Tab` and `Alt+Shift+Tab` native in both directions for the session, including a second, separate press to return. Leave a still-visible fullscreen game's monitor clear. Show the bar on a monitor where minimising or covering the game exposes the desktop. |
| Close the game | Reconcile surviving windows once, respecting the workspace and foreground the player chose. A game crash should have the same result. Preserve a manual pause that preceded play. |

Keep `Ctrl+Alt+Shift+F12` as the sole Moon shortcut during play by default. An optional **Pass every key through** setting must also unregister this separate emergency hotkey while playing. Both the tray and `moon restore` provide recovery. The tray can identify the active game and what triggered game mode. [Emergency registration](../crates/moon-win/src/hooks.rs#L905), [recovery command](../crates/moon/src/main.rs#L336).

## Implementation

1. Check app rules before `adopt_existing` and at first show. For an unclassified window already covering a monitor, let its native frame stand while the foreground and bounds settle. Retain the workspace slot and recovery identity of a previously tiled game. A game recognised at launch can stay outside the tree. Guard moves, cloaking, focus, cursor jumps and DWM border writes wherever Moon applies them, including the direct retry in maintenance. [Adoption](../crates/moon/src/daemon.rs#L566), [window events](../crates/moon/src/daemon.rs#L1122), [native effects](../crates/moon/src/daemon.rs#L917), [DWM borders](../crates/moon-win/src/surfaces.rs#L1938).
2. Cache the classification of known windows outside the keyboard hook. For a new chord, check the actual foreground against that cache before matching bindings or masking Alt. Forward input while a possible game is being classified. Preserve key-down, repeat and key-up ownership across focus changes. Drop pending workspace switches and other queued commands from the previous input policy. [Hook callback contract](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc), [workspace shortcut queue](../crates/moon/src/workspace_shortcuts.rs).
3. Track the game's window handle, process ID, creation time and monitor separately from Moon's last managed focus. Use the visible game on each monitor to drive the bar, AppBar reservation and tiling inset. Batch transient resolution changes. Recheck live identities on window replacement, monitor removal, session unlock and daemon restart. A still-open automatic game can reclaim its old tile after it leaves fullscreen. An **Always** rule protects a game's chosen windowed size. [Tiling inset](../crates/moon-core/src/world.rs#L248), [system events](../crates/moon/src/daemon.rs#L1357), [recovery identity](../crates/moon/src/journal.rs#L221).

Build the app rule, tray action and shared input and placement controls first. Then connect automatic detection. The first stage can be tested with a real game even on a PC with Windows Game Mode switched off.

## Verification

All 106 portable `moon-core` tests passed during this investigation. Moon's recorded Windows integration covers a helper switching to borderless fullscreen. Real-game input, exclusive fullscreen and two-monitor recovery remain unverified. [Integration fixture](../scripts/integration.ps1), [acceptance record](acceptance-status.md).

| Scenario for implementation testing | Observable pass condition |
| --- | --- |
| Native helper plus real games in borderless, exclusive where supported, and explicitly selected windowed modes | The first gameplay chord and its release reach their normal recipient. Windows switches out and back on consecutive `Alt+Tab` presses. No Moon command fires during play apart from the selected emergency policy. |
| Two monitors at different scaling settings, an overlay, a lower game resolution and a newly opened background window | No active bar hit target or reserved top strip on the playing monitor. No Moon move, cloak, focus, cursor jump or repeated placement against the game. Desktop shortcuts work on the other monitor and its workspace survives the resolution round trip. |
| Browser F11, maximised editor, Moon fullscreen, game crash, lock and restart | Desktop shortcuts retain their expected behaviour in the controls. The bar, reservation, prior tile memberships and configured shortcuts recover after game exit. A stale or reused window handle receives no restoration call. |
