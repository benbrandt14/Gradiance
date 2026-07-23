//! Dev-only pipeline diagnostics. Because the editor defers everything
//! (intent → dispatch → `Changed<>` sync), a bug's symptom lands frames after
//! its cause. This traces how many entities each derived-sync `Changed<>`
//! query matches per frame — the one fact Bevy's own system spans and
//! `track_location` do not report. Intents and executed commands trace from
//! [`dispatch`](crate::dispatch) unconditionally; this sync-count mirror is
//! `dev`-feature only. Enable with `RUST_LOG=gradiance_command=trace`.

use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::joint::JointDef;
use gradiance_domain::settings::{RenderSettings, SimSettings};
use gradiance_domain::shape::ShapeDef;

/// Traces the entity count each derived-sync `Changed<>` query matched this
/// frame. A mirror with the same filter as the real sync system, run once per
/// frame, matches exactly what that system processes — without touching it.
/// Field names mirror the real systems' (keep in lockstep).
#[allow(clippy::type_complexity)]
fn trace_sync_counts(
    changed_shapes: Query<(), (With<Body>, Changed<ShapeDef>)>,
    changed_layers: Query<(), (With<Body>, Changed<DepthBand>)>,
    changed_meshes: Query<(), (With<Body>, Or<(Changed<ShapeDef>, Changed<DepthBand>)>)>,
    changed_materials: Query<(), (With<Body>, Changed<Appearance>)>,
    changed_joints: Query<(), Changed<JointDef>>,
    sim_settings: Option<Res<SimSettings>>,
    render_settings: Option<Res<RenderSettings>>,
) {
    let colliders = changed_shapes.iter().count();
    let layers = changed_layers.iter().count();
    let meshes = changed_meshes.iter().count();
    let materials = changed_materials.iter().count();
    let joints = changed_joints.iter().count();
    let sim = sim_settings.is_some_and(|s| s.is_changed());
    let render = render_settings.is_some_and(|s| s.is_changed());
    if colliders + layers + meshes + materials + joints == 0 && !sim && !render {
        return;
    }
    trace!(
        target: "gradiance_command::sync",
        colliders = colliders as u64,
        layers = layers as u64,
        meshes = meshes as u64,
        materials = materials as u64,
        joints = joints as u64,
        sim,
        render,
        "derived sync fired"
    );
}

/// Installs the sync-count trace in `PostUpdate` (alongside the real sync
/// systems). Registered by `CommandPlugin` under the `dev` feature.
pub fn install(app: &mut App) {
    app.add_systems(PostUpdate, trace_sync_counts);
}
