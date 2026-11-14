//! The manual split tree: the v1 layout engine, one tree per workspace.
//!
//! The tree is pure data. It maps windows to rectangles and never touches a
//! real window. Three invariants hold after every operation (see [`Tree::check`]):
//!
//! 1. every split except the root has at least two children;
//! 2. no split has the same axis as its parent, so the axes alternate by depth;
//! 3. the shares of a split's children are positive and sum to one.
//!
//! "Toggle direction" does not wrap a window in a single-child container, as
//! i3 and GlazeWM do. It records a hint on the leaf instead, and the split is
//! created when the next window arrives. The visible behaviour is the same and
//! invariant 1 stays simple.

use crate::geometry::{Axis, Direction, Rect};
use crate::ids::WindowId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The smallest share a resize can leave a container with.
pub const MIN_SHARE: f64 = 0.05;

const SHARE_EPSILON: f64 = 1e-6;

/// Which edge of a container a pointer resize dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The left or top edge.
    Before,
    /// The right or bottom edge.
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct NodeId(u32);

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Split { axis: Axis, children: Vec<NodeId> },
    Leaf { window: WindowId, hint: Option<Axis> },
}

#[derive(Clone, Debug, PartialEq)]
struct Node {
    parent: Option<NodeId>,
    share: f64,
    kind: Kind,
}

/// A serialisable picture of a tree, used by the journal and by `moon query`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TreeSnapshot {
    Split {
        axis: Axis,
        share: f64,
        children: Vec<TreeSnapshot>,
    },
    Window {
        id: WindowId,
        share: f64,
        #[serde(default)]
        hint: Option<Axis>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    nodes: Vec<Option<Node>>,
    free: Vec<NodeId>,
    root: NodeId,
    leaves: BTreeMap<WindowId, NodeId>,
}

