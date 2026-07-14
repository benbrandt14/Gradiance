//! Bulk-matter simulation — the `sim`-feature spike (particles now, MPM
//! later; see `docs/mpm-trade-study.md`).
//!
//! Architecture mirrors the rest of the engine so promotion to a
//! `gradiance-sim` workspace crate is mechanical:
//!
//! - [`kernel`] is the **pure** Tier-B numeric core (`SoA` particles,
//!   integrator, N-body force via `particular`) — no ECS, unit-tested.
//! - [`bridge`] is the **one** ECS seam: authored [`Emitter`] components
//!   drive the derived [`Particles`] buffer on avian's fixed clock,
//!   sampling forces through the field cut-point.
//! - [`render`] draws the whole population as one rebuilt mesh (one draw
//!   call, depth-correct) — no per-particle entities or gizmos.
//!
//! Everything is feature-gated (`--features sim`) so the default build,
//! CI, and test loop stay lean until the crate split.

pub mod bridge;
pub mod groups;
pub mod kernel;
pub mod render;

use crate::core::states::GameState;
use bevy::prelude::*;
use bridge::{Emitter, Particles, SimConfig};

/// Installs the particle spike: the derived buffer, the fixed-step
/// emit/step systems, and the render pass. A demo fountain is spawned at
/// startup so `cargo run --features sim` shows the seam working.
#[derive(Default)]
pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Particles>();
        app.init_resource::<SimConfig>();
        app.init_resource::<groups::ParticleGroups>();
        // The fill queue lives in the UI crate (the button writes it) but
        // its drain lives here, so the sim owns the init — the resource
        // exists whenever the drain runs (incl. headless tests).
        app.init_resource::<crate::ui::context_menu::ParticleFillQueue>();
        app.register_type::<Emitter>();
        // Fill-shape requests are authoring (drained any time, not just
        // while playing, so a filled cloud appears immediately when paused).
        app.add_systems(Update, bridge::fill_from_shape);
        // Emit + step run on the fixed clock (determinism + coupling), and
        // only while playing — pausing freezes the population.
        app.add_systems(
            FixedUpdate,
            (bridge::emit_particles, bridge::step_particles)
                .chain()
                .run_if(in_state(GameState::Playing)),
        );
        // Rendering needs the render plugin (skip headless).
        if app.is_plugin_added::<bevy::render::RenderPlugin>() {
            app.add_systems(Startup, (spawn_demo_emitter, render::setup_particle_cloud));
            app.add_systems(Update, render::sync_particle_mesh);
        }
    }
}

/// A demo emitter so the spike is visible on launch: an [`Emitter`] below
/// the origin throwing particles upward.
///
/// Not authored/persisted (spike scope) — it is scene furniture for the
/// `--features sim` demo, spawned without a `StableId`.
fn spawn_demo_emitter(mut commands: Commands) {
    commands.spawn((
        Emitter::default(),
        bridge::EmitAccum::default(),
        Transform::from_xyz(0.0, -200.0, 0.0),
    ));
}
