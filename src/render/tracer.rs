//! Trajectory trails: derived samples behind the authored [`Tracer`]
//! marker.
//!
//! The trail is derived state per rule #5: sampled live from `Transform`
//! on the physics clock, never serialized, never in undo records. The
//! sampler runs headless too (tests assert on trails); only the drawing
//! needs a renderer.

use crate::core::states::GameState;
use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::tracer::Tracer;
use crate::render::overlay::OverlayGizmos;
use avian2d::prelude::Physics;
use bevy::prelude::*;
use std::collections::VecDeque;

/// Hard cap on samples per trail (a slow-fading fast body can't grow
/// unbounded).
const MAX_TRAIL_SAMPLES: usize = 2048;
/// Minimum movement (world px) between samples — a resting body keeps a
/// point, not a pile.
const MIN_SAMPLE_STEP: f32 = 1.0;

/// Derived trail of a traced body: `(physics-clock timestamp, position)`
/// samples, oldest first.
#[derive(Component, Debug, Default)]
pub struct TraceTrail(pub VecDeque<(f32, Vec2)>);

/// Samples every traced body's position while playing, and expires
/// samples older than the tracer's fade window. Ages on the physics
/// clock, so pausing freezes the trail.
pub fn sample_traces(
    mut commands: Commands,
    time: Res<Time<Physics>>,
    mut traced: Query<(Entity, &Tracer, &Transform, Option<&mut TraceTrail>), With<Body>>,
) {
    let now = time.elapsed_secs();
    for (entity, tracer, transform, trail) in &mut traced {
        let p = transform.translation.truncate();
        let Some(mut trail) = trail else {
            let mut fresh = TraceTrail::default();
            fresh.0.push_back((now, p));
            commands.entity(entity).insert(fresh);
            continue;
        };
        while trail
            .0
            .front()
            .is_some_and(|(t, _)| now - t > tracer.fade_secs.max(0.05))
        {
            trail.0.pop_front();
        }
        let moved = trail
            .0
            .back()
            .is_none_or(|(_, last)| last.distance_squared(p) >= MIN_SAMPLE_STEP * MIN_SAMPLE_STEP);
        if moved {
            if trail.0.len() >= MAX_TRAIL_SAMPLES {
                trail.0.pop_front();
            }
            trail.0.push_back((now, p));
        }
    }
}

/// Drops the derived trail when its authored marker goes away (property
/// edit, undo). Runs unconditionally — a paused removal still cleans up.
pub fn cleanup_traces(mut commands: Commands, mut removed: RemovedComponents<Tracer>) {
    for entity in removed.read() {
        if let Ok(mut entity_mut) = commands.get_entity(entity) {
            entity_mut.remove::<TraceTrail>();
        }
    }
}

/// Draws each trail as a fading polyline in the body's own fill color.
pub fn draw_traces(
    time: Res<Time<Physics>>,
    trails: Query<(&Tracer, &TraceTrail, &Appearance), With<Body>>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    let now = time.elapsed_secs();
    for (tracer, trail, appearance) in &trails {
        let fade = tracer.fade_secs.max(0.05);
        let fill = appearance.fill;
        for pair in trail.0.iter().collect::<Vec<_>>().windows(2) {
            let (t, a) = *pair[0];
            let (_, b) = *pair[1];
            let alpha = (1.0 - (now - t) / fade).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            let color = Color::srgba(fill.r, fill.g, fill.b, alpha * 0.9);
            gizmos.line_2d(a, b, color);
        }
    }
}

/// Registers the trail sampler/cleanup (headless too) — drawing is added
/// by the render plugin alongside the other overlay systems.
pub fn install_sampling(app: &mut App) {
    app.add_systems(
        Update,
        (
            sample_traces.run_if(in_state(GameState::Playing)),
            cleanup_traces,
        ),
    );
}