impl Tree {
    pub fn new(axis: Axis) -> Self {
        let root =
            Node { parent: None, share: 1.0, kind: Kind::Split { axis, children: Vec::new() } };
        Self { nodes: vec![Some(root)], free: Vec::new(), root: NodeId(0), leaves: BTreeMap::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    pub fn contains(&self, window: WindowId) -> bool {
        self.leaves.contains_key(&window)
    }

    pub fn root_axis(&self) -> Axis {
        self.axis(self.root)
    }

    /// Windows in layout order: left to right, top to bottom, depth first.
    pub fn windows(&self) -> Vec<WindowId> {
        let mut windows = Vec::with_capacity(self.leaves.len());
        self.collect_windows(self.root, &mut windows);
        windows
    }

    /// Adds a window next to `near`, or at the end of the root when `near` is
    /// not in this tree. Honours a direction hint left by `toggle_direction`.
    pub fn insert(&mut self, window: WindowId, near: Option<WindowId>) {
        if self.contains(window) {
            return;
        }
        let anchor = near.and_then(|near| self.leaves.get(&near).copied());
        let leaf = self.new_leaf(window);
        let Some(anchor) = anchor else {
            let position = self.children(self.root).len();
            self.attach(self.root, position, leaf);
            return;
        };

        let parent = self.parent_of(anchor);
        let parent_axis = self.axis(parent);
        let hint = self.take_hint(anchor);
        let wanted = hint.unwrap_or(parent_axis);

        if wanted == parent_axis {
            let position = self.index_in_parent(anchor) + 1;
            self.attach(parent, position, leaf);
        } else if self.is_root(parent) && self.children(parent).len() == 1 {
            self.set_axis(parent, wanted);
            self.attach(parent, 1, leaf);
        } else {
            let index = self.index_in_parent(anchor);
            let share = self.node(anchor).share;
            let split = self.alloc(Node {
                parent: Some(parent),
                share,
                kind: Kind::Split { axis: wanted, children: vec![anchor, leaf] },
            });
            self.children_mut(parent)[index] = split;
            for child in [anchor, leaf] {
                let node = self.node_mut(child);
                node.parent = Some(split);
                node.share = 0.5;
            }
        }
    }

    /// Adds a window on the edge it would arrive at when travelling in
    /// `travelling`: a window moving right arrives on the left edge.
    pub fn insert_at_edge(&mut self, window: WindowId, travelling: Direction) {
        if self.contains(window) {
            return;
        }
        let axis = travelling.axis();
        let at_start = travelling.is_forward();
        let leaf = self.new_leaf(window);
        let root = self.root;

        if self.axis(root) != axis && self.children(root).len() >= 2 {
            self.wrap_root(axis, leaf, at_start);
            return;
        }
        if self.axis(root) != axis {
            self.set_axis(root, axis);
            if let Some(&only) = self.children(root).first() {
                self.fix_hint(only);
            }
        }
        let position = if at_start { 0 } else { self.children(root).len() };
        self.attach(root, position, leaf);
    }

    pub fn remove(&mut self, window: WindowId) -> bool {
        let Some(leaf) = self.leaves.remove(&window) else {
            return false;
        };
        let parent = self.parent_of(leaf);
        self.detach(leaf);
        self.release(leaf);
        self.normalize(parent);
        true
    }

    /// The axis along which the next window will be placed beside `anchor`.
    pub fn direction(&self, anchor: Option<WindowId>) -> Axis {
        match anchor.and_then(|anchor| self.leaves.get(&anchor)) {
            Some(&leaf) => self.hint(leaf).unwrap_or_else(|| self.axis(self.parent_of(leaf))),
            None => self.root_axis(),
        }
    }

    pub fn toggle_direction(&mut self, anchor: Option<WindowId>) {
        let wanted = self.direction(anchor).flipped();
        self.set_direction(anchor, wanted);
    }

    pub fn set_direction(&mut self, anchor: Option<WindowId>, wanted: Axis) {
        let Some(leaf) = anchor.and_then(|anchor| self.leaves.get(&anchor).copied()) else {
            if self.is_empty() {
                self.set_axis(self.root, wanted);
            }
            return;
        };
        let parent = self.parent_of(leaf);
        if self.is_root(parent) && self.children(parent).len() == 1 {
            self.set_axis(parent, wanted);
            self.set_hint(leaf, None);
        } else if self.axis(parent) == wanted {
            self.set_hint(leaf, None);
        } else {
            self.set_hint(leaf, Some(wanted));
        }
    }

    /// The window that directional focus reaches from `window`. When the
    /// neighbour is a split, the most recently focused window inside it wins.
    pub fn neighbor(
        &self,
        window: WindowId,
        direction: Direction,
        recency: &dyn Fn(WindowId) -> u64,
    ) -> Option<WindowId> {
        let mut node = *self.leaves.get(&window)?;
        let axis = direction.axis();
        loop {
            let parent = self.node(node).parent?;
            if self.axis(parent) == axis
                && let Some(sibling) = self.sibling(node, direction.is_forward())
            {
                return Some(self.window_of(self.most_recent_leaf(sibling, recency)));
            }
            node = parent;
        }
    }

    /// Moves a window one step in `direction`. Returns false when the window is
    /// already on that edge of the workspace, so the caller can try the next
    /// monitor.
    pub fn move_window(
        &mut self,
        window: WindowId,
        direction: Direction,
        recency: &dyn Fn(WindowId) -> u64,
    ) -> bool {
        self.move_visible_window(window, direction, &|id| Some(recency(id)))
    }

    /// Move past suspended siblings without removing their slots from the tree.
    pub(crate) fn move_visible_window(
        &mut self,
        window: WindowId,
        direction: Direction,
        recency: &dyn Fn(WindowId) -> Option<u64>,
    ) -> bool {
        let Some(&leaf) = self.leaves.get(&window) else {
            return false;
        };
        if !self.leaves.keys().any(|&id| id != window && recency(id).is_some()) {
            return false;
        }
        let axis = direction.axis();
        let forward = direction.is_forward();
        let parent = self.parent_of(leaf);

        if self.axis(parent) == axis {
            let mut following = self.sibling(leaf, forward);
            while let Some(sibling) = following {
                following = self.sibling(sibling, forward);
                let Some((anchor, _)) = self.recent_visible_leaf(sibling, recency) else {
                    continue;
                };
                if self.is_leaf(sibling) {
                    let index = self.index_in_parent(leaf);
                    let other = self.index_in_parent(sibling);
                    self.children_mut(parent).swap(index, other);
                } else {
                    let anchor_child = self.child_containing(sibling, anchor);
                    self.detach(leaf);
                    let position = self.index_in_parent(anchor_child) + 1;
                    self.attach(sibling, position, leaf);
                    self.normalize(parent);
                }
                self.fix_hint(leaf);
                return true;
            }
        } else if self.is_root(parent) && self.children(parent).len() == 1 {
            return false;
        }

        // Leave the parent: climb to the nearest ancestor that runs along the
        // movement axis and land beside the branch we came from.
        let mut via = parent;
        let host = loop {
            match self.node(via).parent {
                Some(up) if self.axis(up) == axis => break up,
                Some(up) => via = up,
                None if self.axis(via) == axis => return false,
                None => {
                    self.detach(leaf);
                    self.wrap_root(axis, leaf, !forward);
                    self.normalize(parent);
                    return true;
                }
            }
        };
        self.detach(leaf);
        let position = self.index_in_parent(via) + usize::from(forward);
        self.attach(host, position, leaf);
        self.normalize(parent);
        self.fix_hint(leaf);
        true
    }

    /// Exchanges the positions of two windows, keeping the sizes of the slots.
    pub fn swap(&mut self, first: WindowId, second: WindowId) -> bool {
        let (Some(&a), Some(&b)) = (self.leaves.get(&first), self.leaves.get(&second)) else {
            return false;
        };
        if a == b {
            return false;
        }
        self.node_mut(a).kind = Kind::Leaf { window: second, hint: None };
        self.node_mut(b).kind = Kind::Leaf { window: first, hint: None };
        self.leaves.insert(first, b);
        self.leaves.insert(second, a);
        true
    }

    /// Grows or shrinks a window along `axis` by `delta`, a fraction of the
    /// container being divided. With no `side` the difference is spread over
    /// every sibling; with a side it is traded with the neighbour on that edge.
    pub fn resize(&mut self, window: WindowId, axis: Axis, delta: f64, side: Option<Side>) -> bool {
        let Some((target, parent)) = self.resize_target(window, axis, side) else {
            return false;
        };
        if !delta.is_finite() || delta == 0.0 {
            return false;
        }
        let index = self.index_in_parent(target);
        let children = self.children(parent).to_vec();
        let mut shares: Vec<f64> = children.iter().map(|&child| self.node(child).share).collect();
        let before = shares.clone();

        match side {
            Some(side) => {
                let other = if side == Side::After { index + 1 } else { index - 1 };
                let give = (shares[other] - MIN_SHARE).max(0.0);
                let take = (shares[index] - MIN_SHARE).max(0.0);
                let transfer = delta.clamp(-take, give);
                shares[index] += transfer;
                shares[other] -= transfer;
            }
            None => {
                let others = (children.len() - 1) as f64;
                let largest = (1.0 - MIN_SHARE * others).max(shares[index]);
                let smallest = MIN_SHARE.min(shares[index]);
                shares[index] = (shares[index] + delta).clamp(smallest, largest);
                rebalance(&mut shares, index);
            }
        }

        for (&child, &share) in children.iter().zip(&shares) {
            self.node_mut(child).share = share;
        }
        shares.iter().zip(&before).any(|(new, old)| (new - old).abs() > SHARE_EPSILON)
    }

    /// Like [`Tree::resize`], with the change given in pixels for a tree laid
    /// out in `area`.
    pub fn resize_pixels(
        &mut self,
        window: WindowId,
        axis: Axis,
        delta: i32,
        side: Option<Side>,
        area: Rect,
        gap: i32,
    ) -> bool {
        let Some((_, parent)) = self.resize_target(window, axis, side) else {
            return false;
        };
        let rects = self.node_rects(area, gap);
        let Some(parent_rect) = rects[parent.0 as usize] else {
            return false;
        };
        let count = self.children(parent).len() as i32;
        let available = parent_rect.length(axis)
            - effective_gap(parent_rect.length(axis), count, gap) * (count - 1);
        if available <= 0 {
            return false;
        }
        self.resize(window, axis, f64::from(delta) / f64::from(available), side)
    }

    /// The rectangle of every window when the tree fills `area` with `gap`
    /// pixels between siblings. Rectangles never overlap and stay inside
    /// `area` as long as it has at least one pixel per window in each split.
    pub fn layout(&self, area: Rect, gap: i32) -> Vec<(WindowId, Rect)> {
        let rects = self.node_rects(area, gap);
        self.windows()
            .into_iter()
            .filter_map(|window| {
                let leaf = self.leaves[&window];
                rects[leaf.0 as usize].map(|rect| (window, rect))
            })
            .collect()
    }

    pub fn snapshot(&self) -> TreeSnapshot {
        self.snapshot_node(self.root)
    }

    /// Rebuilds a tree from a snapshot, keeping only windows `keep` accepts.
    /// A snapshot that breaks the invariants falls back to a flat tree with
    /// the same windows in the same order, so a damaged journal loses sizes
    /// but never windows.
    pub fn from_snapshot(snapshot: &TreeSnapshot, keep: &dyn Fn(WindowId) -> bool) -> Tree {
        let (axis, children) = match snapshot {
            TreeSnapshot::Split { axis, children, .. } => (*axis, children.as_slice()),
            TreeSnapshot::Window { .. } => (Axis::Horizontal, std::slice::from_ref(snapshot)),
        };
        let mut tree = Tree::new(axis);
        let root = tree.root;
        for child in children {
            tree.build_from(child, root);
        }
        let faithful = tree.check().is_ok();
        let listed = tree.windows();
        if !faithful {
            let mut ordered = Vec::new();
            collect_snapshot_windows(snapshot, &mut ordered);
            let mut flat = Tree::new(axis);
            let mut previous = None;
            for window in ordered {
                if keep(window) && !flat.contains(window) {
                    flat.insert(window, previous);
                    previous = Some(window);
                }
            }
            return flat;
        }
        for window in listed {
            if !keep(window) {
                tree.remove(window);
            }
        }
        tree
    }

    /// Verifies the structural invariants. Used by tests and by debug builds
    /// after every reducer step.
    pub fn check(&self) -> Result<(), String> {
        let root = self.node(self.root);
        if root.parent.is_some() {
            return Err("root has a parent".into());
        }
        if !matches!(root.kind, Kind::Split { .. }) {
            return Err("root is not a split".into());
        }
        let mut seen = BTreeMap::new();
        self.check_node(self.root, &mut seen)?;
        if seen != self.leaves {
            return Err(format!(
                "leaf index out of date: {} indexed, {} in tree",
                self.leaves.len(),
                seen.len()
            ));
        }
        let live = self.nodes.iter().filter(|node| node.is_some()).count();
        let reachable = self.count_nodes(self.root);
        if live != reachable {
            return Err(format!("{live} nodes allocated but {reachable} reachable"));
        }
        Ok(())
    }

    fn check_node(&self, id: NodeId, seen: &mut BTreeMap<WindowId, NodeId>) -> Result<(), String> {
        let node = self.node(id);
        match &node.kind {
            Kind::Leaf { window, hint } => {
                if seen.insert(*window, id).is_some() {
                    return Err(format!("window {window} appears twice"));
                }
                let parent = self.parent_of(id);
                if *hint == Some(self.axis(parent)) {
                    return Err(format!(
                        "window {window} carries a hint equal to its parent's axis"
                    ));
                }
                if hint.is_some() && self.is_root(parent) && self.children(parent).len() == 1 {
                    return Err(format!("window {window} is alone in the root but carries a hint"));
                }
            }
            Kind::Split { axis, children } => {
                if id != self.root && children.len() < 2 {
                    return Err(format!("non-root split has {} children", children.len()));
                }
                if id == self.root && children.len() == 1 && !self.is_leaf(children[0]) {
                    return Err("root has a single split child".into());
                }
                let mut total = 0.0;
                for &child in children {
                    let child_node = self.node(child);
                    if child_node.parent != Some(id) {
                        return Err("child has the wrong parent".into());
                    }
                    if !(child_node.share > 0.0 && child_node.share.is_finite()) {
                        return Err(format!("share {} is not positive", child_node.share));
                    }
                    if let Kind::Split { axis: child_axis, .. } = child_node.kind
                        && child_axis == *axis
                    {
                        return Err("split nested in a split of the same axis".into());
                    }
                    total += child_node.share;
                    self.check_node(child, seen)?;
                }
                if !children.is_empty() && (total - 1.0).abs() > 1e-6 {
                    return Err(format!("shares sum to {total}"));
                }
            }
        }
        Ok(())
    }

    fn count_nodes(&self, id: NodeId) -> usize {
        1 + self.children(id).iter().map(|&child| self.count_nodes(child)).sum::<usize>()
    }

    fn build_from(&mut self, snapshot: &TreeSnapshot, parent: NodeId) {
        match snapshot {
            TreeSnapshot::Window { id, share, hint } => {
                if self.leaves.contains_key(id) {
                    return;
                }
                let leaf = self.alloc(Node {
                    parent: Some(parent),
                    share: *share,
                    kind: Kind::Leaf { window: *id, hint: *hint },
                });
                self.leaves.insert(*id, leaf);
                self.children_mut(parent).push(leaf);
            }
            TreeSnapshot::Split { axis, share, children } => {
                let split = self.alloc(Node {
                    parent: Some(parent),
                    share: *share,
                    kind: Kind::Split { axis: *axis, children: Vec::new() },
                });
                self.children_mut(parent).push(split);
                for child in children {
                    self.build_from(child, split);
                }
            }
        }
    }

    fn snapshot_node(&self, id: NodeId) -> TreeSnapshot {
        let node = self.node(id);
        match &node.kind {
            Kind::Leaf { window, hint } => {
                TreeSnapshot::Window { id: *window, share: node.share, hint: *hint }
            }
            Kind::Split { axis, children } => TreeSnapshot::Split {
                axis: *axis,
                share: node.share,
                children: children.iter().map(|&child| self.snapshot_node(child)).collect(),
            },
        }
    }

    fn node_rects(&self, area: Rect, gap: i32) -> Vec<Option<Rect>> {
        let mut rects = vec![None; self.nodes.len()];
        self.place(self.root, area, gap.max(0), &mut rects);
        rects
    }

    fn place(&self, id: NodeId, rect: Rect, gap: i32, rects: &mut [Option<Rect>]) {
        rects[id.0 as usize] = Some(rect);
        let Kind::Split { axis, children } = &self.node(id).kind else {
            return;
        };
        let count = children.len() as i32;
        if count == 0 {
            return;
        }
        let length = rect.length(*axis);
        let gap = effective_gap(length, count, gap);
        let available = (length - gap * (count - 1)).max(count);
        let mut cumulative = 0.0;
        let mut previous_edge = 0;
        let mut offset = rect.start(*axis);
        for (index, &child) in children.iter().enumerate() {
            cumulative += self.node(child).share;
            let remaining = count - 1 - index as i32;
            let edge = if remaining == 0 {
                available
            } else {
                // Leave at least one pixel for this child and each later one.
                ((f64::from(available) * cumulative).round() as i32)
                    .clamp(previous_edge + 1, available - remaining)
            };
            let child_length = edge - previous_edge;
            let child_rect = match axis {
                Axis::Horizontal => Rect::new(offset, rect.y, child_length, rect.height),
                Axis::Vertical => Rect::new(rect.x, offset, rect.width, child_length),
            };
            self.place(child, child_rect, gap, rects);
            offset += child_length + gap;
            previous_edge = edge;
        }
    }

    /// Finds the container a resize along `axis` acts on: the nearest ancestor
    /// (or the window itself) whose parent divides space along that axis and,
    /// for a pointer resize, has a neighbour on the dragged side.
    fn resize_target(
        &self,
        window: WindowId,
        axis: Axis,
        side: Option<Side>,
    ) -> Option<(NodeId, NodeId)> {
        let mut node = *self.leaves.get(&window)?;
        loop {
            let parent = self.node(node).parent?;
            if self.axis(parent) == axis && self.children(parent).len() > 1 {
                let has_neighbor = match side {
                    None => true,
                    Some(side) => self.sibling(node, side == Side::After).is_some(),
                };
                if has_neighbor {
                    return Some((node, parent));
                }
            }
            node = parent;
        }
    }

    fn wrap_root(&mut self, axis: Axis, leaf: NodeId, leaf_first: bool) {
        let old_root = self.root;
        let children = if leaf_first { vec![leaf, old_root] } else { vec![old_root, leaf] };
        let new_root =
            self.alloc(Node { parent: None, share: 1.0, kind: Kind::Split { axis, children } });
        self.root = new_root;
        for child in [old_root, leaf] {
            let node = self.node_mut(child);
            node.parent = Some(new_root);
            node.share = 0.5;
        }
        self.set_hint(leaf, None);
    }

    /// Restores the invariants around a split that has just lost a child.
    fn normalize(&mut self, split: NodeId) {
        let count = self.children(split).len();
        match (self.node(split).parent, count) {
            (None, 1) => {
                let child = self.children(split)[0];
                if self.is_leaf(child) {
                    self.fix_hint(child);
                } else {
                    self.release(split);
                    let node = self.node_mut(child);
                    node.parent = None;
                    node.share = 1.0;
                    self.root = child;
                }
            }
            (Some(parent), 1) => {
                let child = self.children(split)[0];
                let share = self.node(split).share;
                let index = self.index_in_parent(split);
                self.children_mut(parent)[index] = child;
                let node = self.node_mut(child);
                node.parent = Some(parent);
                node.share = share;
                self.release(split);
                if self.is_leaf(child) {
                    self.fix_hint(child);
                } else if self.axis(child) == self.axis(parent) {
                    self.splice(child);
                }
            }
            (Some(parent), 0) => {
                self.detach(split);
                self.release(split);
                self.normalize(parent);
            }
            _ => {}
        }
    }

    /// Replaces a split with its children inside a parent of the same axis.
    fn splice(&mut self, split: NodeId) {
        let parent = self.parent_of(split);
        let index = self.index_in_parent(split);
        let share = self.node(split).share;
        let children = std::mem::take(self.children_mut(split));
        for &child in &children {
            let node = self.node_mut(child);
            node.parent = Some(parent);
            node.share *= share;
        }
        self.children_mut(parent).splice(index..=index, children);
        self.release(split);
    }

    /// Drops a hint that no longer says anything: one equal to the parent's
    /// axis, or one on a window alone in the root (which becomes the root's
    /// axis instead).
    fn fix_hint(&mut self, leaf: NodeId) {
        let Some(hint) = self.hint(leaf) else {
            return;
        };
        let parent = self.parent_of(leaf);
        if self.is_root(parent) && self.children(parent).len() == 1 {
            self.set_axis(parent, hint);
            self.set_hint(leaf, None);
        } else if self.axis(parent) == hint {
            self.set_hint(leaf, None);
        }
    }

    fn attach(&mut self, parent: NodeId, position: usize, child: NodeId) {
        let existing = self.children(parent).len() as f64;
        let scale = existing / (existing + 1.0);
        for sibling in self.children(parent).to_vec() {
            self.node_mut(sibling).share *= scale;
        }
        let node = self.node_mut(child);
        node.parent = Some(parent);
        node.share = 1.0 / (existing + 1.0);
        self.children_mut(parent).insert(position, child);
    }

    /// Unlinks a child and gives its share back to its siblings.
    fn detach(&mut self, child: NodeId) {
        let parent = self.parent_of(child);
        let index = self.index_in_parent(child);
        self.children_mut(parent).remove(index);
        let total: f64 =
            self.children(parent).iter().map(|&sibling| self.node(sibling).share).sum();
        if total > 0.0 {
            for sibling in self.children(parent).to_vec() {
                self.node_mut(sibling).share /= total;
            }
        }
        self.node_mut(child).parent = None;
    }

    fn new_leaf(&mut self, window: WindowId) -> NodeId {
        let leaf =
            self.alloc(Node { parent: None, share: 1.0, kind: Kind::Leaf { window, hint: None } });
        self.leaves.insert(window, leaf);
        leaf
    }

    fn alloc(&mut self, node: Node) -> NodeId {
        match self.free.pop() {
            Some(id) => {
                self.nodes[id.0 as usize] = Some(node);
                id
            }
            None => {
                self.nodes.push(Some(node));
                NodeId((self.nodes.len() - 1) as u32)
            }
        }
    }

    fn release(&mut self, id: NodeId) {
        self.nodes[id.0 as usize] = None;
        self.free.push(id);
    }

    fn node(&self, id: NodeId) -> &Node {
        self.nodes[id.0 as usize].as_ref().expect("tree node is live")
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.nodes[id.0 as usize].as_mut().expect("tree node is live")
    }

    fn is_root(&self, id: NodeId) -> bool {
        id == self.root
    }

    fn is_leaf(&self, id: NodeId) -> bool {
        matches!(self.node(id).kind, Kind::Leaf { .. })
    }

    fn parent_of(&self, id: NodeId) -> NodeId {
        self.node(id).parent.expect("node has a parent")
    }

    fn axis(&self, id: NodeId) -> Axis {
        match self.node(id).kind {
            Kind::Split { axis, .. } => axis,
            Kind::Leaf { .. } => unreachable!("leaves have no axis"),
        }
    }

    fn set_axis(&mut self, id: NodeId, new_axis: Axis) {
        if let Kind::Split { axis, .. } = &mut self.node_mut(id).kind {
            *axis = new_axis;
        }
    }

    fn children(&self, id: NodeId) -> &[NodeId] {
        match &self.node(id).kind {
            Kind::Split { children, .. } => children,
            Kind::Leaf { .. } => &[],
        }
    }

    fn children_mut(&mut self, id: NodeId) -> &mut Vec<NodeId> {
        match &mut self.node_mut(id).kind {
            Kind::Split { children, .. } => children,
            Kind::Leaf { .. } => unreachable!("leaves have no children"),
        }
    }

    fn index_in_parent(&self, id: NodeId) -> usize {
        let parent = self.parent_of(id);
        self.children(parent)
            .iter()
            .position(|&child| child == id)
            .expect("child is listed in its parent")
    }

    fn sibling(&self, id: NodeId, forward: bool) -> Option<NodeId> {
        let parent = self.node(id).parent?;
        let index = self.index_in_parent(id);
        let target = if forward { index.checked_add(1)? } else { index.checked_sub(1)? };
        self.children(parent).get(target).copied()
    }

    fn window_of(&self, leaf: NodeId) -> WindowId {
        match self.node(leaf).kind {
            Kind::Leaf { window, .. } => window,
            Kind::Split { .. } => unreachable!("splits hold no window"),
        }
    }

    fn hint(&self, leaf: NodeId) -> Option<Axis> {
        match self.node(leaf).kind {
            Kind::Leaf { hint, .. } => hint,
            Kind::Split { .. } => None,
        }
    }

    fn set_hint(&mut self, leaf: NodeId, new_hint: Option<Axis>) {
        if let Kind::Leaf { hint, .. } = &mut self.node_mut(leaf).kind {
            *hint = new_hint;
        }
    }

    fn take_hint(&mut self, leaf: NodeId) -> Option<Axis> {
        let hint = self.hint(leaf);
        self.set_hint(leaf, None);
        hint
    }

    fn collect_windows(&self, id: NodeId, windows: &mut Vec<WindowId>) {
        match &self.node(id).kind {
            Kind::Leaf { window, .. } => windows.push(*window),
            Kind::Split { children, .. } => {
                for &child in children {
                    self.collect_windows(child, windows);
                }
            }
        }
    }

    fn most_recent_leaf(&self, id: NodeId, recency: &dyn Fn(WindowId) -> u64) -> NodeId {
        self.recent_visible_leaf(id, &|window| Some(recency(window)))
            .expect("a live subtree has a leaf")
            .0
    }

    fn recent_visible_leaf(
        &self,
        id: NodeId,
        recency: &dyn Fn(WindowId) -> Option<u64>,
    ) -> Option<(NodeId, u64)> {
        match &self.node(id).kind {
            Kind::Leaf { window, .. } => recency(*window).map(|order| (id, order)),
            Kind::Split { children, .. } => {
                // max_by_key keeps the last tie, so visit in reverse to keep layout order.
                children
                    .iter()
                    .rev()
                    .filter_map(|&child| self.recent_visible_leaf(child, recency))
                    .max_by_key(|&(_, order)| order)
            }
        }
    }

    /// The direct child of `ancestor` whose subtree holds `descendant`.
    fn child_containing(&self, ancestor: NodeId, descendant: NodeId) -> NodeId {
        let mut node = descendant;
        loop {
            let parent = self.parent_of(node);
            if parent == ancestor {
                return node;
            }
            node = parent;
        }
    }
}

/// The gap actually used between `count` children sharing `length` pixels:
/// the configured gap, reduced when it would leave a child without a pixel.
fn effective_gap(length: i32, count: i32, gap: i32) -> i32 {
    if count <= 1 {
        return 0;
    }
    gap.min((length - count) / (count - 1)).max(0)
}

/// Scales every share except `fixed` so the total is one again, without
/// letting a share that was above the minimum fall below it.
fn rebalance(shares: &mut [f64], fixed: usize) {
    let target = 1.0 - shares[fixed];
    let mut floored = vec![false; shares.len()];
    for _ in 0..shares.len() {
        let floored_total: f64 = shares
            .iter()
            .enumerate()
            .filter(|(index, _)| floored[*index])
            .map(|(_, share)| share)
            .sum();
        let free_total: f64 = shares
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != fixed && !floored[*index])
            .map(|(_, share)| share)
            .sum();
        if free_total <= 0.0 {
            break;
        }
        let scale = (target - floored_total) / free_total;
        let mut changed = false;
        for index in 0..shares.len() {
            if index == fixed || floored[index] {
                continue;
            }
            let floor = MIN_SHARE.min(shares[index]);
            if shares[index] * scale < floor {
                shares[index] = floor;
                floored[index] = true;
                changed = true;
            }
        }
        if !changed {
            for index in 0..shares.len() {
                if index != fixed && !floored[index] {
                    shares[index] *= scale;
                }
            }
            break;
        }
    }
    let total: f64 = shares.iter().sum();
    if total > 0.0 {
        for share in shares.iter_mut() {
            *share /= total;
        }
    }
}

fn collect_snapshot_windows(snapshot: &TreeSnapshot, windows: &mut Vec<WindowId>) {
    match snapshot {
        TreeSnapshot::Window { id, .. } => windows.push(*id),
        TreeSnapshot::Split { children, .. } => {
            for child in children {
                collect_snapshot_windows(child, windows);
            }
        }
    }
}

impl Serialize for Tree {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.snapshot().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Tree {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let snapshot = TreeSnapshot::deserialize(deserializer)?;
        let tree = Self::from_snapshot(&snapshot, &|_| true);
        tree.check().map_err(serde::de::Error::custom)?;
        Ok(tree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect::new(0, 0, 1000, 600);

    fn window(id: u64) -> WindowId {
        WindowId(id)
    }

    fn no_recency(_: WindowId) -> u64 {
        0
    }

    fn tree_with(count: u64) -> Tree {
        let mut tree = Tree::new(Axis::Horizontal);
        for id in 1..=count {
            tree.insert(window(id), id.checked_sub(1).filter(|&previous| previous > 0).map(window));
        }
        tree.check().unwrap();
        tree
    }

    fn rect_of(tree: &Tree, id: u64) -> Rect {
        tree.layout(AREA, 0).into_iter().find(|(candidate, _)| *candidate == window(id)).unwrap().1
    }

    #[test]
    fn windows_split_the_root_evenly() {
        let tree = tree_with(4);
        let widths: Vec<i32> = tree.layout(AREA, 0).iter().map(|(_, rect)| rect.width).collect();
        assert_eq!(widths, vec![250, 250, 250, 250]);
    }

    #[test]
    fn gaps_sit_between_siblings_only() {
        let tree = tree_with(2);
        let layout = tree.layout(AREA, 20);
        assert_eq!(layout[0].1, Rect::new(0, 0, 490, 600));
        assert_eq!(layout[1].1, Rect::new(510, 0, 490, 600));
    }

    #[test]
    fn toggle_direction_splits_the_focused_window() {
        let mut tree = tree_with(2);
        tree.toggle_direction(Some(window(2)));
        assert_eq!(tree.direction(Some(window(2))), Axis::Vertical);
        tree.insert(window(3), Some(window(2)));
        tree.check().unwrap();
        assert_eq!(rect_of(&tree, 1), Rect::new(0, 0, 500, 600));
        assert_eq!(rect_of(&tree, 2), Rect::new(500, 0, 500, 300));
        assert_eq!(rect_of(&tree, 3), Rect::new(500, 300, 500, 300));
    }

    #[test]
    fn toggle_on_a_lone_window_turns_the_root() {
        let mut tree = tree_with(1);
        tree.toggle_direction(Some(window(1)));
        tree.insert(window(2), Some(window(1)));
        tree.check().unwrap();
        assert_eq!(tree.root_axis(), Axis::Vertical);
        assert_eq!(rect_of(&tree, 2), Rect::new(0, 300, 1000, 300));
    }

    #[test]
    fn removing_collapses_single_child_splits() {
        let mut tree = tree_with(2);
        tree.toggle_direction(Some(window(2)));
        tree.insert(window(3), Some(window(2)));
        tree.remove(window(3));
        tree.check().unwrap();
        assert_eq!(rect_of(&tree, 2), Rect::new(500, 0, 500, 600));
        tree.remove(window(1));
        tree.remove(window(2));
        tree.check().unwrap();
        assert!(tree.is_empty());
    }

    #[test]
    fn focus_descends_to_the_most_recent_window() {
        let mut tree = tree_with(2);
        tree.toggle_direction(Some(window(2)));
        tree.insert(window(3), Some(window(2)));
        let recency = |id: WindowId| if id == window(3) { 9 } else { 1 };
        assert_eq!(tree.neighbor(window(1), Direction::Right, &recency), Some(window(3)));
        assert_eq!(tree.neighbor(window(3), Direction::Up, &recency), Some(window(2)));
        assert_eq!(tree.neighbor(window(3), Direction::Left, &recency), Some(window(1)));
        assert_eq!(tree.neighbor(window(1), Direction::Left, &recency), None);
        assert_eq!(tree.neighbor(window(2), Direction::Up, &recency), None);
    }

    #[test]
    fn move_swaps_with_a_sibling_window() {
        let mut tree = tree_with(3);
        assert!(tree.move_window(window(1), Direction::Right, &no_recency));
        assert_eq!(tree.windows(), vec![window(2), window(1), window(3)]);
        assert!(!tree.move_window(window(3), Direction::Right, &no_recency));
    }

    #[test]
    fn perpendicular_move_wraps_the_root() {
        let mut tree = tree_with(3);
        assert!(tree.move_window(window(2), Direction::Up, &no_recency));
        tree.check().unwrap();
        assert_eq!(rect_of(&tree, 2), Rect::new(0, 0, 1000, 300));
        assert_eq!(rect_of(&tree, 1), Rect::new(0, 300, 500, 300));
        assert_eq!(rect_of(&tree, 3), Rect::new(500, 300, 500, 300));
    }

    #[test]
    fn move_enters_a_sibling_split() {
        let mut tree = tree_with(2);
        tree.toggle_direction(Some(window(2)));
        tree.insert(window(3), Some(window(2)));
        assert!(tree.move_window(window(1), Direction::Right, &no_recency));
        tree.check().unwrap();
        assert_eq!(tree.root_axis(), Axis::Vertical);
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn move_leaves_a_nested_split() {
        let mut tree = tree_with(2);
        tree.toggle_direction(Some(window(2)));
        tree.insert(window(3), Some(window(2)));
        assert!(tree.move_window(window(3), Direction::Right, &no_recency));
        tree.check().unwrap();
        assert_eq!(tree.windows(), vec![window(1), window(2), window(3)]);
        assert_eq!(tree.root_axis(), Axis::Horizontal);
    }

    #[test]
    fn keyboard_resize_takes_from_every_sibling() {
        let mut tree = tree_with(4);
        assert!(tree.resize(window(1), Axis::Horizontal, 0.15, None));
        tree.check().unwrap();
        let widths: Vec<i32> = tree.layout(AREA, 0).iter().map(|(_, rect)| rect.width).collect();
        assert_eq!(widths, vec![400, 200, 200, 200]);
    }

    #[test]
    fn resize_respects_the_minimum_share() {
        let mut tree = tree_with(2);
        assert!(tree.resize(window(1), Axis::Horizontal, 5.0, None));
        tree.check().unwrap();
        assert_eq!(rect_of(&tree, 2).width, 50);
        assert!(!tree.resize(window(1), Axis::Horizontal, 0.1, None));
    }

    #[test]
    fn resize_without_a_matching_split_does_nothing() {
        let mut tree = tree_with(3);
        assert!(!tree.resize(window(1), Axis::Vertical, 0.1, None));
    }

    #[test]
    fn pointer_resize_trades_with_the_dragged_side() {
        let mut tree = tree_with(3);
        assert!(tree.resize_pixels(window(2), Axis::Horizontal, 100, Some(Side::After), AREA, 0));
        tree.check().unwrap();
        let widths: Vec<i32> = tree.layout(AREA, 0).iter().map(|(_, rect)| rect.width).collect();
        assert_eq!(widths, vec![333, 434, 233]);
        assert!(!tree.resize_pixels(window(3), Axis::Horizontal, 50, Some(Side::After), AREA, 0));
    }

    #[test]
    fn edge_insert_arrives_on_the_near_side() {
        let mut tree = tree_with(2);
        tree.insert_at_edge(window(9), Direction::Right);
        assert_eq!(tree.windows(), vec![window(9), window(1), window(2)]);
        tree.insert_at_edge(window(8), Direction::Down);
        tree.check().unwrap();
        assert_eq!(tree.root_axis(), Axis::Vertical);
        assert_eq!(tree.windows()[0], window(8));
    }

    #[test]
    fn snapshot_round_trips() {
        let mut tree = tree_with(3);
        tree.toggle_direction(Some(window(3)));
        tree.insert(window(4), Some(window(3)));
        tree.resize(window(1), Axis::Horizontal, 0.1, None);
        let rebuilt = Tree::from_snapshot(&tree.snapshot(), &|_| true);
        rebuilt.check().unwrap();
        assert_eq!(rebuilt.layout(AREA, 10), tree.layout(AREA, 10));
    }

    #[test]
    fn snapshot_preserves_next_split_direction() {
        let mut tree = tree_with(2);
        tree.toggle_direction(Some(window(2)));
        let mut rebuilt = Tree::from_snapshot(&tree.snapshot(), &|_| true);
        tree.insert(window(3), Some(window(2)));
        rebuilt.insert(window(3), Some(window(2)));
        assert_eq!(rebuilt.layout(AREA, 10), tree.layout(AREA, 10));
    }

    #[test]
    fn snapshot_drops_windows_that_are_gone() {
        let mut tree = tree_with(3);
        tree.toggle_direction(Some(window(3)));
        tree.insert(window(4), Some(window(3)));
        let rebuilt = Tree::from_snapshot(&tree.snapshot(), &|id| id != window(4));
        rebuilt.check().unwrap();
        assert_eq!(rebuilt.windows(), vec![window(1), window(2), window(3)]);
    }

    #[test]
    fn damaged_snapshot_keeps_every_window() {
        let damaged = TreeSnapshot::Split {
            axis: Axis::Horizontal,
            share: 1.0,
            children: vec![
                TreeSnapshot::Window { id: window(1), share: 0.9, hint: None },
                TreeSnapshot::Split {
                    axis: Axis::Horizontal,
                    share: 0.9,
                    children: vec![TreeSnapshot::Window { id: window(2), share: 1.0, hint: None }],
                },
            ],
        };
        let rebuilt = Tree::from_snapshot(&damaged, &|_| true);
        rebuilt.check().unwrap();
        assert_eq!(rebuilt.windows(), vec![window(1), window(2)]);
    }

    #[test]
    fn layout_survives_areas_smaller_than_the_gaps() {
        let tree = tree_with(5);
        let layout = tree.layout(Rect::new(0, 0, 30, 30), 20);
        let total: i32 = layout.iter().map(|(_, rect)| rect.width).sum();
        assert!(layout.iter().all(|(_, rect)| rect.width >= 1));
        assert!(total <= 30);
    }
}
