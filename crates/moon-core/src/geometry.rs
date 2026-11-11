//! Integer geometry in physical pixels.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

/// Distances from each edge of a rectangle, as used for gaps and insets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Edges {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

impl Edges {
    pub const fn all(value: i32) -> Self {
        Self { top: value, right: value, bottom: value, left: value }
    }

    pub fn scaled(self, factor: f64) -> Self {
        let scale = |value: i32| (f64::from(value) * factor).round() as i32;
        Self {
            top: scale(self.top),
            right: scale(self.right),
            bottom: scale(self.bottom),
            left: scale(self.left),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    pub const fn from_edges(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { x: left, y: top, width: right - left, height: bottom - top }
    }

    pub const fn right(&self) -> i32 {
        self.x + self.width
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.height
    }

    pub const fn center(&self) -> Point {
        Point::new(self.x + self.width / 2, self.y + self.height / 2)
    }

    pub const fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    pub const fn contains(&self, point: Point) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }

    pub const fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    pub fn intersection_area(&self, other: &Rect) -> i64 {
        let width = (self.right().min(other.right()) - self.x.max(other.x)).max(0);
        let height = (self.bottom().min(other.bottom()) - self.y.max(other.y)).max(0);
        i64::from(width) * i64::from(height)
    }

    /// Shrinks the rectangle by `edges`, never below a 1×1 rectangle.
    pub fn inset(&self, edges: Edges) -> Rect {
        let width = (self.width - edges.left - edges.right).max(1);
        let height = (self.height - edges.top - edges.bottom).max(1);
        Rect::new(self.x + edges.left, self.y + edges.top, width, height)
    }

    pub const fn outset(&self, amount: i32) -> Rect {
        Rect::new(
            self.x - amount,
            self.y - amount,
            self.width + 2 * amount,
            self.height + 2 * amount,
        )
    }

    pub const fn translated(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    pub const fn length(&self, axis: Axis) -> i32 {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }

    pub const fn start(&self, axis: Axis) -> i32 {
        match axis {
            Axis::Horizontal => self.x,
            Axis::Vertical => self.y,
        }
    }

    /// A rectangle of `size` centred in `self`, clamped so it never exceeds `self`.
    pub fn centered(&self, size: Size) -> Rect {
        let width = size.width.clamp(1, self.width.max(1));
        let height = size.height.clamp(1, self.height.max(1));
        Rect::new(
            self.x + (self.width - width) / 2,
            self.y + (self.height - height) / 2,
            width,
            height,
        )
    }

    /// Moves the rectangle the smallest distance that puts it inside `bounds`,
    /// shrinking it first if it is larger than `bounds`.
    pub fn clamped_within(&self, bounds: &Rect) -> Rect {
        let width = self.width.min(bounds.width).max(1);
        let height = self.height.min(bounds.height).max(1);
        let x = self.x.clamp(bounds.x, (bounds.right() - width).max(bounds.x));
        let y = self.y.clamp(bounds.y, (bounds.bottom() - height).max(bounds.y));
        Rect::new(x, y, width, height)
    }

    /// Maps the rectangle from one bounding rectangle to another, keeping its
    /// relative position and size. Used when a floating window changes monitor.
    pub fn remapped(&self, from: &Rect, to: &Rect) -> Rect {
        if from.is_empty() {
            return self.clamped_within(to);
        }
        let scale_x = f64::from(to.width) / f64::from(from.width);
        let scale_y = f64::from(to.height) / f64::from(from.height);
        let mapped = Rect::new(
            to.x + (f64::from(self.x - from.x) * scale_x).round() as i32,
            to.y + (f64::from(self.y - from.y) * scale_y).round() as i32,
            (f64::from(self.width) * scale_x).round() as i32,
            (f64::from(self.height) * scale_y).round() as i32,
        );
        mapped.clamped_within(to)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Axis {
    /// Children sit side by side, left to right.
    Horizontal,
    /// Children are stacked, top to bottom.
    Vertical,
}

impl Axis {
    pub const fn flipped(self) -> Axis {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Axis::Horizontal => "horizontal",
            Axis::Vertical => "vertical",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub const ALL: [Direction; 4] =
        [Direction::Left, Direction::Right, Direction::Up, Direction::Down];

    pub const fn axis(self) -> Axis {
        match self {
            Direction::Left | Direction::Right => Axis::Horizontal,
            Direction::Up | Direction::Down => Axis::Vertical,
        }
    }

    /// Whether the direction points towards larger coordinates.
    pub const fn is_forward(self) -> bool {
        matches!(self, Direction::Right | Direction::Down)
    }

    pub const fn opposite(self) -> Direction {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }

    pub fn parse(text: &str) -> Option<Direction> {
        match text {
            "left" => Some(Direction::Left),
            "right" => Some(Direction::Right),
            "up" => Some(Direction::Up),
            "down" => Some(Direction::Down),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inset_never_collapses() {
        let rect = Rect::new(0, 0, 10, 10);
        assert_eq!(rect.inset(Edges::all(20)), Rect::new(20, 20, 1, 1));
    }

    #[test]
    fn clamp_moves_and_shrinks() {
        let bounds = Rect::new(0, 0, 100, 100);
        assert_eq!(Rect::new(90, -5, 20, 20).clamped_within(&bounds), Rect::new(80, 0, 20, 20));
        assert_eq!(Rect::new(0, 0, 300, 50).clamped_within(&bounds), Rect::new(0, 0, 100, 50));
    }

    #[test]
    fn remap_keeps_relative_placement() {
        let from = Rect::new(0, 0, 1000, 1000);
        let to = Rect::new(1000, 0, 2000, 2000);
        assert_eq!(
            Rect::new(100, 100, 200, 200).remapped(&from, &to),
            Rect::new(1200, 200, 400, 400)
        );
    }

    #[test]
    fn contains_is_half_open() {
        let rect = Rect::new(0, 0, 10, 10);
        assert!(rect.contains(Point::new(0, 0)));
        assert!(!rect.contains(Point::new(10, 10)));
    }
}
