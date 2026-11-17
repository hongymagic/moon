//! Committed desktop projection and one-step, in-memory layout recovery.
//! Neither API enumerates native windows or restores application contents.
use crate::{
    command::Command,
    config::Matcher,
    geometry::Rect,
    ids::{MonitorId, WindowId, WorkspaceId},
    world::{Event, Transition, WindowState, Workspace, World},
};
use serde::Serialize;
use std::collections::BTreeMap;

pub const MAX_CATALOG_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UndoStatus {
    pub available: bool,
    pub label: Option<String>,
    pub reason: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Catalog {
    pub generation: String,
    pub revision: String,
    pub paused: bool,
    pub session_suspended: bool,
    pub undo: UndoStatus,
    pub active_workspace: Option<String>,
    pub total_windows: usize,
    pub truncated: bool,
    pub rule_evaluation: &'static str,
    pub workspaces: Vec<CatalogWorkspace>,
    pub windows: Vec<CatalogWindow>,
}
#[derive(Debug, Serialize)]
pub struct CatalogWorkspace {
    pub name: String,
    pub monitor_id: String,
    pub bounds: Rect,
    pub active: bool,
    pub windows: usize,
}
#[derive(Debug, Serialize)]
pub struct CatalogWindow {
    pub id: String,
    pub process_id: String,
    pub process_birth: String,
    pub title: String,
    pub process: String,
    pub class: String,
    pub workspace: String,
    pub monitor_id: String,
    pub rect: Rect,
    pub state: WindowStatus,
    pub rules: Vec<RuleExplanation>,
}
#[derive(Debug, Serialize)]
pub struct WindowStatus {
    pub focused: bool,
    pub visible: bool,
    pub floating: bool,
    pub fullscreen: bool,
    pub suspended: bool,
}
#[derive(Debug, Serialize)]
pub struct RuleExplanation {
    pub index: usize,
    pub commands: Vec<String>,
    pub matches: Vec<MatchExplanation>,
}
#[derive(Debug, Serialize)]
pub struct MatchExplanation {
    pub index: usize,
    pub matched: bool,
    pub conditions: Vec<ConditionExplanation>,
}
#[derive(Debug, Serialize)]
pub struct ConditionExplanation {
    pub field: &'static str,
    pub operator: &'static str,
    pub value: String,
    pub matched: bool,
}
fn bounded(value: &str, truncated: &mut bool) -> String {
    if value.chars().count() <= 1024 {
        return value.into();
    }
    *truncated = true;
    value.chars().take(1023).chain(std::iter::once('…')).collect()
}
fn condition(
    field: &'static str,
    matcher: &Matcher,
    actual: &str,
    truncated: &mut bool,
) -> ConditionExplanation {
    let (operator, value) = if let Some(value) = &matcher.equals {
        ("equals", value)
    } else if let Some(value) = &matcher.regex {
        ("regex", value)
    } else {
        ("not-regex", matcher.not_regex.as_ref().expect("validated matcher"))
    };
    ConditionExplanation {
        field,
        operator,
        value: bounded(value, truncated),
        matched: matcher.matches(actual),
    }
}
/// Derive only from committed state and journal identities. All integer tokens
/// used by actions are decimal strings so JavaScript never rounds them.
pub fn catalog(
    world: &World,
    births: &BTreeMap<WindowId, u64>,
    generation: String,
    session_suspended: bool,
    undo: UndoStatus,
) -> Catalog {
    // Pausing suppresses effects, not the logical layout shown in the overview.
    let mut preview = world.clone();
    preview.paused = false;
    let placements: BTreeMap<_, _> =
        preview.plan().placements.into_iter().map(|p| (p.id, p)).collect();
    let mut catalog = Catalog {
        generation,
        revision: world.revision.to_string(),
        paused: world.paused,
        session_suspended,
        undo,
        active_workspace: world
            .active
            .get(&world.active_monitor)
            .map(|id| world.workspaces[id.index()].name.clone()),
        total_windows: world.windows.len(),
        truncated: false,
        rule_evaluation: "current-metadata",
        workspaces: Vec::new(),
        windows: Vec::new(),
    };
    for workspace in world.workspaces.iter().take(256) {
        let Some(monitor) = world.monitor(workspace.monitor) else { continue };
        catalog.workspaces.push(CatalogWorkspace {
            name: workspace.name.clone(),
            monitor_id: workspace.monitor.0.to_string(),
            bounds: monitor.work_area,
            active: world.active.get(&workspace.monitor) == Some(&workspace.id),
            windows: world.windows.values().filter(|w| w.workspace == workspace.id).count(),
        });
    }
    catalog.truncated |= world.workspaces.len() > catalog.workspaces.len();
    let mut bytes = serde_json::to_vec(&catalog).expect("finite committed state").len();
    for (&id, window) in &world.windows {
        let Some(&birth) = births.get(&id) else {
            catalog.truncated = true;
            continue;
        };
        let workspace = &world.workspaces[window.workspace.index()];
        let monitor = world.monitor(workspace.monitor).expect("validated workspace monitor");
        let placement = placements.get(&id);
        let rect = placement.map(|p| p.rect).unwrap_or_else(|| {
            if window.fullscreen {
                monitor.bounds
            } else if window.floating {
                window.floating_rect.clamped_within(&monitor.work_area)
            } else {
                let gap = (f64::from(
                    world.config.gaps_for(window.workspace.index(), &monitor.name).inner,
                ) * monitor.scale)
                    .round() as i32;
                workspace
                    .tree
                    .layout(world.tiling_area(window.workspace), gap)
                    .into_iter()
                    .find_map(|(window, rect)| (window == id).then_some(rect))
                    .unwrap_or(window.original_rect)
            }
        });
        let mut rules = Vec::new();
        for (index, rule) in world.config.rules.iter().enumerate() {
            if !rule
                .matches
                .iter()
                .any(|m| m.matches(&window.info.process, &window.info.class, &window.info.title))
            {
                continue;
            }
            if rules.len() == 16 {
                catalog.truncated = true;
                break;
            }
            catalog.truncated |= rule.commands.len() > 16 || rule.matches.len() > 16;
            rules.push(RuleExplanation {
                index,
                commands: rule
                    .commands
                    .iter()
                    .take(16)
                    .map(|c| bounded(c, &mut catalog.truncated))
                    .collect(),
                matches: rule
                    .matches
                    .iter()
                    .take(16)
                    .enumerate()
                    .map(|(index, m)| {
                        let mut conditions = Vec::new();
                        for (field, matcher, actual) in [
                            ("process", &m.process, &window.info.process),
                            ("class", &m.class, &window.info.class),
                            ("title", &m.title, &window.info.title),
                        ] {
                            if let Some(matcher) = matcher {
                                conditions.push(condition(
                                    field,
                                    matcher,
                                    actual,
                                    &mut catalog.truncated,
                                ));
                            }
                        }
                        MatchExplanation {
                            index,
                            matched: m.matches(
                                &window.info.process,
                                &window.info.class,
                                &window.info.title,
                            ),
                            conditions,
                        }
                    })
                    .collect(),
            });
        }
        let item = CatalogWindow {
            id: id.0.to_string(),
            process_id: window.info.process_id.to_string(),
            process_birth: birth.to_string(),
            title: bounded(&window.info.title, &mut catalog.truncated),
            process: bounded(&window.info.process, &mut catalog.truncated),
            class: bounded(&window.info.class, &mut catalog.truncated),
            workspace: workspace.name.clone(),
            monitor_id: workspace.monitor.0.to_string(),
            rect,
            state: WindowStatus {
                focused: world.focused == Some(id),
                visible: placement.is_some_and(|p| p.visible) && !session_suspended,
                floating: window.floating,
                fullscreen: window.fullscreen || world.native_fullscreen == Some((id, monitor.id)),
                suspended: window.suspended,
            },
            rules,
        };
        let size = serde_json::to_vec(&item).expect("finite catalog item").len() + 1;
        if bytes + size + 64 > MAX_CATALOG_BYTES {
            catalog.truncated = true;
            break;
        }
        bytes += size;
        catalog.windows.push(item);
    }
    catalog.truncated |= catalog.windows.len() != catalog.total_windows;
    while serde_json::to_vec(&catalog).expect("finite catalog").len() > MAX_CATALOG_BYTES {
        catalog.truncated = true;
        if catalog.windows.pop().is_none() && catalog.workspaces.pop().is_none() {
            catalog.active_workspace = None;
            break;
        }
    }
    catalog
}

#[derive(Clone, Debug, PartialEq)]
struct WindowLayout {
    workspace: WorkspaceId,
    floating: bool,
    fullscreen: bool,
    floating_rect: Rect,
}
impl From<&WindowState> for WindowLayout {
    fn from(window: &WindowState) -> Self {
        Self {
            workspace: window.workspace,
            floating: window.floating,
            fullscreen: window.fullscreen,
            floating_rect: window.floating_rect,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Layout {
    workspaces: Vec<Workspace>,
    windows: BTreeMap<WindowId, WindowLayout>,
    active: BTreeMap<MonitorId, WorkspaceId>,
    focused: Option<WindowId>,
    active_monitor: MonitorId,
    recent_workspace: Option<WorkspaceId>,
}
impl Layout {
    fn capture(world: &World) -> Self {
        Self {
            workspaces: world.workspaces.clone(),
            windows: world.windows.iter().map(|(&id, w)| (id, w.into())).collect(),
            active: world.active.clone(),
            focused: world.focused,
            active_monitor: world.active_monitor,
            recent_workspace: world.recent_workspace,
        }
    }
}
#[derive(Clone, Debug)]
struct Checkpoint {
    layout: Layout,
    label: String,
    config: crate::config::Config,
    monitors: Vec<crate::world::Monitor>,
    identities: BTreeMap<WindowId, (u32, bool)>,
    native_fullscreen: Option<(WindowId, MonitorId)>,
}
/// One nonpersistent checkpoint. Metadata/focus navigation does not erase a
/// deliberate edit; liveness, configuration and display changes do.
#[derive(Clone, Debug, Default)]
pub struct LayoutUndo {
    checkpoint: Option<Checkpoint>,
}
impl LayoutUndo {
    pub fn clear(&mut self) {
        self.checkpoint = None;
    }
    pub fn observe(&mut self, event: &Event) {
        if Self::invalidates(event) {
            self.clear();
        }
    }
    pub fn invalidates(event: &Event) -> bool {
        matches!(
            event,
            Event::WindowSuspended { .. }
                | Event::ConfigChanged(_)
                | Event::MonitorsChanged(_)
                | Event::PlacementRejected(_)
                | Event::NativeFullscreen(_)
                | Event::Command(
                    Command::Close
                        | Command::Exec(_)
                        | Command::Ignore
                        | Command::Restart
                        | Command::Restore
                        | Command::Exit
                )
        )
    }
    pub fn record(&mut self, before: &World, after: &World, label: &str) -> bool {
        let layout = Layout::capture(before);
        if layout != Layout::capture(after) {
            self.checkpoint = Some(Checkpoint {
                layout,
                label: label.into(),
                config: after.config.clone(),
                monitors: after.monitors.clone(),
                identities: after
                    .windows
                    .iter()
                    .map(|(&id, w)| (id, (w.info.process_id, w.suspended)))
                    .collect(),
                native_fullscreen: after.native_fullscreen,
            });
            return true;
        }
        false
    }
    fn compatible(&self, world: &World) -> bool {
        self.checkpoint.as_ref().is_none_or(|saved| {
            saved.config == world.config
                && saved.monitors == world.monitors
                && saved.native_fullscreen == world.native_fullscreen
                && saved.identities.len() == world.windows.len()
                && world.windows.iter().all(|(id, w)| {
                    saved.identities.get(id) == Some(&(w.info.process_id, w.suspended))
                })
        })
    }
    pub fn reconcile(&mut self, world: &World) {
        if !self.compatible(world) {
            self.clear();
        }
    }
    pub fn status(&self, world: &World, session_suspended: bool) -> UndoStatus {
        let reason = if session_suspended {
            Some("Windows session is suspended")
        } else if world.paused {
            Some("Resume Moon before undoing a layout change")
        } else if world.windows.values().any(|w| w.suspended) {
            Some("Restore suspended windows before undoing a layout change")
        } else if !self.compatible(world) {
            Some("Desktop changed; the layout cannot be undone")
        } else if self.checkpoint.is_none() {
            Some("No layout change to undo")
        } else {
            None
        };
        UndoStatus {
            available: reason.is_none(),
            label: self.checkpoint.as_ref().map(|c| c.label.clone()),
            reason: reason.map(str::to_owned),
        }
    }
    pub fn transition(&self, world: &World) -> Result<Transition, String> {
        if world.paused {
            return Err("Resume Moon before undoing a layout change".into());
        }
        if !self.compatible(world) {
            return Err("Desktop changed; the layout cannot be undone".into());
        }
        let checkpoint = self.checkpoint.as_ref().ok_or("No layout change to undo")?;
        if !checkpoint.layout.windows.keys().eq(world.windows.keys())
            || world.windows.values().any(|w| w.suspended)
        {
            return Err("Window availability changed; the layout cannot be undone".into());
        }
        let mut next = world.clone();
        next.revision = next.revision.saturating_add(1);
        next.workspaces = checkpoint.layout.workspaces.clone();
        next.active = checkpoint.layout.active.clone();
        next.focused = checkpoint.layout.focused;
        next.active_monitor = checkpoint.layout.active_monitor;
        next.recent_workspace = checkpoint.layout.recent_workspace;
        for (&id, saved) in &checkpoint.layout.windows {
            let window = next.windows.get_mut(&id).expect("matched window set");
            window.workspace = saved.workspace;
            window.floating = saved.floating;
            window.fullscreen = saved.fullscreen;
            window.floating_rect = saved.floating_rect;
            window.info.monitor = next.workspaces[saved.workspace.index()].monitor;
            if Some(id) == next.focused {
                window.last_focus = next.revision;
            }
        }
        next.check()?;
        let plan = next.plan();
        Ok(Transition { world: next, plan })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        command::WindowAction,
        config::Config,
        geometry::{Axis, Direction},
        protocol::{Request, RequestAction, WindowTarget, check_generation},
        world::{Monitor, WindowInfo, reduce},
    };
    fn fixture() -> World {
        let mut world = World::new(
            Config::default(),
            vec![Monitor {
                id: MonitorId(1),
                name: "fixture".into(),
                bounds: Rect::new(0, 0, 1920, 1080),
                work_area: Rect::new(0, 0, 1920, 1040),
                scale: 1.0,
            }],
        )
        .unwrap();
        for id in 1..=3 {
            world = reduce(
                &world,
                Event::WindowAdded(WindowInfo {
                    id: WindowId(id),
                    process_id: id as u32 + 10,
                    process: "fixture.exe".into(),
                    class: "Fixture".into(),
                    title: format!("Document {id}"),
                    rect: Rect::new(50, 60, 400, 300),
                    monitor: MonitorId(1),
                }),
            )
            .unwrap()
            .world;
        }
        world
    }
    fn action(world: &World, id: u64, action: WindowAction) -> Transition {
        reduce(world, Event::WindowAction { id: WindowId(id), action }).unwrap()
    }
    fn births(world: &World) -> BTreeMap<WindowId, u64> {
        world.windows.keys().map(|&id| (id, 134391580679147114 + id.0)).collect()
    }
    #[test]
    fn typed_actions_activate_containing_workspace_without_refocusing_unrelated_moves() {
        let before = fixture();
        let moved = action(
            &before,
            1,
            WindowAction::MoveToWorkspace { workspace: "2".into(), follow: false },
        );
        assert_eq!(moved.world.focused, before.focused);
        assert_eq!(moved.world.active, before.active);
        assert_eq!(moved.world.windows[&WindowId(1)].workspace, WorkspaceId(1));
        assert!(!moved.plan.placements.iter().find(|p| p.id == WindowId(1)).unwrap().visible);
        let focused = action(&moved.world, 1, WindowAction::Focus);
        assert_eq!(focused.world.active[&MonitorId(1)], WorkspaceId(1));
        assert_eq!(focused.plan.focus, Some(WindowId(1)));
        assert_eq!(focused.world.workspaces[1].tree, moved.world.workspaces[1].tree);
        let again = action(&focused.world, 1, WindowAction::Focus);
        assert_eq!(again.world.active, focused.world.active);
        assert_eq!(again.plan.focus, Some(WindowId(1)));
        let floating = action(&focused.world, 2, WindowAction::Float);
        assert!(floating.world.windows[&WindowId(2)].floating);
        assert_eq!(floating.world.focused, Some(WindowId(1)));
        let tiled = action(&floating.world, 2, WindowAction::Tile);
        assert!(!tiled.world.windows[&WindowId(2)].floating);
        assert_eq!(tiled.world.focused, Some(WindowId(1)));
        let followed = action(
            &tiled.world,
            2,
            WindowAction::MoveToWorkspace { workspace: "3".into(), follow: true },
        );
        assert_eq!(followed.world.active[&MonitorId(1)], WorkspaceId(2));
        assert_eq!(followed.world.focused, Some(WindowId(2)));
    }
    #[test]
    fn guarded_identity_rejects_restarts_revisions_recycled_windows_and_unsafe_numbers() {
        let world = fixture();
        let target = WindowTarget {
            id: "1".into(),
            process_id: "11".into(),
            process_birth: "134391580679147115".into(),
        };
        assert!(
            check_generation(
                "20:134391580679147114",
                &world.revision.to_string(),
                "20:134391580679147114",
                world.revision
            )
            .is_ok()
        );
        assert!(
            check_generation(
                "20:134391580679147114",
                &world.revision.to_string(),
                "21:134391580679147115",
                world.revision
            )
            .is_err()
        );
        assert!(check_generation("20:1", "2", "20:1", world.revision).is_err());
        assert_eq!(
            target.check(&world, 134391580679147115, 11, 134391580679147115),
            Ok(WindowId(1))
        );
        for (recorded, owner, birth) in [
            (9, 11, 134391580679147115),
            (134391580679147115, 12, 134391580679147115),
            (134391580679147115, 11, 9),
        ] {
            assert!(target.check(&world, recorded, owner, birth).is_err());
        }
        let base = serde_json::json!({"version":1,"id":1,"method":"window","generation":"20:134391580679147114","revision":world.revision.to_string(),"target":target,"action":{"type":"focus"}});
        let parsed = Request::parse(&base.to_string()).unwrap();
        assert!(matches!(parsed.action, RequestAction::Window { .. }));
        for (field, value) in [
            ("id", serde_json::json!(1)),
            ("process_birth", serde_json::json!(134391580679147115_u64)),
            ("process_id", serde_json::json!("4294967296")),
        ] {
            let mut bad = base.clone();
            bad["target"][field] = value;
            assert!(Request::parse(&bad.to_string()).is_err());
        }
        for bad_action in [
            serde_json::json!({"type":"exec","command":"unsafe"}),
            serde_json::json!({"type":"focus","extra":true}),
            serde_json::json!({"type":"move-to-workspace","workspace":"2"}),
        ] {
            let mut bad = base.clone();
            bad["action"] = bad_action;
            assert!(Request::parse(&bad.to_string()).is_err());
        }
        for action in ["pause", "reload", "restore"] {
            assert!(Request::parse(&serde_json::json!({"version":1,"id":1,"method":"control","generation":"20:1","revision":"3","action":action}).to_string()).is_ok());
        }
        assert!(Request::parse(r#"{"version":1,"id":1,"method":"control","generation":"20:1","revision":"3","action":"exec"}"#).is_err());
    }
    #[test]
    fn catalog_uses_planned_hidden_geometry_exact_tokens_and_current_rule_conditions() {
        let mut world = fixture();
        world = action(
            &world,
            1,
            WindowAction::MoveToWorkspace { workspace: "2".into(), follow: false },
        )
        .world;
        world.config.rules.push(crate::config::Rule {
            commands: vec!["set-floating".into()],
            matches: vec![
                crate::config::WindowMatch {
                    process: Some(Matcher {
                        equals: Some("FIXTURE.EXE".into()),
                        regex: None,
                        not_regex: None,
                    }),
                    title: Some(Matcher {
                        equals: None,
                        regex: Some("Document 1$".into()),
                        not_regex: None,
                    }),
                    class: None,
                },
                crate::config::WindowMatch {
                    class: Some(Matcher {
                        equals: Some("Other".into()),
                        regex: None,
                        not_regex: None,
                    }),
                    ..Default::default()
                },
            ],
        });
        let plan = world.plan();
        let id = WindowId(1);
        world.windows.get_mut(&id).unwrap().info.rect = Rect::new(-32000, 0, 1, 1);
        let mut undo = LayoutUndo::default();
        let result = catalog(
            &world,
            &births(&world),
            "10:134391580679147114".into(),
            false,
            undo.status(&world, false),
        );
        let item = result.windows.iter().find(|w| w.id == "1").unwrap();
        assert_eq!(item.rect, plan.placements.iter().find(|p| p.id == id).unwrap().rect);
        assert!(!item.state.visible);
        assert_eq!(item.process_birth, "134391580679147115");
        assert_eq!(item.rules.len(), 1);
        assert!(item.rules[0].matches[0].matched);
        assert!(!item.rules[0].matches[1].matched);
        assert!(item.rules[0].matches[0].conditions.iter().all(|c| c.matched));
        assert_eq!(result.workspaces[1].bounds, world.monitors[0].work_area);
        world.paused = true;
        let paused =
            catalog(&world, &births(&world), "10:1".into(), false, undo.status(&world, false));
        assert_eq!(paused.windows[0].rect, item.rect);
        assert!(!paused.undo.available);
        undo.clear();
    }
    #[test]
    fn catalog_bounds_large_metadata_and_reports_truncation() {
        let mut world = fixture();
        for window in world.windows.values_mut() {
            window.info.title = "雪\u{0001}".repeat(5000);
        }
        let result = catalog(
            &world,
            &births(&world),
            "10:1".into(),
            false,
            LayoutUndo::default().status(&world, false),
        );
        assert!(result.truncated);
        assert_eq!(result.total_windows, 3);
        assert!(result.windows.iter().all(|w| w.title.chars().count() <= 1024));
        assert!(serde_json::to_vec(&result).unwrap().len() <= MAX_CATALOG_BYTES);
    }
    #[test]
    fn undo_restores_layout_without_rewinding_application_metadata_and_has_one_step() {
        let before = fixture();
        let command = Command::MoveToWorkspace { workspace: "2".into(), follow: true };
        let changed = reduce(&before, Event::Command(command.clone())).unwrap().world;
        let mut history = LayoutUndo::default();
        history.record(&before, &changed, command.layout_label().unwrap());
        let mut current = changed.clone();
        current.windows.get_mut(&WindowId(3)).unwrap().info.title = "New document contents".into();
        current = reduce(&current, Event::Command(Command::Workspace("3".into()))).unwrap().world;
        let restored = history.transition(&current).unwrap();
        assert_eq!(Layout::capture(&restored.world), Layout::capture(&before));
        assert_eq!(restored.world.windows[&WindowId(3)].info.title, "New document contents");
        assert_eq!(restored.world.revision, current.revision + 1);
        assert_eq!(restored.plan.focus, before.focused);
        history.clear();
        assert!(!history.status(&restored.world, false).available);
        assert!(history.transition(&restored.world).is_err());
        let second = reduce(&changed, Event::Command(Command::ToggleFloating)).unwrap().world;
        history.record(&before, &changed, "Move window");
        history.record(&changed, &second, "Float window");
        assert_eq!(
            Layout::capture(&history.transition(&second).unwrap().world),
            Layout::capture(&changed)
        );
    }
    #[test]
    fn undo_covers_resize_split_float_fullscreen_and_rejects_context_changes() {
        let before = fixture();
        for command in [
            Command::Move(Direction::Left),
            Command::Resize { axis: Axis::Horizontal, delta: 0.02 },
            Command::ToggleDirection,
            Command::ToggleFloating,
            Command::ToggleFullscreen,
        ] {
            let next = reduce(&before, Event::Command(command.clone())).unwrap().world;
            let mut history = LayoutUndo::default();
            history.record(&before, &next, command.layout_label().unwrap());
            assert!(history.status(&next, false).available, "{command:?}");
            assert_eq!(
                Layout::capture(&history.transition(&next).unwrap().world),
                Layout::capture(&before)
            );
            for event in [
                Event::WindowRemoved(WindowId(1)),
                Event::WindowSuspended { id: WindowId(1), suspended: true },
                Event::ConfigChanged(Box::new(next.config.clone())),
                Event::MonitorsChanged(next.monitors.clone()),
                Event::Command(Command::Close),
                Event::Command(Command::Exec("example".into())),
            ] {
                let mut invalidated = history.clone();
                invalidated.observe(&event);
                let changed = reduce(&next, event).unwrap().world;
                invalidated.reconcile(&changed);
                assert!(!invalidated.status(&changed, false).available);
            }
            let mut removed = next.clone();
            removed.windows.get_mut(&WindowId(1)).unwrap().info.process_id = 999;
            assert!(history.transition(&removed).is_err());
            let mut config = next.clone();
            config.config.gaps.inner += 1;
            assert!(history.transition(&config).is_err());
            assert!(!history.status(&next, true).available);
        }
    }
    #[test]
    fn undo_preserves_ignored_app_events_and_roundtrips_monitor_transfers() {
        let mut before = fixture();
        let mut monitors = before.monitors.clone();
        monitors.push(Monitor {
            id: MonitorId(2),
            name: "secondary".into(),
            bounds: Rect::new(1920, 0, 1600, 900),
            work_area: Rect::new(1920, 0, 1600, 860),
            scale: 1.25,
        });
        before = reduce(&before, Event::MonitorsChanged(monitors)).unwrap().world;
        for command in
            [Command::MoveToMonitor(Direction::Right), Command::MoveWorkspace(Direction::Right)]
        {
            let changed = reduce(&before, Event::Command(command.clone())).unwrap().world;
            let mut history = LayoutUndo::default();
            history.record(&before, &changed, command.layout_label().unwrap());
            assert!(history.status(&changed, false).available);
            let ignored = Event::WindowAdded(WindowInfo {
                id: WindowId(99),
                process_id: 99,
                process: "moon-settings".into(),
                class: "Settings".into(),
                title: "Desktop overview".into(),
                rect: Rect::new(20, 20, 800, 600),
                monitor: MonitorId(1),
            });
            history.observe(&ignored);
            let after = reduce(&changed, ignored).unwrap().world;
            assert!(!after.windows.contains_key(&WindowId(99)));
            history.reconcile(&after);
            assert!(history.status(&after, false).available);
            assert_eq!(
                Layout::capture(&history.transition(&after).unwrap().world),
                Layout::capture(&before)
            );
        }
        let same_workspace = action(
            &fixture(),
            1,
            WindowAction::MoveToWorkspace { workspace: "1".into(), follow: true },
        );
        assert_eq!(same_workspace.world.focused, Some(WindowId(1)));
    }

    #[test]
    fn unavailable_target_requests_leave_the_source_world_unchanged() {
        let mut world = fixture();
        let saved = world.clone();
        assert!(
            reduce(&world, Event::WindowAction { id: WindowId(999), action: WindowAction::Focus })
                .is_err()
        );
        assert_eq!(world, saved);
        world.paused = true;
        assert!(
            reduce(&world, Event::WindowAction { id: WindowId(1), action: WindowAction::Float })
                .is_err()
        );
        world.paused = false;
        world.windows.get_mut(&WindowId(1)).unwrap().suspended = true;
        assert!(
            reduce(&world, Event::WindowAction { id: WindowId(1), action: WindowAction::Focus })
                .is_err()
        );
    }
}
