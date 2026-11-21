//! Authored components → engine components, `Changed<>`-driven and idempotent.

use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::profile::DragPanels;

use crate::air::AirPanels;
use crate::buoyancy::SubmergeOutline;
use gradiance_geometry::contours::Contours;
use gradiance_geometry::polygonize::ShapeContours;
use gradiance_geometry::submerge::SolidOutline;

/// Builds the engine collider for a validated shape.
///
/// Returns `None` (and the caller keeps the previous collider) when the
/// shape fails validation — commands should have refused it upstream.
fn collider_for(shape: &ShapeDef, components: &[Contours]) -> Option<Collider> {
    shape.validate().ok()?;
    match shape {
        ShapeDef::Box { width, height } => Some(Collider::rectangle(*width, *height)),
        ShapeDef::Circle { radius } => Some(Collider::circle(*radius)),
        ShapeDef::Polygon { outline, holes } => {
            decomposition_collider(std::iter::once(outline).chain(holes.iter()))
        }
        // Exact: the segment along local x inflated by the radius.
        ShapeDef::Capsule {
            half_length,
            radius,
        } => Some(Collider::capsule_endpoints(
            *radius,
            Vec2::new(-half_length, 0.0),
            Vec2::new(*half_length, 0.0),
        )),
        // Truly infinite: the surface passes through the body origin with
        // local +Y outward; body rotation orients it.
        ShapeDef::HalfPlane => Some(Collider::half_space(Vec2::Y)),
        // CSG trees collide as their contoured polygons — every connected
        // component's rings, so merged-but-barely-touching unions keep
        // full collision coverage.
        ShapeDef::Csg { .. } | ShapeDef::Placed { .. } => {
            decomposition_collider(components.iter().flat_map(Contours::rings))
        }
    }
}

/// Convex-decomposes closed rings (outline + holes) into one collider;
/// concavity and holes are handled alike.
///
/// Rings are first decimated to [`DECIMATE_FRACTION`] of their extent:
/// a contoured curve keeps one vertex per field cell, and a fine
/// near-circular ring would otherwise decompose to no convex part (parry
/// reads its gently turning edges as collinear) and panic — see
/// `geometry::contours::decimate_ring`.
fn decomposition_collider<'a>(rings: impl Iterator<Item = &'a Vec<Vec2>>) -> Option<Collider> {
    let rings: Vec<&Vec<Vec2>> = rings.collect();
    let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for v in rings.iter().copied().flatten() {
        min = min.min(*v);
        max = max.max(*v);
    }
    let tol = (max - min).max_element().max(0.0) * DECIMATE_FRACTION;
    let mut vertices: Vec<Vec2> = Vec::new();
    let mut indices: Vec<[u32; 2]> = Vec::new();
    for ring in rings {
        let ring = gradiance_geometry::contours::decimate_ring(ring, tol);
        let start = vertices.len() as u32;
        let n = ring.len() as u32;
        if n < 3 {
            return None;
        }
        vertices.extend(ring.iter().copied());
        indices.extend((0..n).map(|i| [start + i, start + (i + 1) % n]));
    }
    if vertices.is_empty() {
        return None;
    }
    Some(Collider::convex_decomposition(&vertices, &indices))
}

/// Collider decimation tolerance as a fraction of the shape's extent
/// (1 mm on a 1 m body): small against contact slop, large enough that a
/// circle keeps some 50 vertices rather than hundreds.
const DECIMATE_FRACTION: f32 = 1e-3;

/// The drag panels air drag reads ([`AirPanels`], briefs N-4, Q-6), from
/// the same discretization the collider uses: the convex hull of a leaf's
/// polygonized outline, of every component's outline for a CSG tree (as its
/// collider covers every component). A half-plane is infinite and gets none.
fn air_panels_for(shape: &ShapeDef, components: &[Contours]) -> DragPanels {
    if shape.contains_half_plane() {
        return DragPanels::NONE;
    }
    let points: Vec<Vec2> = components
        .iter()
        .flat_map(|c| c.outline.iter().copied())
        .collect();
    DragPanels::from_points(&points)
}

