//! One command language for key bindings, rules, the CLI and IPC.
use crate::geometry::{Axis, Direction};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", content = "args", rename_all = "kebab-case")]
pub enum Command {
    Focus(Direction),
    Move(Direction),
    Resize { axis: Axis, delta: f64 },
    ToggleDirection,
    Direction(Axis),
    Close,
    ToggleFullscreen,
    ToggleFloating,
    SetFloating,
    SetTiling,
    Workspace(String),
    MoveToWorkspace { workspace: String, follow: bool },
    RecentWorkspace,
    FocusMonitor(Direction),
    MoveToMonitor(Direction),
    MoveWorkspace(Direction),
    Pause,
    Reload,
    Restart,
    Exit,
    Restore,
    StartGame,
    EndGame,
    CheatSheet,
    Settings,
    Overview,
    Undo,
    Mode(String),
    Exec(String),
    Ignore,
}

impl FromStr for Command {
    type Err = String;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.len() > 16_384 {
            return Err("command exceeds 16 KiB".into());
        }
        let input = input.trim();
        let (verb, args) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
        let args = args.trim();
        let workspace_name = |name: &str| {
            if name.starts_with('"')
                && let Ok(decoded) = serde_json::from_str::<String>(name)
            {
                decoded
            } else {
                name.into()
            }
        };
        let (first_arg, remaining) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
        let remaining = remaining.trim_start();
        match (verb, first_arg) {
            ("workspace", _) if !args.is_empty() => {
                return Ok(Self::Workspace(workspace_name(args)));
            }
            ("focus", "--workspace") if !remaining.is_empty() => {
                return Ok(Self::Workspace(workspace_name(remaining)));
            }
            ("move", "--workspace") if !remaining.is_empty() => {
                return Ok(Self::MoveToWorkspace {
                    workspace: workspace_name(remaining),
                    follow: false,
                });
            }
            ("move-to-workspace", _) if !args.is_empty() => {
                let (name, follow) = match args.rsplit_once(char::is_whitespace) {
                    Some((name, "--follow")) => (name.trim_end(), true),
                    _ => (args, false),
                };
                return Ok(Self::MoveToWorkspace { workspace: workspace_name(name), follow });
            }
            _ => {}
        }
        let words: Vec<_> = input.split_whitespace().collect();
        let direction = |text: &str| {
            Direction::parse(text)
                .ok_or_else(|| format!("unknown direction {text:?}; use left, right, up or down"))
        };
        let axis = |text: &str| match text {
            "width" | "--width" | "horizontal" => Ok(Axis::Horizontal),
            "height" | "--height" | "vertical" => Ok(Axis::Vertical),
            _ => Err(format!("unknown axis {text:?}; use width or height")),
        };
        match words.as_slice() {
            ["recent-workspace"] | ["focus", "--recent-workspace"] => Ok(Self::RecentWorkspace),
            ["focus", d] | ["focus", "--direction", d] => Ok(Self::Focus(direction(d)?)),
            ["move", d] | ["move", "--direction", d] => Ok(Self::Move(direction(d)?)),
            ["resize", a, amount] => {
                let amount = amount
                    .strip_suffix('%')
                    .ok_or("resize amount must be a percentage, such as +2%")?;
                let delta: f64 = amount.parse().map_err(|_| "invalid resize percentage")?;
                if !delta.is_finite() || delta.abs() > 100.0 {
                    return Err("resize must be between -100% and +100%".into());
                }
                Ok(Self::Resize { axis: axis(a)?, delta: delta / 100.0 })
            }
            ["toggle-direction" | "toggle-tiling-direction"] => Ok(Self::ToggleDirection),
            ["direction", a] => Ok(Self::Direction(axis(a)?)),
            ["close"] => Ok(Self::Close),
            ["toggle-fullscreen" | "fullscreen"] => Ok(Self::ToggleFullscreen),
            ["toggle-floating" | "float"] | ["toggle-floating", "--centered"] => {
                Ok(Self::ToggleFloating)
            }
            ["set-floating"] => Ok(Self::SetFloating),
            ["set-tiling"] => Ok(Self::SetTiling),
            ["move-workspace", "--direction", d] | ["move-workspace", d] => {
                Ok(Self::MoveWorkspace(direction(d)?))
            }
            ["focus-monitor", d] => Ok(Self::FocusMonitor(direction(d)?)),
            ["move-to-monitor", d] => Ok(Self::MoveToMonitor(direction(d)?)),
            ["pause" | "toggle-pause"] => Ok(Self::Pause),
            ["reload" | "wm-reload-config"] => Ok(Self::Reload),
            ["restart"] => Ok(Self::Restart),
            ["exit" | "quit" | "wm-exit"] => Ok(Self::Exit),
            ["game-mode", "start"] => Ok(Self::StartGame),
            ["game-mode", "end"] => Ok(Self::EndGame),
            ["restore"] => Ok(Self::Restore),
            ["cheatsheet" | "help-bindings"] => Ok(Self::CheatSheet),
            ["settings"] => Ok(Self::Settings),
            ["overview" | "window-search"] => Ok(Self::Overview),
            ["undo" | "layout-undo"] => Ok(Self::Undo),
            ["mode", name] => Ok(Self::Mode((*name).into())),
            ["ignore"] => Ok(Self::Ignore),
            ["exec" | "shell-exec", ..] if words.len() > 1 => Ok(Self::Exec(args.into())),
            _ => Err(format!("unknown command {input:?}; see moon help")),
        }
    }
}

