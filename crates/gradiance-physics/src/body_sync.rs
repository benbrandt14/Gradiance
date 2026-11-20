//! Authored components → engine components, `Changed<>`-driven and idempotent.

use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::profile::WidthProfile;

use crate::air::AirProfile;
use crate::buoyancy::SubmergeOutline;
use gradiance_geometry::contours::Contours;
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

/// The projected-width profile air drag reads ([`AirProfile`], brief N-4),
/// from the same discretization the collider uses: a leaf's polygonized
/// outline, every component's outline for a CSG tree (as its collider
/// covers every component). A half-plane is infinite and gets no width.
fn air_profile_for(shape: &ShapeDef, components: &[Contours]) -> WidthProfile {
    if shape.contains_half_plane() {
        return WidthProfile::ZERO;
    }
    let points: Vec<Vec2> = components
        .iter()
        .flat_map(|c| c.outline.iter().copied())
        .collect();
    WidthProfile::from_points(&points)
}

/// The outline buoyancy clips ([`SubmergeOutline`], brief N-7), from the
/// same components. A shape holding a half-plane is infinite: it gets none.
fn submerge_outline_for(shape: &ShapeDef, components: &[Contours]) -> SolidOutline {
    if shape.contains_half_plane() {
        return SolidOutline::default();
    }
    SolidOutline::from_components(components)
}

/// Regenerates colliders — and the other shape-derived physics data, the
/// air-drag width profile and the buoyancy outline — for bodies whose shape
/// changed. The shape is polygonized once for all three.
pub fn sync_colliders(
    mut commands: Commands,
    changed: Query<(Entity, &ShapeDef), (With<Body>, Changed<ShapeDef>)>,
) {
    for (entity, shape) in &changed {
        let components = gradiance_geometry::polygonize::polygonize_components(shape);
        match collider_for(shape, &components) {
            Some(collider) => {
                commands.entity(entity).insert((
                    collider,
                    AirProfile(air_profile_for(shape, &components)),
                    SubmergeOutline(submerge_outline_for(shape, &components)),
                ));
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
        (Entity, &DepthBand, &ShapeDef),
        (With<Body>, Or<(Changed<DepthBand>, Changed<ShapeDef>)>),
    >,
) {
    for (entity, band, shape) in &changed {
        let bits = if shape.contains_half_plane() {
            u32::MAX
        } else {
            band.bits()
        };
        commands
            .entity(entity)
            .insert(CollisionLayers::from_bits(bits, bits));
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
