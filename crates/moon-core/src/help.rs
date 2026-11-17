//! Read-only product help derived from the committed state, never a draft file.
use crate::{command::Command, world::World};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Shortcut {
    pub chord: String,
    pub commands: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Shortcuts {
    pub mode: String,
    pub paused: bool,
    pub hook_enabled: bool,
    pub bindings: Vec<Shortcut>,
    pub emergency: &'static str,
}

impl Shortcuts {
    pub fn text(&self) -> String {
        let mut text = format!(
            "Moon — shortcuts\r\nMode: {}{}\r\nConfigured shortcuts: {}\r\nEmergency restore: {}\r\n\r\n",
            self.mode,
            if self.paused { " (paused)" } else { "" },
            if self.hook_enabled { "enabled" } else { "disabled for this session" },
            self.emergency,
        );
        for binding in &self.bindings {
            text.push_str(&format!("{:<24} {}\r\n", binding.chord, binding.commands.join("; ")));
        }
        text
    }
}

/// The same resolver drives the keyboard hook and its live help.
pub fn shortcuts(world: &World, hook_enabled: bool) -> Shortcuts {
    let source = if world.mode == "default" {
        &world.config.bindings
    } else {
        world.config.modes.get(&world.mode).unwrap_or(&world.config.bindings)
    };
    let bindings = source
        .iter()
        .filter(|(_, texts)| {
            texts.iter().any(|text| {
                world.config.commands(text).is_ok_and(|commands| {
                    if world.paused {
                        commands.iter().any(|command| {
                            matches!(
                                command,
                                Command::Pause
                                    | Command::Reload
                                    | Command::Exit
                                    | Command::Restore
                                    | Command::Overview
                                    | Command::Settings
                                    | Command::CheatSheet
                            )
                        })
                    } else {
                        !commands.is_empty()
                    }
                })
            })
        })
        .map(|(chord, commands)| Shortcut { chord: chord.clone(), commands: commands.clone() })
        .collect();
    Shortcuts {
        mode: world.mode.clone(),
        paused: world.paused,
        hook_enabled,
        bindings,
        emergency: "ctrl+alt+shift+f12",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorkspaceOverview {
    pub name: String,
    pub windows: usize,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Overview {
    pub managed_windows: usize,
    pub active_workspace: Option<String>,
    pub workspaces: Vec<WorkspaceOverview>,
}

pub fn overview(world: &World) -> Overview {
    let mut counts = vec![0; world.workspaces.len()];
    for window in world.windows.values() {
        counts[window.workspace.index()] += 1;
    }
    Overview {
        managed_windows: world.windows.len(),
        active_workspace: world
            .active
            .get(&world.active_monitor)
            .map(|id| world.workspaces[id.index()].name.clone()),
        workspaces: world
            .workspaces
            .iter()
            .map(|workspace| WorkspaceOverview {
                name: workspace.name.clone(),
                windows: counts[workspace.id.index()],
                active: world.active.get(&workspace.monitor) == Some(&workspace.id),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        geometry::Rect,
        ids::{MonitorId, WindowId},
        world::{Event, Monitor, WindowInfo, reduce},
    };

    fn world(config: Config) -> World {
        World::new(
            config,
            vec![Monitor {
                id: MonitorId(1),
                name: "fixture".into(),
                bounds: Rect::new(0, 0, 1920, 1080),
                work_area: Rect::new(0, 0, 1920, 1080),
                scale: 1.0,
            }],
        )
        .unwrap()
    }

    #[test]
    fn live_shortcuts_follow_mode_pause_aliases_and_disabled_bindings() {
        let config = Config::load(
            "[bindings]\n'alt+h'=[]\n'alt+l'=['nothing']\n[aliases]\nnothing=[]\nresume=['pause']\n[modes.edit]\n'alt+p'=['resume']\n'alt+h'=['focus left']\n",
        )
        .unwrap();
        let mut world = world(config);
        let normal = shortcuts(&world, true);
        assert!(!normal.bindings.iter().any(|b| b.chord == "alt+h" || b.chord == "alt+l"));
        world = reduce(&world, Event::Command(Command::Mode("edit".into()))).unwrap().world;
        let mode = shortcuts(&world, true);
        assert_eq!(mode.mode, "edit");
        assert_eq!(mode.bindings.len(), 2);
        world = reduce(&world, Event::Command(Command::Pause)).unwrap().world;
        let paused = shortcuts(&world, true);
        assert!(paused.paused);
        assert_eq!(
            paused.bindings,
            vec![Shortcut { chord: "alt+p".into(), commands: vec!["resume".into()] }]
        );
        assert_eq!(paused.emergency, "ctrl+alt+shift+f12");
        let text = paused.text();
        assert!(text.contains("Mode: edit (paused)") && text.contains("alt+p"));
        assert!(!text.contains("alt+h") && text.contains(paused.emergency));
        world = reduce(&world, Event::Command(Command::Pause)).unwrap().world;
        assert_eq!(shortcuts(&world, true), mode);
    }

    #[test]
    fn live_help_matches_canonical_overrides_and_preserves_workspace_command_text() {
        let world = world(Config::load(
            "workspaces=[{name='Web Browsing'}]\n[bindings]\n'Shift + Alt + H'=['workspace Web Browsing']\n'alt+l'=['noop']\n[aliases]\nnoop=[]\n",
        ).unwrap());
        let help = shortcuts(&world, true);
        let chord = crate::config::parse_chord("alt+shift+h").unwrap();
        let matches = help
            .bindings
            .iter()
            .filter(|binding| crate::config::parse_chord(&binding.chord).unwrap() == chord)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].chord, "Shift + Alt + H");
        assert_eq!(matches[0].commands, ["workspace Web Browsing"]);
        assert_eq!(
            world.config.commands(&matches[0].commands[0]).unwrap(),
            [Command::Workspace("Web Browsing".into())]
        );
        assert!(!help.bindings.iter().any(|binding| binding.chord == "alt+l"));
    }

    #[test]
    fn disabled_hooks_retain_configured_bindings_but_never_claim_they_are_active() {
        let world = world(Config::default());
        let active = shortcuts(&world, true);
        let disabled = shortcuts(&world, false);
        assert!(active.hook_enabled);
        assert!(!disabled.hook_enabled);
        assert_eq!(disabled.bindings, active.bindings);
        assert_eq!(disabled.emergency, active.emergency);
        assert!(!disabled.bindings.is_empty());
        let text = disabled.text();
        assert!(text.contains("Configured shortcuts: disabled for this session"));
        assert!(text.contains(disabled.emergency));
        assert!(!text.contains("active shortcuts"));
        assert_eq!(serde_json::to_value(disabled).unwrap()["hook_enabled"], false);
    }

    #[test]
    fn overview_explains_rule_assignments_without_exposing_window_titles() {
        let config = Config::load(
            "[[rules]]\ncommands=['move-to-workspace 2']\nmatches=[{process={equals='chat'}}]\n",
        )
        .unwrap();
        let mut world = world(config);
        for (id, process) in [(1, "editor"), (2, "chat"), (3, "chat")] {
            world = reduce(
                &world,
                Event::WindowAdded(WindowInfo {
                    id: WindowId(id),
                    process_id: id as u32,
                    process: process.into(),
                    class: "fixture".into(),
                    title: "private fixture title".into(),
                    rect: Rect::new(0, 0, 600, 400),
                    monitor: MonitorId(1),
                }),
            )
            .unwrap()
            .world;
        }
        let before = overview(&world);
        assert_eq!(before.managed_windows, 3);
        assert_eq!(before.active_workspace.as_deref(), Some("1"));
        assert_eq!((before.workspaces[0].windows, before.workspaces[1].windows), (1, 2));
        assert!(!before.workspaces[1].active);
        world = reduce(&world, Event::WindowFocused(WindowId(2))).unwrap().world;
        let after = overview(&world);
        assert_eq!(after.active_workspace.as_deref(), Some("2"));
        assert!(after.workspaces[1].active);
        assert_eq!(after.workspaces[1].windows, 2);
        assert!(!serde_json::to_string(&after).unwrap().contains("private fixture title"));
    }

    #[test]
    fn empty_desktop_and_disabled_mode_still_explain_emergency_recovery() {
        let mut world = world(Config::load("[modes.quiet]\n'alt+h'=[]\n").unwrap());
        let empty = overview(&world);
        assert_eq!(empty.managed_windows, 0);
        assert_eq!(empty.active_workspace.as_deref(), Some("1"));
        assert!(empty.workspaces.iter().all(|workspace| workspace.windows == 0));
        world = reduce(&world, Event::Command(Command::Mode("quiet".into()))).unwrap().world;
        for paused in [false, true] {
            world.paused = paused;
            let help = shortcuts(&world, true);
            assert!(help.bindings.is_empty());
            assert_eq!(help.emergency, "ctrl+alt+shift+f12");
        }
    }
}
