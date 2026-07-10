//! Gradiance: an Algodoo-inspired 2.5D physics sandbox.
//!
//! 2D shapes extrude into 3D prisms whose depth comes from their
//! collision-layer bits; the scene runs on `avian2d` XPBD physics and
//! is authored through an `egui` editor. This page is the map — every
//! module below carries its own detailed docs, and the pure ones
//! ([`geometry`], parts of [`domain`]/[`core`]) carry runnable examples.
//! For a diagram-heavy tour see `docs/architecture.md`.
//!
//! # One-way dataflow, one mutation choke point
//!
//! The single rule that keeps the editor tractable: **nothing mutates
//! authored state except commands, and commands only run from the
//! dispatcher.** Tools and UI never touch the world directly — they emit
//! typed *intent* messages, one exclusive system drains them into
//! [`GameCommand`](command::GameCommand)s, and everything derived
//! (colliders, meshes, engine joints) is rebuilt by `Changed<>`-driven
//! sync systems.
//!
//! ```text
//!   ┌─────────────┐   intent      ┌────────────────────┐
//!   │ tools & UI  │──messages────▶│ command::dispatch  │
//!   │ (emit only) │               │ (the ONLY mutator) │
//!   └─────────────┘               └─────────┬──────────┘
//!          ▲                                 │ push_apply
//!          │ read component copies           ▼
//!          │                        ┌────────────────────┐
//!          │                        │ CommandStack        │
//!          │                        │ (undo / redo)       │
//!          │                        └─────────┬──────────┘
//!          │                                  │ mutates
//!          │                                  ▼
//!          │                        ┌────────────────────┐
//!          └────────────────────────│ authored components│
//!                                   │ (domain/ + StableId)│
//!                                   └─────────┬──────────┘
//!                                             │ Changed<>
//!                        ┌────────────────────┼────────────────────┐
//!                        ▼                     ▼                    ▼
//!                  physics sync          render sync          (never saved:
//!                  colliders/joints      meshes/materials      all derived)
//! ```
//!
//! # Authored vs derived
//!
//! *Authored* state — the [`domain`] components plus
//! [`StableId`](core::ids::StableId) — **is** the save file. *Derived*
//! state (avian colliders/joints, render meshes/materials) is a pure
//! function of it, rebuilt on change, never serialized and never read by
//! commands. Loading a scene therefore has zero special cases: spawn the
//! authored records, and the sync systems reconstruct the rest.
//!
//! # Module map
//!
//! | Module | Role | May import |
//! |---|---|---|
//! | [`core`] | ids, states, units, constants | glam, serde |
//! | [`domain`] | authored components = the save file (incl. avian physics) | glam, serde, avian2d |
//! | [`geometry`] | pure 2D/2.5D math (SDF, contours, extrusion) | glam, lyon |
//! | [`command`] | intents, commands, undo, snapshots | domain, geometry |
//! | [`physics`] | avian systems: collider/joint derivation, read facade | avian2d |
//! | [`interaction`] | cursor, camera, selection, tools, snapping | command intents |
//! | [`render`] | derived meshes, materials, lighting, gizmos | domain, geometry |
//! | [`ui`] | the egui editor | **`egui`** (only here) |
//! | [`persist`] | RON save/load, snapshots | command snapshots |
//! | [`script`] | scripting: pure kernel + reflect bridge + the World-facing `bridge` seam (feature-gated) | pure math; `steel` + intents (only here) |
//!
//! These boundaries are **mechanically enforced** by `tests/boundaries.rs`
//! (and CI): `egui` appears only inside [`ui`], `steel` only inside
//! [`script`], and [`CommandStack`](command::CommandStack) only inside
//! [`command`]. avian is now used directly wherever physics is done (the
//! engine-swap seam was retired). Violating a boundary fails the build — see
//! `CLAUDE.md`
//! for the full contract and `docs/bevy19-notes.md` for verified Bevy
//! 0.19 API notes.

pub mod command;
pub mod core;
pub mod domain;
pub mod geometry;
pub mod interaction;
pub mod persist;
pub mod physics;
pub mod prelude;
pub mod render;
pub mod script;
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
