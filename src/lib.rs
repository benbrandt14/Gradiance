//! Gradiance: an Algodoo-inspired 2.5D physics sandbox.
//!
//! # Architecture
//!
//! One-way dataflow with a single mutation choke point:
//!
//! ```text
//! input/picking → tools & UI → intent messages → command dispatch
//!      → authored components → Changed<>-driven sync → physics / render
//! ```
//!
//! Layer boundaries are mechanically enforced (see `CLAUDE.md` and
//! `tests/boundaries.rs`): `avian2d` only inside `physics`, `egui` only
//! inside `ui`, the command stack only inside `command`.

pub mod command;
pub mod core;
pub mod domain;
pub mod geometry;
pub mod interaction;
pub mod persist;
pub mod physics;
pub mod prelude;
pub mod render;
pub mod ui;

use bevy::app::plugin_group;

plugin_group! {
    /// Everything Gradiance adds on top of Bevy's `DefaultPlugins`.
    pub struct GradiancePlugins {
        crate::core:::CorePlugin,
        crate::domain:::DomainPlugin,
        crate::command:::CommandPlugin,
        crate::persist:::PersistPlugin,
        crate::physics:::GradiancePhysicsPlugin,
        crate::interaction:::InteractionPlugin,
        crate::render:::GradianceRenderPlugin,
        crate::ui:::GradianceUiPlugin,
    }
}
