//! Identifier newtypes shared by every stage of the pipeline.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A top-level window. On Windows this is the numeric value of the `HWND`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct WindowId(pub u64);

impl fmt::Display for WindowId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:#x}", self.0)
    }
}

/// A monitor, identified by a number moon assigns and keeps stable for as long
/// as the device stays connected.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct MonitorId(pub u32);

/// Index of a workspace in configuration order.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct WorkspaceId(pub u32);

impl WorkspaceId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
