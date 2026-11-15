//! At most one deferred absolute workspace shortcut, with a fixed frame deadline.
//! Other actions are ordering boundaries; their meaning must never be coalesced.
use moon_core::{command::Command, config::Config};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const FRAME: Duration = Duration::from_millis(16);

pub struct Shortcut {
    pub commands: Arc<[String]>,
    pub received: Instant,
}

#[derive(Default)]
pub struct WorkspaceShortcuts {
    pending: Option<Shortcut>,
    next_dispatch: Option<Instant>,
}

pub fn is_absolute_workspace(config: &Config, commands: &[String]) -> bool {
    commands.len() == 1
        && config
            .commands(&commands[0])
            .is_ok_and(|expanded| matches!(expanded.as_slice(), [Command::Workspace(_)]))
}

impl WorkspaceShortcuts {
    pub fn submit(&mut self, shortcut: Shortcut) {
        self.pending = Some(shortcut);
    }

    pub fn take_due(&mut self, now: Instant) -> Option<Shortcut> {
        if self.next_dispatch.is_some_and(|deadline| now < deadline) {
            return None;
        }
        let pending = self.pending.take()?;
        // New input never extends this deadline: continuous input stays responsive.
        self.next_dispatch = Some(now + FRAME);
        Some(pending)
    }

    /// Flush before an unrelated command/query so it observes the prior intent.
    pub fn flush(&mut self) -> Option<Shortcut> {
        self.next_dispatch = None;
        self.pending.take()
    }

    /// Secure desktop/session changes and emergency exit invalidate deferred input.
    pub fn clear(&mut self) {
        self.pending = None;
        self.next_dispatch = None;
    }

