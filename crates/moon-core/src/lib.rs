//! moon's OS-independent core.
//!
//! Everything here is pure: it takes state and events and returns new state,
//! plans and scenes. Nothing in this crate talks to Windows, and `unsafe` is
//! forbidden.

#![forbid(unsafe_code)]

pub mod desktop;
pub mod geometry;
pub mod help;
pub mod ids;
pub mod tree;

pub mod command;
pub mod config;
pub mod layout;
pub mod motion;
pub mod protocol;
pub mod scene;
pub mod settings;
pub mod world;