/// Deliberate layout edits eligible for the single in-memory undo checkpoint.
impl Command {
    pub fn layout_label(&self) -> Option<&'static str> {
        Some(match self {
            Self::Move(_) => "Move window",
            Self::Resize { .. } => "Resize window",
            Self::ToggleDirection | Self::Direction(_) => "Change split direction",
            Self::ToggleFloating | Self::SetFloating | Self::SetTiling => "Change floating state",
            Self::ToggleFullscreen => "Change fullscreen state",
            Self::MoveToWorkspace { .. } => "Move window to workspace",
            Self::MoveToMonitor(_) => "Move window to monitor",
            Self::MoveWorkspace(_) => "Move workspace to monitor",
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum WindowAction {
    Focus,
    MoveToWorkspace { workspace: String, follow: bool },
    Float,
    Tile,
}
impl WindowAction {
    pub fn layout_label(&self) -> Option<&'static str> {
        match self {
            Self::Focus => None,
            Self::MoveToWorkspace { .. } => Some("Move window to workspace"),
            Self::Float | Self::Tile => Some("Change floating state"),
        }
    }
}

pub(crate) fn workspace_command(name: &str) -> String {
    if name.trim() == name && !name.starts_with('"') && !name.chars().any(char::is_control) {
        format!("workspace {name}")
    } else {
        format!("workspace {}", serde_json::Value::String(name.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_percentages_and_arguments() {
        assert!("resize width NaN%".parse::<Command>().is_err());
        assert!("resize width +101%".parse::<Command>().is_err());
        assert!("close anything".parse::<Command>().is_err());
        assert_eq!(
            "resize width -2%".parse::<Command>().unwrap(),
            Command::Resize { axis: Axis::Horizontal, delta: -0.02 }
        );
        assert_eq!(
            "exec cmd /c echo hello".parse::<Command>().unwrap(),
            Command::Exec("cmd /c echo hello".into())
        );
    }

    #[test]
    fn workspace_commands_preserve_names_with_spaces_and_escaped_text() {
        for (name, command) in [
            ("Web Browsing", "workspace Web Browsing"),
            ("Web  Browsing", "focus --workspace Web  Browsing"),
            (" Quoted \"name\" ", r#"workspace " Quoted \"name\" ""#),
        ] {
            let expected = Command::Workspace(name.into());
            assert_eq!(command.parse(), Ok(expected.clone()));
            assert_eq!(workspace_command(name).parse(), Ok(expected));
        }
        assert_eq!(
            "move-to-workspace Web Browsing --follow".parse(),
            Ok(Command::MoveToWorkspace { workspace: "Web Browsing".into(), follow: true })
        );
        assert_eq!(
            r#"move-to-workspace "Web --follow""#.parse(),
            Ok(Command::MoveToWorkspace { workspace: "Web --follow".into(), follow: false })
        );
    }
}