    pub fn wait_ms(&self, now: Instant) -> Option<u32> {
        self.pending.as_ref()?;
        let remaining = self.next_dispatch.unwrap_or(now).saturating_duration_since(now);
        Some(remaining.as_micros().div_ceil(1000).min(u128::from(u32::MAX)) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moon_core::{
        geometry::Rect,
        ids::{MonitorId, WindowId, WorkspaceId},
        world::{self, Event, Monitor, WindowInfo, World},
    };

    fn shortcut(name: &str, received: Instant) -> Shortcut {
        Shortcut { commands: vec![format!("workspace {name}")].into(), received }
    }
    fn apply(world: &mut World, shortcut: Shortcut) {
        for text in shortcut.commands.iter() {
            for command in world.config.commands(text).unwrap() {
                *world = world::reduce(world, Event::Command(command)).unwrap().world;
            }
        }
    }
    fn populated_world() -> World {
        let mut world = World::new(
            Config::default(),
            vec![Monitor {
                id: MonitorId(1),
                name: "fixture".into(),
                bounds: Rect::new(0, 0, 1920, 1080),
                work_area: Rect::new(0, 0, 1920, 1080),
                scale: 1.0,
            }],
        )
        .unwrap();
        for id in 1..=2 {
            world = world::reduce(
                &world,
                Event::WindowAdded(WindowInfo {
                    id: WindowId(id),
                    process_id: id as u32,
                    process: "fixture".into(),
                    class: "fixture".into(),
                    title: String::new(),
                    rect: Rect::new(10, 10, 600, 400),
                    monitor: MonitorId(1),
                }),
            )
            .unwrap()
            .world;
            if id == 2 {
                world = world::reduce(
                    &world,
                    Event::Command(Command::MoveToWorkspace {
                        workspace: "2".into(),
                        follow: false,
                    }),
                )
                .unwrap()
                .world;
            }
        }
        world
    }

    #[test]
    fn alternating_burst_keeps_only_last_target_and_final_focus_layout() {
        let start = Instant::now();
        let mut queue = WorkspaceShortcuts::default();
        let mut world = populated_world();
        let trees: Vec<_> = world.workspaces.iter().map(|w| w.tree.clone()).collect();
        queue.submit(shortcut("2", start));
        apply(&mut world, queue.take_due(start).unwrap());
        let leading_revision = world.revision;
        assert_eq!(world.focused, Some(WindowId(2)));
        for n in 1..=128 {
            let received = start + Duration::from_micros(n * 100);
            queue.submit(shortcut(if n % 2 == 0 { "1" } else { "2" }, received));
            assert!(queue.take_due(received).is_none());
        }
        assert_eq!(queue.wait_ms(start + Duration::from_millis(15)), Some(1));
        let latest = queue.take_due(start + FRAME).unwrap();
        assert_eq!(latest.received, start + Duration::from_micros(12800));
        apply(&mut world, latest);
        assert_eq!(world.revision, leading_revision + 1);
        assert_eq!(world.current_workspace(), WorkspaceId(0));
        assert_eq!(world.focused, Some(WindowId(1)));
        assert_eq!(
            world.plan().placements.iter().filter(|p| p.visible).map(|p| p.id).collect::<Vec<_>>(),
            [WindowId(1)]
        );
        assert_eq!(world.workspaces.iter().map(|w| w.tree.clone()).collect::<Vec<_>>(), trees);
        assert!(queue.take_due(start + Duration::from_secs(2)).is_none());
        assert_eq!(queue.wait_ms(start + FRAME), None);
        world.check().unwrap();
    }

    #[test]
    fn continuous_input_does_not_postpone_dispatch_and_release_has_no_replay() {
        let start = Instant::now();
        let mut queue = WorkspaceShortcuts::default();
        let mut dispatched = Vec::new();
        for n in 0..=100 {
            let now = start + Duration::from_millis(n);
            queue.submit(shortcut(if n % 2 == 0 { "1" } else { "2" }, now));
            if let Some(input) = queue.take_due(now) {
                dispatched.push(input.received);
            }
        }
        assert_eq!(
            dispatched,
            (0..=6).map(|n| start + Duration::from_millis(n * 16)).collect::<Vec<_>>()
        );
        assert_eq!(
            queue.take_due(start + Duration::from_millis(112)).unwrap().received,
            start + Duration::from_millis(100)
        );
        assert!(queue.take_due(start + Duration::from_secs(1)).is_none());
    }

    #[test]
    fn boundaries_flush_once_and_suspension_discards_pending_before_new_input() {
        let start = Instant::now();
        let mut queue = WorkspaceShortcuts::default();
        let mut world = populated_world();
        queue.submit(shortcut("1", start));
        apply(&mut world, queue.take_due(start).unwrap());
        queue.submit(shortcut("1", start));
        queue.submit(shortcut("2", start));
        apply(&mut world, queue.flush().unwrap());
        // An unrelated move targets the final requested workspace's window,
        // never the window from a discarded request.
        world = world::reduce(
            &world,
            Event::Command(Command::MoveToWorkspace { workspace: "3".into(), follow: false }),
        )
        .unwrap()
        .world;
        assert_eq!(world.windows[&WindowId(1)].workspace, WorkspaceId(0));
        assert_eq!(world.windows[&WindowId(2)].workspace, WorkspaceId(2));
        assert!(queue.flush().is_none());
        queue.submit(shortcut("1", start));
        assert!(queue.take_due(start).is_some());
        queue.submit(shortcut("2", start));
        queue.clear();
        assert!(queue.take_due(start + FRAME).is_none());
        queue.submit(shortcut("1", start + FRAME));
        assert!(queue.take_due(start + FRAME).is_some());
    }

    #[test]
    fn pending_target_uses_updated_layout_after_animation_is_disabled() {
        let start = Instant::now();
        let mut queue = WorkspaceShortcuts::default();
        let mut world = populated_world();
        queue.submit(shortcut("2", start));
        apply(&mut world, queue.take_due(start).unwrap());
        queue.submit(shortcut("2", start + Duration::from_millis(1)));
        queue.submit(shortcut("1", start + Duration::from_millis(2)));
        let mut config = world.config.clone();
        config.workspace_animation = false;
        config.gaps.inner = 24;
        world = world::reduce(&world, Event::ConfigChanged(Box::new(config))).unwrap().world;
        apply(&mut world, queue.take_due(start + FRAME).unwrap());
        assert_eq!(world.current_workspace(), WorkspaceId(0));
        assert_eq!(world.focused, Some(WindowId(1)));
        assert!(!world.config.workspace_animation);
        assert_eq!(world.config.gaps.inner, 24);
        assert_eq!(
            world.plan().placements.iter().filter(|p| p.visible).map(|p| p.id).collect::<Vec<_>>(),
            [WindowId(1)]
        );
        assert!(queue.take_due(start + Duration::from_secs(1)).is_none());
        world.check().unwrap();
    }

    #[test]
    fn only_single_absolute_focus_commands_and_single_action_aliases_are_coalesced() {
        let config = Config::load("[aliases]\nweb=['workspace 2']\nmixed=['workspace 1','close']\nsequence=['workspace 1','workspace 2']\n").unwrap();
        for input in ["workspace 1", "focus --workspace 2", "web"] {
            assert!(is_absolute_workspace(&config, &[input.into()]), "{input}");
        }
        for input in [
            "move-to-workspace 2",
            "move-to-workspace 2 --follow",
            "recent-workspace",
            "focus left",
            "move-workspace left",
            "pause",
            "reload",
            "mode default",
            "mixed",
            "sequence",
            "invalid-command",
        ] {
            assert!(!is_absolute_workspace(&config, &[input.into()]), "{input}");
        }
        assert!(!is_absolute_workspace(&config, &["workspace 1".into(), "workspace 2".into()]));
        assert!(!is_absolute_workspace(&config, &[]));
    }
}
