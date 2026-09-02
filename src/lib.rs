//! `TrackCrab` internals.
//!
//! Split into a library so the model and store layers can be exercised by
//! integration tests without going anywhere near a window.

pub mod app;
pub mod icon;
pub mod model;
pub mod store;
pub mod ui;
