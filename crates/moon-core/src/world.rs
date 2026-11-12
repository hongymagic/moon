//! Single-writer state machine. Every OS effect is described by a Plan.
use crate::{
    command::Command,
    config::Config,
    geometry::{Axis, Direction, Edges, Rect, Size},
    ids::{MonitorId, WindowId, WorkspaceId},
    tree::Tree,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Monitor {
    pub id: MonitorId,
    pub name: String,
    pub bounds: Rect,
    pub work_area: Rect,
    pub scale: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: WindowId,
    pub process_id: u32,
    pub process: String,
    pub class: String,
    pub title: String,
    pub rect: Rect,
    pub monitor: MonitorId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub info: WindowInfo,
    pub workspace: WorkspaceId,
    pub floating: bool,
    pub fullscreen: bool,
    #[serde(default)]
    pub suspended: bool,
    pub original_rect: Rect,
    pub floating_rect: Rect,
    pub last_focus: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub monitor: MonitorId,
    pub tree: Tree,
    pub focused: Option<WindowId>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub config: Config,
    pub monitors: Vec<Monitor>,
    pub workspaces: Vec<Workspace>,
    pub windows: BTreeMap<WindowId, WindowState>,
    pub active: BTreeMap<MonitorId, WorkspaceId>,
    pub focused: Option<WindowId>,
    pub active_monitor: MonitorId,
    pub recent_workspace: Option<WorkspaceId>,
    pub paused: bool,
    #[serde(default)]
    pub native_fullscreen: Option<(WindowId, MonitorId)>,
    pub mode: String,
    pub revision: u64,
}
#[derive(Clone, Debug)]
pub enum Event {
    WindowAdded(WindowInfo),
    WindowRemoved(WindowId),
    WindowFocused(WindowId),
    WindowChanged(WindowInfo),
    WindowSuspended { id: WindowId, suspended: bool },
    NativeFullscreen(Option<(WindowId, MonitorId)>),
    PlacementRejected(WindowId),
    MonitorsChanged(Vec<Monitor>),
    Command(Command),
    ConfigChanged(Box<Config>),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub id: WindowId,
    pub rect: Rect,
    pub visible: bool,
    pub fullscreen: bool,
    pub floating: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Control {
    Reload,
    Restart,
    Exit,
    Restore,
    CheatSheet,
    Settings,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub placements: Vec<Placement>,
    pub focus: Option<WindowId>,
    pub close: Vec<WindowId>,
    pub launch: Vec<String>,
    pub control: Option<Control>,
}
#[derive(Clone, Debug)]
pub struct Transition {
    pub world: World,
    pub plan: Plan,
}

impl World {
    pub fn new(config: Config, monitors: Vec<Monitor>) -> Result<Self, String> {
        config.validate()?;
        validate_monitors(&monitors)?;
        let active_monitor = monitors[0].id;
        let mut active = BTreeMap::new();
        let workspaces = config
            .workspaces
            .iter()
            .enumerate()
            .map(|(index, configured)| {
                let monitor = configured
                    .monitor
                    .as_ref()
                    .and_then(|name| monitors.iter().find(|m| &m.name == name))
                    .unwrap_or(&monitors[index.min(monitors.len() - 1)])
                    .id;
                let id = WorkspaceId(index as u32);
                active.entry(monitor).or_insert(id);
                Workspace {
                    id,
                    name: configured.name.clone(),
                    monitor,
                    tree: Tree::new(Axis::Horizontal),
                    focused: None,
                }
            })
            .collect();
        if active.len() != monitors.len() {
            return Err("at least one workspace per connected monitor is required".into());
        }
        Ok(Self {
            config,
            monitors,
            workspaces,
            windows: BTreeMap::new(),
            active,
            focused: None,
            active_monitor,
            recent_workspace: None,
            paused: false,
            native_fullscreen: None,
            mode: "default".into(),
            revision: 0,
        })
    }
    pub fn current_workspace(&self) -> WorkspaceId {
        self.active[&self.active_monitor]
    }
    pub fn monitor(&self, id: MonitorId) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.id == id)
    }
    pub fn workspace_named(&self, name: &str) -> Result<WorkspaceId, String> {
        self.workspaces
            .iter()
            .find(|w| w.name == name)
            .map(|w| w.id)
            .ok_or_else(|| format!("unknown workspace {name:?}"))
    }
    pub fn workspace_visible(&self, id: WorkspaceId) -> bool {
        self.active.values().any(|&active| active == id)
    }
    pub fn tiling_area(&self, workspace: WorkspaceId) -> Rect {
        let ws = &self.workspaces[workspace.index()];
        let monitor = self.monitor(ws.monitor).expect("workspace monitor is live");
        let gaps = self.config.gaps_for(workspace.index(), &monitor.name);
        let outer = (f64::from(gaps.outer) * monitor.scale).round() as i32;
        // AppBar registration may already remove the bar from rcWork.
        let bar_bottom = monitor.bounds.y
            + (f64::from(self.config.bar_height_on(&monitor.name)) * monitor.scale).round() as i32;
        let bar = if self.config.bar_enabled_on(&monitor.name) {
            (bar_bottom - monitor.work_area.y).max(0)
        } else {
            0
        };
        let area = monitor.work_area;
        let left = outer.min((area.width - 1).max(0) / 2);
        let vertical = outer.min((area.height - bar - 1).max(0) / 2);
        area.inset(Edges {
            left,
            right: left,
            top: vertical + bar.min((area.height - 1).max(0)),
            bottom: vertical,
        })
    }
    pub fn plan(&self) -> Plan {
        if self.paused {
            return Plan::default();
        }
        let mut placements = Vec::with_capacity(self.windows.len());
        for workspace in &self.workspaces {
            let Some(monitor) = self.monitor(workspace.monitor) else { continue };
            let gaps = self.config.gaps_for(workspace.id.index(), &monitor.name);
            let gap = (f64::from(gaps.inner) * monitor.scale).round() as i32;
            let area = self.tiling_area(workspace.id);
            let tree = self.layout_tree(workspace);
            let layout: BTreeMap<_, _> = tree.layout(area, gap).into_iter().collect();
            for (&id, window) in
                self.windows.iter().filter(|(_, w)| w.workspace == workspace.id && !w.suspended)
            {
                let fullscreen =
                    window.fullscreen || self.native_fullscreen == Some((id, monitor.id));
                let rect = if fullscreen {
                    monitor.bounds
                } else if window.floating {
                    window.floating_rect.clamped_within(&monitor.work_area)
                } else {
                    layout[&id]
                };
                placements.push(Placement {
                    id,
                    rect,
                    visible: self.workspace_visible(workspace.id),
                    fullscreen,
                    floating: window.floating,
                });
            }
        }
        Plan { placements, focus: self.focused, ..Plan::default() }
    }
    fn layout_tree<'a>(&self, workspace: &'a Workspace) -> std::borrow::Cow<'a, Tree> {
        let suspended: Vec<_> = self
            .windows
            .iter()
            .filter(|(_, w)| w.workspace == workspace.id && w.suspended && !w.floating)
            .map(|(&id, _)| id)
            .collect();
        if suspended.is_empty() {
            return std::borrow::Cow::Borrowed(&workspace.tree);
        }
        let mut tree = workspace.tree.clone();
        for id in suspended {
            tree.remove(id);
        }
        std::borrow::Cow::Owned(tree)
    }
    pub fn check(&self) -> Result<(), String> {
        validate_monitors(&self.monitors)?;
        if self.native_fullscreen.is_some_and(|(_, monitor)| self.monitor(monitor).is_none()) {
            return Err("native fullscreen monitor is absent".into());
        }
        if self.workspaces.is_empty() || self.workspaces.len() != self.config.workspaces.len() {
            return Err("workspace/config count differs or is empty".into());
        }
        if self.recent_workspace.is_some_and(|id| id.index() >= self.workspaces.len()) {
            return Err("recent workspace is absent".into());
        }
        let mut tiled = BTreeSet::new();
        for (index, workspace) in self.workspaces.iter().enumerate() {
            if workspace.id.index() != index || self.monitor(workspace.monitor).is_none() {
                return Err("workspace identity or monitor is invalid".into());
            }
            workspace.tree.check()?;
            for id in workspace.tree.windows() {
                let state = self.windows.get(&id).ok_or("tree references absent window")?;
                if !tiled.insert(id) || state.workspace != workspace.id || state.floating {
                    return Err("window is duplicated or in wrong tree".into());
                }
            }
            if workspace
                .focused
                .is_some_and(|id| self.windows.get(&id).is_none_or(|w| w.workspace != workspace.id))
            {
                return Err("workspace focus is stale".into());
            }
        }
        for (&id, window) in &self.windows {
            if window.info.id != id
                || window.workspace.index() >= self.workspaces.len()
                || (!window.floating && !tiled.contains(&id))
            {
                return Err("window membership is invalid".into());
            }
        }
        if self.monitor(self.active_monitor).is_none()
            || self.monitors.iter().any(|m| !self.active.contains_key(&m.id))
        {
            return Err("active monitor mapping is incomplete".into());
        }
        for (&monitor, &workspace) in &self.active {
            if self.workspaces.get(workspace.index()).is_none_or(|w| w.monitor != monitor) {
                return Err("active workspace is on wrong monitor".into());
            }
        }
        if self.focused.is_some_and(|id| {
            self.windows
                .get(&id)
                .is_none_or(|w| w.suspended || !self.workspace_visible(w.workspace))
        }) {
            return Err("focus is absent or hidden".into());
        }
        Ok(())
    }
    fn set_focus(&mut self, id: Option<WindowId>) {
        let id = id.filter(|id| self.windows.get(id).is_some_and(|w| !w.suspended));
        self.focused = id;
        if let Some(id) = id
            && let Some(window) = self.windows.get_mut(&id)
        {
            window.last_focus = self.revision;
            let workspace = &mut self.workspaces[window.workspace.index()];
            workspace.focused = Some(id);
            self.active_monitor = workspace.monitor;
        }
    }
    fn fallback_focus(&self, workspace: WorkspaceId) -> Option<WindowId> {
        self.workspaces[workspace.index()]
            .focused
            .filter(|id| self.windows.get(id).is_some_and(|w| !w.suspended))
            .or_else(|| {
                self.windows
                    .iter()
                    .filter(|(_, w)| w.workspace == workspace && !w.suspended)
                    .max_by_key(|(_, w)| w.last_focus)
                    .map(|(&id, _)| id)
            })
    }
    fn switch_workspace(&mut self, id: WorkspaceId) {
        let monitor = self.workspaces[id.index()].monitor;
        let old = self.current_workspace();
        self.active.insert(monitor, id);
        self.active_monitor = monitor;
        if old != id {
            self.recent_workspace = Some(old);
        }
        self.set_focus(self.fallback_focus(id));
    }
    fn remove_window(&mut self, id: WindowId) {
        if self.native_fullscreen.is_some_and(|(window, _)| window == id) {
            self.native_fullscreen = None;
        }
        if let Some(window) = self.windows.remove(&id) {
            let workspace = &mut self.workspaces[window.workspace.index()];
            workspace.tree.remove(id);
            if workspace.focused == Some(id) {
                workspace.focused = None;
            }
            if self.focused == Some(id) {
                self.set_focus(self.fallback_focus(self.current_workspace()));
            }
        }
    }
    fn move_window_to(&mut self, id: WindowId, target: WorkspaceId, follow: bool) {
        let Some(window) = self.windows.get(&id) else { return };
        let old = window.workspace;
        if old == target {
            if follow {
                self.switch_workspace(target);
            }
            return;
        }
        let floating = window.floating;
        let from = self.monitor(self.workspaces[old.index()].monitor).unwrap().work_area;
        let to = self.monitor(self.workspaces[target.index()].monitor).unwrap().work_area;
        self.workspaces[old.index()].tree.remove(id);
        if self.workspaces[old.index()].focused == Some(id) {
            self.workspaces[old.index()].focused = None;
        }
        let anchor = self.workspaces[target.index()].focused;
        if !floating {
            self.workspaces[target.index()].tree.insert(id, anchor);
        }
        let monitor = self.workspaces[target.index()].monitor;
        let window = self.windows.get_mut(&id).unwrap();
        window.workspace = target;
        window.info.monitor = monitor;
        window.floating_rect = window.floating_rect.remapped(&from, &to);
        self.workspaces[target.index()].focused = Some(id);
        if follow {
            self.switch_workspace(target);
            self.set_focus(Some(id));
        } else if self.focused == Some(id) {
            self.set_focus(self.fallback_focus(self.current_workspace()));
        }
    }
    fn set_floating(&mut self, id: WindowId, floating: bool) {
        let Some(window) = self.windows.get(&id) else { return };
        if window.floating == floating {
            return;
        }
        let workspace = window.workspace;
        if floating {
            let placed = self
                .plan()
                .placements
                .into_iter()
                .find(|p| p.id == id)
                .map(|p| p.rect)
                .unwrap_or(window.info.rect);
            let area = self.monitor(self.workspaces[workspace.index()].monitor).unwrap().work_area;
            self.workspaces[workspace.index()].tree.remove(id);
            let window = self.windows.get_mut(&id).unwrap();
            window.floating = true;
            window.floating_rect = area.centered(Size {
                width: placed.width.min(area.width * 3 / 4),
                height: placed.height.min(area.height * 3 / 4),
            });
        } else {
            self.windows.get_mut(&id).unwrap().floating = false;
            self.workspaces[workspace.index()].tree.insert(id, None);
        }
    }
    fn neighbor_monitor(
        &self,
        from: MonitorId,
        direction: Direction,
        wrap: bool,
    ) -> Option<MonitorId> {
        let origin = self.monitor(from)?.bounds.center();
        let directional = self
            .monitors
            .iter()
            .filter(|m| m.id != from)
            .filter_map(|m| {
                let p = m.bounds.center();
                let dx = i64::from(p.x) - i64::from(origin.x);
                let dy = i64::from(p.y) - i64::from(origin.y);
                let (primary, secondary) = match direction {
                    Direction::Left => (-dx, dy),
                    Direction::Right => (dx, dy),
                    Direction::Up => (-dy, dx),
                    Direction::Down => (dy, dx),
                };
                (primary > 0).then_some((primary * primary + secondary * secondary, m.id))
            })
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, id)| id);
        directional.or_else(|| {
            if wrap && self.monitors.len() > 1 {
                let index = self.monitors.iter().position(|m| m.id == from)?;
                Some(self.monitors[(index + 1) % self.monitors.len()].id)
            } else {
                None
            }
        })
    }
    fn command(
        &mut self,
        command: Command,
        effects: &mut Plan,
        target: Option<WindowId>,
    ) -> Result<(), String> {
        let focused = target.or(self.focused);
        match command {
            Command::Focus(direction) => {
                if let Some(id) = focused {
                    let window = &self.windows[&id];
                    let tree = self.layout_tree(&self.workspaces[window.workspace.index()]);
                    let neighbor = tree.neighbor(id, direction, &|id| self.windows[&id].last_focus);
                    if let Some(next) = neighbor {
                        self.set_focus(Some(next));
                    } else if let Some(monitor) =
                        self.neighbor_monitor(self.active_monitor, direction, false)
                    {
                        self.active_monitor = monitor;
                        self.set_focus(self.fallback_focus(self.current_workspace()));
                    }
                }
            }
            Command::Move(direction) => {
                if let Some(id) = focused {
                    let window = &self.windows[&id];
                    let workspace = window.workspace;
                    if window.floating {
                        let delta = 32;
                        let (dx, dy) = match direction {
                            Direction::Left => (-delta, 0),
                            Direction::Right => (delta, 0),
                            Direction::Up => (0, -delta),
                            Direction::Down => (0, delta),
                        };
                        let area = self
                            .monitor(self.workspaces[workspace.index()].monitor)
                            .unwrap()
                            .work_area;
                        let w = self.windows.get_mut(&id).unwrap();
                        w.floating_rect = w.floating_rect.translated(dx, dy).clamped_within(&area);
                    } else {
                        let recency: BTreeMap<_, _> =
                            self.windows.iter().map(|(&id, w)| (id, w.last_focus)).collect();
                        self.workspaces[workspace.index()]
                            .tree
                            .move_window(id, direction, &|id| recency[&id]);
                    }
                }
            }
            Command::Resize { axis, delta } => {
                if let Some(id) = focused {
                    if !delta.is_finite() || delta.abs() > 1.0 {
                        return Err("resize delta must be finite and between -1 and 1".into());
                    }
                    let window = &self.windows[&id];
                    let workspace = window.workspace;
                    if window.floating {
                        let area = self
                            .monitor(self.workspaces[workspace.index()].monitor)
                            .unwrap()
                            .work_area;
                        let w = self.windows.get_mut(&id).unwrap();
                        match axis {
                            Axis::Horizontal => {
                                w.floating_rect.width +=
                                    (f64::from(area.width) * delta).round() as i32
                            }
                            Axis::Vertical => {
                                w.floating_rect.height +=
                                    (f64::from(area.height) * delta).round() as i32
                            }
                        };
                        w.floating_rect = w.floating_rect.clamped_within(&area);
                    } else {
                        self.workspaces[workspace.index()].tree.resize(id, axis, delta, None);
                    }
                }
            }
            Command::ToggleDirection => {
                let ws = self.current_workspace();
                self.workspaces[ws.index()].tree.toggle_direction(focused);
            }
            Command::Direction(axis) => {
                let ws = self.current_workspace();
                self.workspaces[ws.index()].tree.set_direction(focused, axis);
            }
            Command::Close => effects.close.extend(focused),
            Command::ToggleFullscreen => {
                if let Some(window) = focused.and_then(|id| self.windows.get_mut(&id)) {
                    window.fullscreen = !window.fullscreen;
                }
            }
            Command::ToggleFloating => {
                if let Some(id) = focused {
                    self.set_floating(id, !self.windows[&id].floating);
                }
            }
            Command::SetFloating => {
                if let Some(id) = focused {
                    self.set_floating(id, true);
                }
            }
            Command::SetTiling => {
                if let Some(id) = focused {
                    self.set_floating(id, false);
                }
            }
            Command::Workspace(name) => {
                let id = self.workspace_named(&name)?;
                self.switch_workspace(id);
            }
            Command::MoveToWorkspace { workspace, follow } => {
                let target = self.workspace_named(&workspace)?;
                if let Some(id) = focused {
                    self.move_window_to(id, target, follow);
                }
            }
            Command::RecentWorkspace => {
                if let Some(id) = self.recent_workspace {
                    self.switch_workspace(id);
                }
            }
            Command::FocusMonitor(direction) => {
                if let Some(monitor) = self.neighbor_monitor(self.active_monitor, direction, false)
                {
                    self.active_monitor = monitor;
                    self.set_focus(self.fallback_focus(self.current_workspace()));
                }
            }
            Command::MoveToMonitor(direction) => {
                if let Some(id) = focused
                    && let Some(monitor) = self.neighbor_monitor(
                        self.workspaces[self.windows[&id].workspace.index()].monitor,
                        direction,
                        false,
                    )
                {
                    self.move_window_to(id, self.active[&monitor], true);
                }
            }
            Command::MoveWorkspace(direction) => {
                if let Some(monitor) = self.neighbor_monitor(self.active_monitor, direction, true) {
                    let source = self.active_monitor;
                    let ws = self.current_workspace();
                    let other = self.active[&monitor];
                    let from = self.monitor(source).unwrap().work_area;
                    let to = self.monitor(monitor).unwrap().work_area;
                    self.workspaces[ws.index()].monitor = monitor;
                    self.workspaces[other.index()].monitor = source;
                    self.active.insert(monitor, ws);
                    self.active.insert(source, other);
                    self.active_monitor = monitor;
                    for window in self.windows.values_mut() {
                        if window.workspace == ws {
                            window.info.monitor = monitor;
                            window.floating_rect = window.floating_rect.remapped(&from, &to);
                        } else if window.workspace == other {
                            window.info.monitor = source;
                            window.floating_rect = window.floating_rect.remapped(&to, &from);
                        }
                    }
                }
            }
            Command::Pause => self.paused = !self.paused,
            Command::Mode(mode) => {
                if mode != "default" && !self.config.modes.contains_key(&mode) {
                    return Err(format!("unknown binding mode {mode:?}"));
                }
                self.mode = mode;
            }
            Command::Exec(command) => effects.launch.push(command),
            Command::Ignore => {
                if let Some(id) = focused {
                    self.remove_window(id);
                }
            }
            Command::Reload => effects.control = Some(Control::Reload),
            Command::Restart => effects.control = Some(Control::Restart),
            Command::Exit => effects.control = Some(Control::Exit),
            Command::Restore => effects.control = Some(Control::Restore),
            Command::CheatSheet => effects.control = Some(Control::CheatSheet),
            Command::Settings => effects.control = Some(Control::Settings),
        }
        Ok(())
    }
}