/// The outline buoyancy clips ([`SubmergeOutline`], brief N-7), from the
/// same components. A shape holding a half-plane is infinite: it gets none.
fn submerge_outline_for(shape: &ShapeDef, components: &[Contours]) -> SolidOutline {
    if shape.contains_half_plane() {
        return SolidOutline::default();
    }
    SolidOutline::from_components(components)
}

/// Wakes `entity`'s island so the engine notices an authored change made
/// while the body was asleep — avian's own wake triggers cover pose,
/// velocity, force and gravity edits, but not a collider or layer regen
/// ([`sync_colliders`], [`sync_collision_layers`]). Only dynamic and
/// kinematic bodies belong to an island; static ones never sleep.
fn wake_if_movable(commands: &mut Commands, entity: Entity, kind: RigidBody) {
    if kind.is_dynamic() || kind.is_kinematic() {
        commands.queue(WakeBody(entity));
    }
}

/// Regenerates colliders — and the other shape-derived physics data, the
/// air-drag panels, the buoyancy outline and the cached
/// [`ShapeContours`] every per-frame polygon consumer reads — for bodies
/// whose shape changed. The shape is polygonized once for all of them.
pub fn sync_colliders(
    mut commands: Commands,
    changed: Query<(Entity, &ShapeDef, &RigidBody), (With<Body>, Changed<ShapeDef>)>,
) {
    for (entity, shape, kind) in &changed {
        let contours = ShapeContours(gradiance_geometry::polygonize::polygonize_components(shape));
        match collider_for(shape, &contours) {
            Some(collider) => {
                commands.entity(entity).insert((
                    collider,
                    AirPanels(air_panels_for(shape, &contours)),
                    SubmergeOutline(submerge_outline_for(shape, &contours)),
                    contours,
                ));
                // A resized or reshaped body must re-settle against its
                // neighbours even if it (they) had gone to sleep.
                wake_if_movable(&mut commands, entity, *kind);
            }
            None => {
                warn!(?entity, "invalid ShapeDef reached body_sync; collider kept");
            }
        }
    }
}

/// Applies collision layers for bodies whose depth band (or shape)
/// changed: memberships *and* filters are the band's derived layer bits,
/// so two bodies collide exactly when their depth bands overlap (at
/// layer granularity) — collision layer ≡ visual depth. Ground
/// half-planes are the base everything rests on and collide with all.
pub fn sync_collision_layers(
    mut commands: Commands,
    changed: Query<
        (Entity, &DepthBand, &ShapeDef, &RigidBody),
        (With<Body>, Or<(Changed<DepthBand>, Changed<ShapeDef>)>),
    >,
) {
    for (entity, band, shape, kind) in &changed {
        let bits = if shape.contains_half_plane() {
            u32::MAX
        } else {
            band.bits()
        };
        commands
            .entity(entity)
            .insert(CollisionLayers::from_bits(bits, bits));
        // A depth edit can make a sleeping body start or stop colliding with
        // its neighbours; the change must be solved, not frozen.
        wake_if_movable(&mut commands, entity, *kind);
    }
}

/// Stops a body's spin when its rotation gets locked. avian's
/// [`LockedAxes`] only stops *new* angular impulses: a body that was turning
/// when it got locked (a world-pin slider spawned on a body knocked by a
/// contact, or the inspector's lock pressed during a run) kept that spin
/// forever, its slider unable to hold it (joint stress test, 2026-11-20).
/// The angular velocity is simulation state, not authored, so the physics
/// seam owns it.
pub fn stop_locked_spin(
    mut bodies: Query<(&LockedAxes, &mut AngularVelocity), Changed<LockedAxes>>,
) {
    for (axes, mut spin) in &mut bodies {
        if axes.is_rotation_locked() && spin.0 != 0.0 {
            spin.0 = 0.0;
        }
    }
}
