//! Bounded workspace presentation; native layout and focus do not wait for motion.
use crate::ids::WorkspaceId;

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub workspace: WorkspaceId,
    /// Horizontal positions in viewport widths.
    pub from: f64,
    pub to: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceMotion {
    pub pages: Vec<Page>,
    pub target: WorkspaceId,
}

fn eased(progress: f64) -> f64 {
    let progress = if progress.is_finite() { progress.clamp(0.0, 1.0) } else { 1.0 };
    1.0 - (1.0 - progress).powi(3)
}

impl Page {
    pub fn position(&self, progress: f64) -> f64 {
        self.from + (self.to - self.from) * eased(progress)
    }
}

impl WorkspaceMotion {
    pub fn new(from: WorkspaceId, to: WorkspaceId, direction: i32) -> Self {
        let distance = if direction < 0 { -1.0 } else { 1.0 };
        Self {
            pages: if from == to {
                vec![Page { workspace: to, from: 0.0, to: 0.0 }]
            } else {
                vec![
                    Page { workspace: from, from: 0.0, to: -distance },
                    Page { workspace: to, from: distance, to: 0.0 },
                ]
            },
            target: to,
        }
    }

    /// Keep the visible composition exactly where it is. Offscreen pages are
    /// retired, so rapid commands retain at most two visible pages and one incoming.
    pub fn retarget(&self, target: WorkspaceId, direction: i32, progress: f64) -> Self {
        let mut pages: Vec<_> = self
            .pages
            .iter()
            .filter_map(|page| {
                let position = page.position(progress);
                (position.abs() < 1.0 || page.workspace == target).then_some(Page {
                    workspace: page.workspace,
                    from: position,
                    to: position,
                })
            })
            .collect();
        let destination = if let Some(page) = pages.iter().find(|page| page.workspace == target) {
            page.from
        } else {
            let edge = pages
                .iter()
                .map(|page| page.from)
                .reduce(|a, b| if direction < 0 { a.min(b) } else { a.max(b) })
                .unwrap_or(0.0);
            let position = edge + if direction < 0 { -1.0 } else { 1.0 };
            pages.push(Page { workspace: target, from: position, to: 0.0 });
            position
        };
        for page in &mut pages {
            page.to = page.from - destination;
        }
        Self { pages, target }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapid_switch_and_reversal_preserve_visible_positions_and_finish_at_target() {
        let first = WorkspaceMotion::new(WorkspaceId(0), WorkspaceId(1), 1);
        let positions: Vec<_> = first.pages.iter().map(|page| page.position(0.25)).collect();
        let next = first.retarget(WorkspaceId(2), 1, 0.25);
        assert_eq!(next.pages.len(), 3);
        for (page, expected) in next.pages.iter().zip(positions) {
            assert_eq!(page.position(0.0), expected);
        }
        let reverse = next.retarget(WorkspaceId(1), -1, 0.2);
        for page in &reverse.pages {
            let previous = next.pages.iter().find(|old| old.workspace == page.workspace).unwrap();
            assert_eq!(page.position(0.0), previous.position(0.2));
        }
        assert_eq!(
            reverse
                .pages
                .iter()
                .find(|page| page.workspace == WorkspaceId(1))
                .unwrap()
                .position(1.0),
            0.0
        );
        assert!(
            reverse
                .pages
                .iter()
                .filter(|page| page.workspace != reverse.target)
                .all(|page| page.position(1.0).abs() >= 1.0 - 1e-12)
        );
    }

    #[test]
    fn repeated_interruptions_keep_at_most_three_pages_and_bounded_travel() {
        let mut motion = WorkspaceMotion::new(WorkspaceId(0), WorkspaceId(1), 1);
        for index in 2..10_000 {
            let target = WorkspaceId(index % 10);
            let direction = if index % 3 == 0 { -1 } else { 1 };
            motion = motion.retarget(target, direction, (index % 19) as f64 / 20.0);
            assert!(motion.pages.len() <= 3);
            assert!(
                motion
                    .pages
                    .iter()
                    .all(|page| page.from.abs() <= 2.0 && page.to.abs() <= 2.0 + 1e-12)
            );
            assert_eq!(
                motion.pages.iter().find(|page| page.workspace == target).unwrap().position(1.0),
                0.0
            );
        }
    }

    #[test]
    fn presentation_never_changes_committed_workspace_geometry_or_focus() {
        use crate::{
            command::Command,
            config::Config,
            geometry::Rect,
            ids::{MonitorId, WindowId},
            world::{Event, Monitor, WindowInfo, World, reduce},
        };
        let monitor = Monitor {
            id: MonitorId(1),
            name: "fixture".into(),
            bounds: Rect::new(0, 0, 1920, 1080),
            work_area: Rect::new(0, 0, 1920, 1080),
            scale: 1.0,
        };
        let mut world = World::new(Config::default(), vec![monitor]).unwrap();
        for index in 0..3 {
            world = reduce(&world, Event::Command(Command::Workspace((index + 1).to_string())))
                .unwrap()
                .world;
            for offset in 1..=2 {
                let id = index * 2 + offset;
                world = reduce(
                    &world,
                    Event::WindowAdded(WindowInfo {
                        id: WindowId(id),
                        process_id: id as u32,
                        process: "fixture".into(),
                        class: "fixture".into(),
                        title: String::new(),
                        rect: Rect::new(20, 20, 400, 300),
                        monitor: MonitorId(1),
                    }),
                )
                .unwrap()
                .world;
            }
        }
        let initial = world.plan().placements;
        let trees: Vec<_> =
            world.workspaces.iter().map(|workspace| workspace.tree.clone()).collect();
        let mut motion = WorkspaceMotion::new(WorkspaceId(1), WorkspaceId(2), 1);
        for index in [0, 1, 0, 2, 1] {
            let focused = WindowId(index * 2 + 1);
            world = reduce(&world, Event::WindowFocused(focused)).unwrap().world;
            motion = motion.retarget(WorkspaceId(index as u32), 1, 0.15);
            assert_eq!(world.focused, Some(focused));
            assert_eq!(world.current_workspace(), WorkspaceId(index as u32));
            let visible: Vec<_> =
                world.plan().placements.into_iter().filter(|placement| placement.visible).collect();
            assert_eq!(visible.len(), 2);
            assert!(visible.iter().all(
                |placement| placement.id.0 == index * 2 + 1 || placement.id.0 == index * 2 + 2
            ));
            for placement in visible {
                assert_eq!(
                    placement.rect,
                    initial.iter().find(|before| before.id == placement.id).unwrap().rect
                );
            }
            assert_eq!(
                world.workspaces.iter().map(|workspace| workspace.tree.clone()).collect::<Vec<_>>(),
                trees
            );
            assert_eq!(motion.target, world.current_workspace());
            world.check().unwrap();
        }
    }
}