pub fn reduce(world: &World, event: Event) -> Result<Transition, String> {
    let focus_requested = matches!(
        &event,
        Event::Command(
            Command::Focus(_)
                | Command::Workspace(_)
                | Command::RecentWorkspace
                | Command::FocusMonitor(_)
                | Command::MoveToMonitor(_)
                | Command::MoveWorkspace(_)
                | Command::MoveToWorkspace { follow: true, .. }
        )
    );
    let mut next = world.clone();
    next.revision = next.revision.saturating_add(1);
    let mut effects = Plan::default();
    match event {
        Event::WindowAdded(info) => {
            if next.windows.contains_key(&info.id) {
                return reduce(world, Event::WindowChanged(info));
            }
            let mut commands = Vec::new();
            for rule in &next.config.rules {
                if rule.matches.iter().any(|m| m.matches(&info.process, &info.class, &info.title)) {
                    for command in &rule.commands {
                        commands.extend(next.config.commands(command)?);
                    }
                }
            }
            if !commands.contains(&Command::Ignore) {
                let monitor = if next.monitor(info.monitor).is_some() {
                    info.monitor
                } else {
                    next.active_monitor
                };
                let workspace = next.active[&monitor];
                let id = info.id;
                let rect = info.rect;
                next.windows.insert(
                    id,
                    WindowState {
                        info,
                        workspace,
                        floating: false,
                        fullscreen: false,
                        suspended: false,
                        original_rect: rect,
                        floating_rect: rect,
                        last_focus: next.revision,
                    },
                );
                let anchor = next.workspaces[workspace.index()].focused;
                next.workspaces[workspace.index()].tree.insert(id, anchor);
                for command in commands {
                    next.command(command, &mut effects, Some(id))?;
                }
                if let Some(window) = next.windows.get(&id)
                    && window.floating
                {
                    let area = next
                        .monitor(next.workspaces[window.workspace.index()].monitor)
                        .unwrap()
                        .work_area;
                    let size = Size {
                        width: window.original_rect.width,
                        height: window.original_rect.height,
                    };
                    next.windows.get_mut(&id).unwrap().floating_rect = area.centered(size);
                }
                if let Some(window) = next.windows.get(&id)
                    && next.workspace_visible(window.workspace)
                {
                    next.set_focus(Some(id));
                }
            }
        }
        Event::WindowRemoved(id) => next.remove_window(id),
        Event::WindowFocused(id) => {
            if let Some(window) = next.windows.get(&id)
                && !window.suspended
            {
                let workspace = window.workspace;
                if !next.workspace_visible(workspace) {
                    next.switch_workspace(workspace);
                }
                next.set_focus(Some(id));
            }
        }
        Event::WindowChanged(info) => {
            let update_rect = next.windows.get(&info.id).is_some_and(|window| {
                window.floating
                    && !window.fullscreen
                    && !window.suspended
                    && next.workspace_visible(window.workspace)
                    && next.monitors.iter().any(|m| info.rect.intersects(&m.work_area))
            });
            if let Some(window) = next.windows.get_mut(&info.id) {
                if update_rect {
                    window.floating_rect = info.rect;
                }
                window.info = info;
            }
        }
        Event::WindowSuspended { id, suspended } => {
            if let Some(window) = next.windows.get_mut(&id) {
                window.suspended = suspended;
                if suspended {
                    let workspace = &mut next.workspaces[window.workspace.index()];
                    if workspace.focused == Some(id) {
                        workspace.focused = None;
                    }
                    if next.focused == Some(id) {
                        next.set_focus(next.fallback_focus(next.current_workspace()));
                    }
                }
            }
        }
        Event::NativeFullscreen(fullscreen) => {
            next.native_fullscreen = fullscreen;
        }
        Event::PlacementRejected(id) => next.set_floating(id, true),
        Event::Command(command) => next.command(command, &mut effects, None)?,
        Event::ConfigChanged(config) => {
            config.validate()?;
            let mut rebuilt = World::new(*config, next.monitors.clone())?;
            rebuilt.revision = next.revision;
            rebuilt.paused = next.paused;
            rebuilt.native_fullscreen = next.native_fullscreen;
            for old in &next.workspaces {
                if let Ok(id) = rebuilt.workspace_named(&old.name) {
                    rebuilt.workspaces[id.index()].tree = old.tree.clone();
                    rebuilt.workspaces[id.index()].focused = old.focused;
                    if rebuilt.config.workspaces[id.index()].monitor.is_none() {
                        rebuilt.workspaces[id.index()].monitor = old.monitor;
                    }
                }
            }
            for (&id, window) in &next.windows {
                let mut window = window.clone();
                let old_name = &next.workspaces[window.workspace.index()].name;
                window.workspace = rebuilt.workspace_named(old_name).unwrap_or(WorkspaceId(0));
                window.info.monitor = rebuilt.workspaces[window.workspace.index()].monitor;
                if !window.floating
                    && !rebuilt.workspaces[window.workspace.index()].tree.contains(id)
                {
                    rebuilt.workspaces[window.workspace.index()].tree.insert(id, None);
                }
                rebuilt.windows.insert(id, window);
            }
            rebuilt.active.clear();
            for monitor in &rebuilt.monitors {
                let preferred = next
                    .active
                    .get(&monitor.id)
                    .and_then(|old| {
                        rebuilt.workspace_named(&next.workspaces[old.index()].name).ok()
                    })
                    .filter(|id| rebuilt.workspaces[id.index()].monitor == monitor.id);
                let active = preferred
                    .or_else(|| {
                        rebuilt.workspaces.iter().find(|w| w.monitor == monitor.id).map(|w| w.id)
                    })
                    .ok_or("configuration leaves a monitor without a workspace")?;
                rebuilt.active.insert(monitor.id, active);
            }
            rebuilt.active_monitor = next.active_monitor;
            rebuilt.mode =
                if next.mode == "default" || rebuilt.config.modes.contains_key(&next.mode) {
                    next.mode.clone()
                } else {
                    "default".into()
                };
            if let Some(id) = next.focused
                && rebuilt.windows.contains_key(&id)
            {
                let ws = rebuilt.windows[&id].workspace;
                rebuilt.switch_workspace(ws);
                rebuilt.set_focus(Some(id));
            }
            next = rebuilt;
        }
        Event::MonitorsChanged(monitors) => {
            validate_monitors(&monitors)?;
            if monitors.len() > next.workspaces.len() {
                return Err("more monitors than configured workspaces".into());
            }
            let previous = next.monitors.clone();
            next.monitors = monitors;
            if next.native_fullscreen.is_some_and(|(_, monitor)| next.monitor(monitor).is_none()) {
                next.native_fullscreen = None;
            }
            let fallback = next.monitors[0].id;
            next.active.retain(|id, _| next.monitors.iter().any(|m| m.id == *id));
            for workspace in &mut next.workspaces {
                if !next.monitors.iter().any(|m| m.id == workspace.monitor) {
                    workspace.monitor = fallback;
                }
            }
            for monitor in &next.monitors {
                if !next.active.contains_key(&monitor.id) {
                    let candidate = next
                        .workspaces
                        .iter()
                        .find(|w| {
                            w.monitor == monitor.id && !next.active.values().any(|id| *id == w.id)
                        })
                        .or_else(|| {
                            next.workspaces
                                .iter()
                                .find(|w| !next.active.values().any(|id| *id == w.id))
                        })
                        .ok_or("no workspace available for monitor")?
                        .id;
                    next.workspaces[candidate.index()].monitor = monitor.id;
                    next.active.insert(monitor.id, candidate);
                }
            }
            if next.monitor(next.active_monitor).is_none() {
                next.active_monitor = fallback;
            }
            for window in next.windows.values_mut() {
                let monitor = next.workspaces[window.workspace.index()].monitor;
                let to = next.monitors.iter().find(|m| m.id == monitor).unwrap().work_area;
                let from = previous
                    .iter()
                    .find(|m| m.id == window.info.monitor)
                    .map(|m| m.work_area)
                    .unwrap_or(to);
                window.floating_rect = window.floating_rect.remapped(&from, &to);
                window.info.monitor = monitor;
            }
            if next.focused.is_some_and(|id| !next.workspace_visible(next.windows[&id].workspace)) {
                next.set_focus(next.fallback_focus(next.current_workspace()));
            }
        }
    }
    next.check()?;
    let mut plan = next.plan();
    plan.close = effects.close;
    plan.launch = effects.launch;
    plan.control = effects.control;
    // Metadata/location echoes never steal focus back from a user's foreground app.
    if next.focused == world.focused && !focus_requested {
        plan.focus = None;
    }
    Ok(Transition { world: next, plan })
}
fn validate_monitors(monitors: &[Monitor]) -> Result<(), String> {
    if monitors.is_empty() {
        return Err("no connected monitors".into());
    }
    let mut ids = BTreeSet::new();
    for m in monitors {
        if !ids.insert(m.id)
            || m.bounds.is_empty()
            || m.work_area.is_empty()
            || !m.scale.is_finite()
            || m.scale <= 0.0
            || m.scale > 8.0
        {
            return Err("invalid monitor geometry, scale or duplicate id".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn monitor(id: u32, x: i32) -> Monitor {
        Monitor {
            id: MonitorId(id),
            name: format!("monitor-{id}"),
            bounds: Rect::new(x, 0, 1920, 1080),
            work_area: Rect::new(x, 0, 1920, 1080),
            scale: 1.0,
        }
    }
    fn world() -> World {
        World::new(Config::default(), vec![monitor(1, 0)]).unwrap()
    }
    fn window(id: u64) -> WindowInfo {
        WindowInfo {
            id: WindowId(id),
            process_id: id as u32,
            process: "testwin".into(),
            class: "Test".into(),
            title: format!("Test {id}"),
            rect: Rect::new(100, 100, 600, 400),
            monitor: MonitorId(1),
        }
    }
    fn apply(world: &mut World, event: Event) {
        *world = reduce(world, event).unwrap().world;
    }
    #[test]
    fn switch_move_recent_and_serialization_preserve_layout() {
        let mut w = world();
        for id in 1..=4 {
            apply(&mut w, Event::WindowAdded(window(id)));
        }
        apply(&mut w, Event::Command("move-to-workspace 2 --follow".parse().unwrap()));
        assert_eq!(w.current_workspace(), WorkspaceId(1));
        assert_eq!(w.plan().placements.iter().filter(|p| p.visible).count(), 1);
        apply(&mut w, Event::Command(Command::RecentWorkspace));
        assert_eq!(w.current_workspace(), WorkspaceId(0));
        let restored: World = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        restored.check().unwrap();
        assert_eq!(w.plan(), restored.plan());
    }
    #[test]
    fn floating_and_fullscreen_never_lose_membership() {
        let mut w = world();
        apply(&mut w, Event::WindowAdded(window(1)));
        for _ in 0..4 {
            apply(&mut w, Event::Command(Command::ToggleFloating));
            apply(&mut w, Event::Command(Command::ToggleFullscreen));
            w.check().unwrap();
        }
        assert_eq!(w.windows.len(), 1);
    }
    #[test]
    fn detach_monitor_preserves_every_window() {
        let mut w = World::new(Config::default(), vec![monitor(1, 0), monitor(2, 1920)]).unwrap();
        apply(&mut w, Event::WindowAdded(window(1)));
        apply(&mut w, Event::Command(Command::MoveToMonitor(Direction::Right)));
        apply(&mut w, Event::MonitorsChanged(vec![monitor(1, 0)]));
        w.check().unwrap();
        assert_eq!(w.windows.len(), 1);
        assert!(w.plan().placements.iter().all(|p| p.rect.x < 1920));
    }
    #[test]
    fn reload_removes_workspace_without_losing_windows() {
        let mut w = world();
        apply(&mut w, Event::WindowAdded(window(1)));
        apply(&mut w, Event::Command("move-to-workspace 10 --follow".parse().unwrap()));
        let c = Config::load("workspaces=[{name='1'}]").unwrap();
        apply(&mut w, Event::ConfigChanged(Box::new(c)));
        assert_eq!(w.windows.len(), 1);
        w.check().unwrap();
    }
    #[test]
    fn suspended_windows_keep_their_tree_slots_without_effects() {
        let mut w = world();
        for id in 1..=2 {
            apply(&mut w, Event::WindowAdded(window(id)));
        }
        let before = w.plan().placements;
        let tree = w.workspaces[0].tree.clone();
        apply(&mut w, Event::WindowSuspended { id: WindowId(2), suspended: true });
        assert_eq!(w.workspaces[0].tree, tree);
        assert_eq!(w.plan().placements.len(), 1);
        assert_eq!(w.focused, Some(WindowId(1)));
        assert_eq!(w.plan().placements[0].rect, w.tiling_area(WorkspaceId(0)));
        apply(&mut w, Event::WindowSuspended { id: WindowId(2), suspended: false });
        assert_eq!(w.plan().placements, before);
        w.check().unwrap();
    }
    #[test]
    fn explicit_workspace_focus_is_not_suppressed_as_an_echo() {
        let mut w = world();
        apply(&mut w, Event::WindowAdded(window(1)));
        let transition = reduce(&w, Event::Command(Command::Workspace("1".into()))).unwrap();
        assert_eq!(transition.plan.focus, Some(WindowId(1)));
        let transition = reduce(&w, Event::WindowChanged(window(1))).unwrap();
        assert_eq!(transition.plan.focus, None);
    }
    #[test]
    fn monitor_and_workspace_overrides_resolve_before_dpi_scaling() {
        let config = Config::load(
            r#"
            workspaces=[{name="one",gaps={inner=8,outer=4}}]
            [[monitors]]
            name="monitor-1"
            gaps={inner=20,outer=10}
            bar-height=64
        "#,
        )
        .unwrap();
        let mut screen = monitor(1, 0);
        screen.scale = 1.25;
        let mut w = World::new(config, vec![screen]).unwrap();
        for id in 1..=2 {
            apply(&mut w, Event::WindowAdded(window(id)));
        }
        let placements = w.plan().placements;
        assert_eq!(placements[0].rect, Rect::new(5, 85, 950, 990));
        assert_eq!(placements[1].rect.x, 965);
        let mut config = w.config.clone();
        config.workspaces[0].gaps = None;
        apply(&mut w, Event::ConfigChanged(Box::new(config)));
        assert_eq!(w.tiling_area(WorkspaceId(0)), Rect::new(13, 93, 1894, 974));
        let mut config = w.config.clone();
        config.monitors[0].bar_enabled = Some(false);
        apply(&mut w, Event::ConfigChanged(Box::new(config)));
        assert_eq!(w.tiling_area(WorkspaceId(0)).y, 13);
    }
    #[test]
    fn reload_applies_explicit_monitor_assignment_and_preserves_unconfigured_assignment() {
        let mut w = World::new(Config::default(), vec![monitor(1, 0), monitor(2, 1920)]).unwrap();
        apply(&mut w, Event::WindowAdded(window(1)));
        let config = Config::load(
            r#"workspaces=[{name="1",monitor="monitor-2"},{name="2",monitor="monitor-1"}]"#,
        )
        .unwrap();
        apply(&mut w, Event::ConfigChanged(Box::new(config)));
        assert_eq!(w.workspaces[0].monitor, MonitorId(2));
        assert_eq!(w.windows[&WindowId(1)].info.monitor, MonitorId(2));
        assert_eq!(w.active_monitor, MonitorId(2));
        apply(&mut w, Event::ConfigChanged(Box::default()));
        assert_eq!(w.workspaces[0].monitor, MonitorId(2));
        assert_eq!(w.workspaces[1].monitor, MonitorId(1));
        w.check().unwrap();
    }
    #[test]
    fn dialog_rules_keep_natural_size_and_assignment_does_not_steal_focus() {
        let config = Config::load(
            r##"[[rules]]
commands=["move-to-workspace 2"]
matches=[{process={equals="testwin"},class={equals="#32770"}}]
"##,
        )
        .unwrap();
        let mut w = World::new(config, vec![monitor(1, 0)]).unwrap();
        let mut info = window(1);
        info.class = "#32770".into();
        apply(&mut w, Event::WindowAdded(info));
        let placement = &w.plan().placements[0];
        assert!(placement.floating);
        assert!(!placement.visible);
        assert_eq!(placement.rect, Rect::new(660, 340, 600, 400));
        assert_eq!(w.focused, None);
        let mut ignore = window(2);
        ignore.process = "EXCEL".into();
        ignore.class = "Dialog".into();
        apply(&mut w, Event::WindowAdded(ignore));
        assert_eq!(w.windows.len(), 1);
    }
    #[test]
    fn every_default_binding_executes_against_a_live_world() {
        let mut fixture = world();
        for id in 1..=4 {
            apply(&mut fixture, Event::WindowAdded(window(id)));
        }
        for (chord, commands) in &fixture.config.bindings {
            let mut current = fixture.clone();
            for text in commands {
                for command in fixture.config.commands(text).unwrap() {
                    current = reduce(&current, Event::Command(command))
                        .unwrap_or_else(|error| panic!("{chord}: {error}"))
                        .world;
                }
            }
            current.check().unwrap();
        }
        for key in [
            "alt+h",
            "alt+l",
            "alt+j",
            "alt+k",
            "alt+left",
            "alt+right",
            "alt+up",
            "alt+down",
            "alt+f",
            "alt+shift+oem_minus",
            "alt+shift+oem_plus",
            "alt+oem_question",
            "alt+shift+space",
            "alt+tab",
            "alt+shift+tab",
            "alt+shift+r",
            "alt+shift+e",
            "alt+shift+q",
        ] {
            assert!(fixture.config.bindings.contains_key(key), "missing live binding {key}");
        }
        for n in 0..=9 {
            assert!(fixture.config.bindings.contains_key(&format!("alt+{n}")));
            assert!(fixture.config.bindings.contains_key(&format!("alt+shift+{n}")));
        }
    }
    #[test]
    fn focusing_a_hidden_workspace_restores_all_its_tiles_without_reordering() {
        let mut w = world();
        apply(&mut w, Event::WindowAdded(window(1)));
        apply(&mut w, Event::Command(Command::ToggleDirection));
        for id in 2..=3 {
            apply(&mut w, Event::WindowAdded(window(id)));
        }
        let original_tiles = w.plan().placements;
        let original_tree = w.workspaces[0].tree.snapshot();
        apply(&mut w, Event::Command(Command::Workspace("2".into())));
        for id in 4..=5 {
            apply(&mut w, Event::WindowAdded(window(id)));
        }
        let other_tree = w.workspaces[1].tree.snapshot();
        let transition = reduce(&w, Event::WindowFocused(WindowId(1))).unwrap();
        assert_eq!(transition.plan.focus, Some(WindowId(1)));
        w = transition.world;
        assert_eq!(w.current_workspace(), WorkspaceId(0));
        assert_eq!(w.recent_workspace, Some(WorkspaceId(1)));
        assert_eq!(w.workspaces[0].tree.snapshot(), original_tree);
        assert_eq!(w.workspaces[1].tree.snapshot(), other_tree);
        assert_eq!(
            w.plan().placements.into_iter().filter(|p| p.visible).collect::<Vec<_>>(),
            original_tiles
        );
        assert!(original_tiles.iter().all(|p| p.rect != w.tiling_area(WorkspaceId(0))));
        w.check().unwrap();
    }
    #[test]
    fn focusing_within_a_workspace_preserves_active_workspaces_and_tile_order() {
        let mut w = World::new(Config::default(), vec![monitor(1, 0), monitor(2, 1920)]).unwrap();
        for id in 1..=4 {
            let mut info = window(id);
            if id > 2 {
                info.monitor = MonitorId(2);
                info.rect.x += 1920;
            }
            apply(&mut w, Event::WindowAdded(info));
        }
        let active = w.active.clone();
        let recent = w.recent_workspace;
        let trees: Vec<_> = w.workspaces.iter().map(|ws| ws.tree.snapshot()).collect();
        let placements = w.plan().placements;
        for id in [3, 4, 1, 2] {
            apply(&mut w, Event::WindowFocused(WindowId(id)));
            assert_eq!(w.focused, Some(WindowId(id)));
            assert_eq!(w.active, active);
            assert_eq!(w.recent_workspace, recent);
            assert_eq!(w.plan().placements, placements);
            assert_eq!(w.workspaces.iter().map(|ws| ws.tree.snapshot()).collect::<Vec<_>>(), trees);
        }
        apply(&mut w, Event::Command(Command::Focus(Direction::Left)));
        assert_eq!(w.focused, Some(WindowId(1)));
        w.check().unwrap();
    }
    #[test]
    fn animation_reload_and_restart_preserve_resized_tiling_and_membership() {
        let mut w = world();
        for id in 1..=3 {
            apply(&mut w, Event::WindowAdded(window(id)));
        }
        let before = w.workspaces[0].tree.snapshot();
        apply(&mut w, Event::Command(Command::Resize { axis: Axis::Horizontal, delta: 0.1 }));
        let resized = w.workspaces[0].tree.snapshot();
        assert_ne!(before, resized);
        let plan = w.plan();
        for enabled in [false, true] {
            let mut config = w.config.clone();
            config.workspace_animation = enabled;
            apply(&mut w, Event::ConfigChanged(Box::new(config)));
            assert_eq!(w.workspaces[0].tree.snapshot(), resized);
            assert_eq!(w.plan(), plan);
            let restored: World =
                serde_json::from_value(serde_json::to_value(&w).unwrap()).unwrap();
            assert_eq!(restored.config.workspace_animation, enabled);
            assert_eq!(restored.plan(), plan);
            restored.check().unwrap();
        }
        let mut old = serde_json::to_value(&w).unwrap();
        old["config"].as_object_mut().unwrap().remove("workspace-animation");
        let restored: World = serde_json::from_value(old).unwrap();
        assert!(restored.config.workspace_animation);
        assert_eq!(restored.plan(), plan);
    }
    #[test]
    fn focusing_a_hidden_floating_window_preserves_fullscreen_and_tiled_siblings() {
        let mut w = world();
        for id in 1..=3 {
            apply(&mut w, Event::WindowAdded(window(id)));
        }
        apply(&mut w, Event::Command(Command::SetFloating));
        apply(&mut w, Event::Command(Command::ToggleFullscreen));
        let before = w.plan().placements;
        let tree = w.workspaces[0].tree.snapshot();
        let floating = w.windows[&WindowId(3)].floating_rect;
        apply(&mut w, Event::Command(Command::Workspace("2".into())));
        apply(&mut w, Event::WindowAdded(window(4)));
        apply(&mut w, Event::WindowFocused(WindowId(3)));
        assert_eq!(w.current_workspace(), WorkspaceId(0));
        assert_eq!(w.workspaces[0].tree.snapshot(), tree);
        assert_eq!(
            w.plan().placements.into_iter().filter(|p| p.visible).collect::<Vec<_>>(),
            before
        );
        assert_eq!(w.windows[&WindowId(3)].floating_rect, floating);
        assert!(w.windows[&WindowId(3)].floating && w.windows[&WindowId(3)].fullscreen);
        w.check().unwrap();
    }
    #[test]
    fn one_hundred_thousand_command_sequences_keep_invariants() {
        let mut w = world();
        for id in 1..=8 {
            apply(&mut w, Event::WindowAdded(window(id)));
        }
        let mut seed = 0x12a4_6789u64;
        let fixture = w;
        for sequence in 0..100_000 {
            let mut w = fixture.clone();
            // Keep one long evolution as well as independent short sequences.
            let length = if sequence == 0 { 100_000 } else { (seed % 8 + 1) as usize };
            for _ in 0..length {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let direction = Direction::ALL[(seed as usize >> 4) % 4];
                let command = match seed % 9 {
                    0 => Command::Focus(direction),
                    1 => Command::Move(direction),
                    2 => Command::Resize {
                        axis: direction.axis(),
                        delta: if seed & 32 == 0 { 0.02 } else { -0.02 },
                    },
                    3 => Command::ToggleDirection,
                    4 => Command::ToggleFloating,
                    5 => Command::ToggleFullscreen,
                    6 => Command::Workspace(((seed >> 8) % 10 + 1).to_string()),
                    7 => Command::MoveToWorkspace {
                        workspace: ((seed >> 12) % 10 + 1).to_string(),
                        follow: true,
                    },
                    _ => Command::RecentWorkspace,
                };
                apply(&mut w, Event::Command(command));
                let placements = w.plan().placements;
                for p in &placements {
                    assert!(p.rect.width > 0 && p.rect.height > 0);
                    assert!(
                        p.rect.x >= 0
                            && p.rect.y >= 0
                            && p.rect.right() <= 1920
                            && p.rect.bottom() <= 1080
                    );
                }
                for (i, p) in placements
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.visible && !p.floating && !p.fullscreen)
                {
                    for q in placements
                        .iter()
                        .skip(i + 1)
                        .filter(|q| q.visible && !q.floating && !q.fullscreen)
                    {
                        assert!(!p.rect.intersects(&q.rect));
                    }
                }
            }
        }
    }
}
