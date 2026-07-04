//! Authored components → engine components, `Changed<>`-driven and idempotent.

use crate::domain::Body;
use crate::domain::layers::LayerMask32;
use crate::domain::props::{BodyKind, PhysicalProps};
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
            // Outline and each hole become closed loops of segment indices;
            // convex decomposition handles concavity and holes alike.
            let mut vertices: Vec<Vector> = Vec::new();
            let mut indices: Vec<[u32; 2]> = Vec::new();
            for ring in std::iter::once(outline).chain(holes.iter()) {
                let start = vertices.len() as u32;
                let n = ring.len() as u32;
                if n < 3 {
                    return None;
                }
                vertices.extend(ring.iter().copied());
                indices.extend((0..n).map(|i| [start + i, start + (i + 1) % n]));
            }
            Some(Collider::convex_decomposition(vertices, indices))
        }
        // Truly infinite: the surface passes through the body origin with
        // local +Y outward; body rotation orients it.
        ShapeDef::HalfPlane => Some(Collider::half_space(Vector::Y)),
    }
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

/// Applies rigid-body kind and material properties for bodies whose
/// physical properties changed.
pub fn sync_rigid_bodies(
    mut commands: Commands,
    changed: Query<(Entity, &PhysicalProps), (With<Body>, Changed<PhysicalProps>)>,
) {
    for (entity, props) in &changed {
        let kind = match props.body {
            BodyKind::Dynamic => RigidBody::Dynamic,
            BodyKind::Static => RigidBody::Static,
            BodyKind::Kinematic => RigidBody::Kinematic,
        };
        let mut entity_commands = commands.entity(entity);
        entity_commands.insert((
            kind,
            ColliderDensity(props.density),
            Friction::new(props.friction),
            Restitution::new(props.restitution),
            GravityScale(props.gravity_scale),
        ));
        if props.sensor {
            entity_commands.insert(Sensor);
        } else {
            entity_commands.remove::<Sensor>();
        }
        if props.rotation_locked {
            entity_commands.insert(LockedAxes::ROTATION_LOCKED);
        } else {
            entity_commands.remove::<LockedAxes>();
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
