//! Authored components → engine components, `Changed<>`-driven and idempotent.

use crate::domain::Body;
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use avian2d::math::Vector;
use avian2d::prelude::*;
use bevy::prelude::*;

/// Builds the engine collider for a validated shape.
///
/// Returns `None` (and the caller keeps the previous collider) when the
/// shape fails validation — commands should have refused it upstream.
fn collider_for(shape: &ShapeDef) -> Option<Collider> {
    shape.validate().ok()?;
    match shape {
        ShapeDef::Box { width, height } => Some(Collider::rectangle(*width, *height)),
        ShapeDef::Circle { radius } => Some(Collider::circle(*radius)),
        ShapeDef::Polygon { outline, holes } => {
            decomposition_collider(std::iter::once(outline).chain(holes.iter()))
        }
        // Truly infinite: the surface passes through the body origin with
        // local +Y outward; body rotation orients it.
        ShapeDef::HalfPlane => Some(Collider::half_space(Vector::Y)),
        // CSG trees collide as their contoured polygons — every connected
        // component's rings, so merged-but-barely-touching unions keep
        // full collision coverage.
        ShapeDef::Csg { .. } | ShapeDef::Placed { .. } => {
            let components = crate::geometry::polygonize::polygonize_components(shape);
            decomposition_collider(
                components
                    .iter()
                    .flat_map(crate::geometry::contours::Contours::rings),
            )
        }
    }
}

/// Convex-decomposes closed rings (outline + holes) into one collider;
/// concavity and holes are handled alike.
fn decomposition_collider<'a>(rings: impl Iterator<Item = &'a Vec<Vec2>>) -> Option<Collider> {
    let mut vertices: Vec<Vector> = Vec::new();
    let mut indices: Vec<[u32; 2]> = Vec::new();
    for ring in rings {
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
    Some(Collider::convex_decomposition(vertices, indices))
}

/// Regenerates colliders for bodies whose shape changed.
pub fn sync_colliders(
    mut commands: Commands,
    changed: Query<(Entity, &ShapeDef), (With<Body>, Changed<ShapeDef>)>,
) {
    for (entity, shape) in &changed {
        match collider_for(shape) {
            Some(collider) => {
                commands.entity(entity).insert(collider);
            }
            None => {
                warn!(?entity, "invalid ShapeDef reached body_sync; collider kept");
            }
        }
    }
}

/// Applies collision layers for bodies whose layer mask changed.
pub fn sync_collision_layers(
    mut commands: Commands,
    changed: Query<(Entity, &LayerMask32), (With<Body>, Changed<LayerMask32>)>,
) {
    for (entity, layers) in &changed {
        commands.entity(entity).insert(CollisionLayers::from_bits(
            layers.memberships,
            layers.filters,
        ));
    }
}
