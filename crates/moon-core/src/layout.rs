//! Pure extension boundary for layout engines. No platform types cross it.
use crate::{geometry::Rect, ids::WindowId, tree::Tree};
pub trait LayoutEngine {
    fn layout(&self, area: Rect, gap: i32) -> Vec<(WindowId, Rect)>;
    fn windows(&self) -> Vec<WindowId>;
}
impl LayoutEngine for Tree {
    fn layout(&self, area: Rect, gap: i32) -> Vec<(WindowId, Rect)> {
        Tree::layout(self, area, gap)
    }
    fn windows(&self) -> Vec<WindowId> {
        Tree::windows(self)
    }
}
