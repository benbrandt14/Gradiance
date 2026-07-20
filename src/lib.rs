//! Gradiance: an Algodoo-inspired 2.5D physics sandbox.
//!
//! 2D shapes extrude into 3D prisms whose depth comes from their authored
//! depth band; the scene runs on `avian2d` XPBD physics and is authored
//! through an `egui` editor. This crate is the **application shell**: it
//! re-exports every layer package under its architectural name, assembles
//! the plugin group, and hosts the integration test suite. Each layer
//! carries its own crate-level docs; for a diagram-heavy tour see
//! `docs/architecture.md`.
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
//! # Package map
//!
//! Every layer is its own workspace package under `crates/`; the layer
//! diagram **is** the dependency graph, so a boundary violation is a
//! compile error. This crate re-exports each package under its layer name:
//!
//! | Module (package) | Role | May depend on |
//! |---|---|---|
//! | [`kernel`] | pure Tier-B numeric kernel | *(nothing)* |
//! | [`core`] | ids, states, units, constants | bevy |
//! | [`geometry`] | the SDF shape tree + all 2D/2.5D math | core |
//! | [`domain`] | authored components = save content (incl. avian physics) | core, geometry |
//! | [`scene`] | records + RON format + migrations | core, domain |
//! | [`physics`] | avian systems: collider/joint derivation, read facade | core, domain, geometry |
//! | [`signal`] | signal-dataflow evaluation | kernel, domain, physics |
//! | [`command`] | intents, commands, undo | scene, signal + below |
//! | [`persist`] | disk IO, dialogs, autosave | scene, command |
//! | [`interaction`] | cursor, camera, selection, tools, snapping | command, persist + below |
//! | [`render`] | derived meshes, materials, lighting, gizmos | interaction + below |
//! | [`script`] | the steel Lisp bridge + operation registry (**`steel`** only here) | command, signal + below |
//! | [`ui`] | the egui editor (**`egui`** only here) | everything except render |
//!
//! `egui` and `steel` confinement is enforced by the package manifests
//! (only `gradiance-ui`/`gradiance-script` declare them) and re-checked by
//! `tests/boundaries.rs`, which also holds the exact-pin line on engine
//! dependencies. See `CLAUDE.md` for the full contract and
//! `docs/bevy19-notes.md` for verified Bevy 0.19 API notes.

pub use gradiance_command as command;
pub use gradiance_core as core;
pub use gradiance_domain as domain;
pub use gradiance_geometry as geometry;
pub use gradiance_interaction as interaction;
pub use gradiance_kernel as kernel;
pub use gradiance_persist as persist;
pub use gradiance_physics as physics;
pub use gradiance_render as render;
pub use gradiance_scene as scene;
pub use gradiance_script as script;
pub use gradiance_signal as signal;
pub use gradiance_ui as ui;

pub mod prelude;

use bevy::app::plugin_group;

plugin_group! {
    /// Everything Gradiance adds on top of Bevy's `DefaultPlugins`.
    pub struct GradiancePlugins {
        crate::core:::CorePlugin,
        crate::domain:::DomainPlugin,
        crate::command:::CommandPlugin,
        crate::persist:::PersistPlugin,
        crate::physics:::GradiancePhysicsPlugin,
        crate::signal:::SignalPlugin,
        crate::interaction:::InteractionPlugin,
        crate::render:::GradianceRenderPlugin,
        crate::ui:::GradianceUiPlugin,
        crate::script:::ScriptPlugin,
    }
}
