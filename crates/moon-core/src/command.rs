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
    CheatSheet,
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
            ["workspace", name] | ["focus", "--workspace", name] => {
                Ok(Self::Workspace((*name).into()))
            }
            ["move-workspace", "--direction", d] | ["move-workspace", d] => {
                Ok(Self::MoveWorkspace(direction(d)?))
            }
            ["move-to-workspace", name] | ["move", "--workspace", name] => {
                Ok(Self::MoveToWorkspace { workspace: (*name).into(), follow: false })
            }
            ["move-to-workspace", name, "--follow"] => {
                Ok(Self::MoveToWorkspace { workspace: (*name).into(), follow: true })
            }
            ["focus-monitor", d] => Ok(Self::FocusMonitor(direction(d)?)),
            ["move-to-monitor", d] => Ok(Self::MoveToMonitor(direction(d)?)),
            ["pause" | "toggle-pause"] => Ok(Self::Pause),
            ["reload" | "wm-reload-config"] => Ok(Self::Reload),
            ["restart"] => Ok(Self::Restart),
            ["exit" | "quit" | "wm-exit"] => Ok(Self::Exit),
            ["restore"] => Ok(Self::Restore),
            ["cheatsheet" | "help-bindings"] => Ok(Self::CheatSheet),
            ["mode", name] => Ok(Self::Mode((*name).into())),
            ["ignore"] => Ok(Self::Ignore),
            ["exec" | "shell-exec", ..] if words.len() > 1 => {
                Ok(Self::Exec(input.split_once(char::is_whitespace).unwrap().1.trim().into()))
            }
            _ => Err(format!("unknown command {input:?}; see moon help")),
        }
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
}
