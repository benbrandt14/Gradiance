//! Trajectory trails: derived samples behind the authored [`Tracer`]
//! marker.
//!
//! The trail is derived state per rule #5: sampled live from `Transform`
//! on the physics clock, never serialized, never in undo records. The
//! sampler runs headless too (tests assert on trails); only the drawing
//! needs a renderer.
//!
//! A `Tracer` lives on either a **body** (the inspector convenience) or a
//! standalone **behavior node** (the placeable tracer tool,
//! [`domain::node`](gradiance_domain::node)); both carry `Tracer` +
//! `Transform` + `Appearance`, so the sampler/drawer here treat them
//! uniformly. An attached node's `Transform` is re-derived each frame from
//! its target body by [`follow_node_attachments`].

use avian2d::prelude::Physics;
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_core::ids::IdIndex;
use gradiance_core::states::GameState;
use gradiance_domain::Body;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance_domain::tracer::Tracer;
use gradiance_interaction::overlay::OverlayGizmos;
use gradiance_interaction::selection::Selection;
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

/// Repositions each attached behavior node onto its target body (offset in
/// the body's local frame), so a node placed on a body rides it and its
/// trail traces the body's motion. Free nodes (no target) keep their pose.
/// Runs before [`sample_traces`]; the `Without<BehaviorNode>` filter proves
/// the two `Transform` accesses are disjoint.
pub fn follow_node_attachments(
    index: Res<IdIndex>,
    bodies: Query<&Transform, (With<Body>, Without<BehaviorNode>)>,
    mut nodes: Query<(&NodeAttachment, &mut Transform), With<BehaviorNode>>,
) {
    for (attachment, mut transform) in &mut nodes {
        let Some(target) = attachment.target else {
            continue;
        };
        let Some(pose) = index
            .entity(target)
            .and_then(|e| bodies.get(e).ok())
            .map(gradiance_core::units::PosRot::from_transform)
        else {
            continue;
        };
        let world = pose.pos + Vec2::from_angle(pose.rot).rotate(attachment.offset);
        transform.translation.x = world.x;
        transform.translation.y = world.y;
    }
}

/// Samples every traced entity's position while playing, and expires
/// samples older than the tracer's fade window. Ages on the physics
/// clock, so pausing freezes the trail. A **free** tracer node (attached to
/// nothing) does nothing — a tracer traces an object's motion, and there is
/// none to trace.
pub fn sample_traces(
    mut commands: Commands,
    time: Res<Time<Physics>>,
    mut traced: Query<(
        Entity,
        &Tracer,
        &Transform,
        Option<&NodeAttachment>,
        Option<&mut TraceTrail>,
    )>,
) {
    let now = time.elapsed_secs();
    for (entity, tracer, transform, attachment, trail) in &mut traced {
        // A node tracer with no target is inert (body tracers have no
        // `NodeAttachment` at all and always sample).
        if attachment.is_some_and(|a| a.target.is_none()) {
            continue;
        }
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

/// Draws each trail as a fading polyline in the body's own fill color —
/// or the signal dataflow's trail tint when one is bound.
pub fn draw_traces(
    time: Res<Time<Physics>>,
    trails: Query<(
        &Tracer,
        &TraceTrail,
        &Appearance,
        Option<&gradiance_signal::SignalColorOverride>,
    )>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    let now = time.elapsed_secs();
    for (tracer, trail, appearance, tint) in &trails {
        let fade = tracer.fade_secs.max(0.05);
        let fill = match tint.and_then(|t| t.trail) {
            Some(color) => {
                let c = color.to_srgba();
                gradiance_domain::appearance::Rgba {
                    r: c.red,
                    g: c.green,
                    b: c.blue,
                    a: c.alpha,
                }
            }
            None => appearance.fill,
        };
        let samples: Vec<(f32, Vec2)> = trail.0.iter().copied().collect();
        match tracer.pattern {
            gradiance_domain::tracer::TracePattern::Line => {
                for pair in samples.windows(2) {
                    let (t, a) = pair[0];
                    let (_, b) = pair[1];
                    let alpha = (1.0 - (now - t) / fade).clamp(0.0, 1.0);
                    if alpha > 0.0 {
                        gizmos.line_2d(a, b, Color::srgba(fill.r, fill.g, fill.b, alpha * 0.9));
                    }
                }
            }
            gradiance_domain::tracer::TracePattern::Dots => {
                for (t, p) in samples {
                    let alpha = (1.0 - (now - t) / fade).clamp(0.0, 1.0);
                    if alpha > 0.0 {
                        gizmos.circle_2d(
                            Isometry2d::from_translation(p),
                            tracer.size.max(0.5),
                            Color::srgba(fill.r, fill.g, fill.b, alpha),
                        );
                    }
                }
            }
        }
    }
}

/// Draws each behavior node's glyph (a small ring at its position, labelled
/// by kind), so a placed node is visible and shows its selection state.
/// Attached nodes draw a tether to their target.
pub fn draw_node_glyphs(
    selection: Res<Selection>,
    index: Res<IdIndex>,
    nodes: Query<(Entity, &Transform, &NodeKind, &NodeAttachment, &Appearance)>,
    bodies: Query<&Transform, With<Body>>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    for (entity, transform, kind, attachment, appearance) in &nodes {
        let p = transform.translation.truncate();
        let selected = selection.contains(entity);
        let fill = appearance.fill;
        let base = Color::srgb(fill.r, fill.g, fill.b);
        // A double ring marks the node; brighter + larger when selected.
        let r = if selected { 8.0 } else { 6.0 };
        gizmos.circle_2d(Isometry2d::from_translation(p), r, base);
        gizmos.circle_2d(
            Isometry2d::from_translation(p),
            r - 2.5,
            if selected { css::WHITE.into() } else { base },
        );
        // A comet-tail tick marks the tracer (the one placeable node kind).
        match kind {
            NodeKind::Tracer(_) => {
                gizmos.line_2d(p, p + Vec2::new(-r - 4.0, 0.0), base.with_alpha(0.6));
            }
        }
        // Tether to the attached body.
        if let Some(target) = attachment.target
            && let Some(body_pos) = index
                .entity(target)
                .and_then(|e| bodies.get(e).ok())
                .map(|t| t.translation.truncate())
        {
            gizmos.line_2d(p, body_pos, base.with_alpha(0.25));
        }
    }
}

/// Registers the node-follow + trail sampler/cleanup (headless too) —
/// drawing is added by the render plugin alongside the other overlay
/// systems.
pub fn install_sampling(app: &mut App) {
    app.add_systems(
        Update,
        (
            follow_node_attachments,
            sample_traces.run_if(in_state(GameState::Playing)),
            cleanup_traces,
        )
            .chain(),
    );
}
