//! Debug overlays: gizmo visualizations of editor/physics internals,
//! toggled from the Debug settings tab
//! ([`DebugSettings`](crate::domain::settings::DebugSettings)).
//!
//! Everything here reads authored components, the pure geometry layer,
//! and the engine-agnostic `physics::queries` facade — no avian types.

use crate::core::ids::IdIndex;
use crate::domain::Body;
use crate::domain::joint::{JointDef, JointKind};
use crate::domain::settings::DebugSettings;
use crate::domain::shape::ShapeDef;
use crate::geometry::polygonize::polygonize;
use crate::geometry::sdf;
use crate::physics::queries::PhysicsQueries;
use bevy::color::palettes::css;
use bevy::prelude::*;

fn world_point(transform: &Transform, local: Vec2) -> Vec2 {
    transform
        .compute_affine()
        .transform_point3(local.extend(0.0))
        .truncate()
}

/// Draws the enabled debug overlays.
pub fn draw_debug_overlays(
    debug: Res<DebugSettings>,
    bodies: Query<(Entity, &ShapeDef, &Transform), With<Body>>,
    joints: Query<&JointDef>,
    index: Res<IdIndex>,
    poses: Query<&Transform, With<Body>>,
    physics: PhysicsQueries,
    mut gizmos: Gizmos,
) {
    for (entity, shape, transform) in &bodies {
        let infinite = shape.contains_half_plane();

        if debug.show_colliders && !infinite {
            let contours = polygonize(shape);
            for ring in contours.rings() {
                let mut pts: Vec<Vec2> = ring.iter().map(|v| world_point(transform, *v)).collect();
                if let Some(first) = pts.first().copied() {
                    pts.push(first);
                }
                gizmos.linestrip_2d(pts, css::SPRING_GREEN);
            }
        }

        if debug.show_aabbs && !infinite {
            let (min, max) = sdf::aabb(shape);
            let corners = [
                Vec2::new(min.x, min.y),
                Vec2::new(max.x, min.y),
                Vec2::new(max.x, max.y),
                Vec2::new(min.x, max.y),
            ]
            .map(|c| world_point(transform, c));
            let (mut wmin, mut wmax) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
            for c in corners {
                wmin = wmin.min(c);
                wmax = wmax.max(c);
            }
            gizmos.rect_2d(
                Isometry2d::from_translation((wmin + wmax) / 2.0),
                wmax - wmin,
                css::DARK_KHAKI,
            );
        }

        if debug.show_origins {
            let origin = transform.translation.truncate();
            gizmos.line_2d(origin - Vec2::X * 6.0, origin + Vec2::X * 6.0, css::WHITE);
            gizmos.line_2d(origin - Vec2::Y * 6.0, origin + Vec2::Y * 6.0, css::WHITE);
            if !infinite {
                // Centroid may sit off-origin after CSG reshapes.
                let centroid = world_point(transform, polygonize(shape).centroid());
                gizmos.circle_2d(Isometry2d::from_translation(centroid), 3.0, css::HOT_PINK);
            }
        }

        if debug.show_velocities
            && let Some((lin, ang)) = physics.velocity_of(entity)
        {
            let origin = transform.translation.truncate();
            if physics.is_sleeping(entity) {
                gizmos.circle_2d(Isometry2d::from_translation(origin), 8.0, css::GRAY);
            } else {
                gizmos.arrow_2d(origin, origin + lin * 0.25, css::YELLOW);
                if ang.abs() > 0.05 {
                    // Angular velocity: an arc-ish tick, radius ∝ spin.
                    gizmos.circle_2d(
                        Isometry2d::from_translation(origin),
                        4.0 + ang.abs().min(20.0),
                        css::ORANGE,
                    );
                }
            }
        }
    }

    if debug.show_joint_anchors {
        for def in &joints {
            let color = match def.kind {
                JointKind::Hinge { .. } => css::ORANGE,
                JointKind::Weld => css::CRIMSON,
                JointKind::Slider { .. } => css::DEEP_SKY_BLUE,
            };
            let world_a = index
                .entity(def.body_a)
                .and_then(|e| poses.get(e).ok())
                .map(|t| world_point(t, def.anchor_a));
            let world_b = match def.body_b {
                Some(id) => index
                    .entity(id)
                    .and_then(|e| poses.get(e).ok())
                    .map(|t| world_point(t, def.anchor_b)),
                None => Some(def.anchor_b), // world pin
            };
            if let Some(a) = world_a {
                gizmos.circle_2d(Isometry2d::from_translation(a), 4.0, color);
            }
            if let Some(b) = world_b {
                gizmos.circle_2d(Isometry2d::from_translation(b), 6.0, color);
                if def.body_b.is_none() {
                    gizmos.line_2d(b - Vec2::splat(4.0), b + Vec2::splat(4.0), color);
                    gizmos.line_2d(b + Vec2::new(-4.0, 4.0), b + Vec2::new(4.0, -4.0), color);
                }
            }
            if let (Some(a), Some(b)) = (world_a, world_b) {
                gizmos.line_2d(a, b, color);
            }
        }
    }
}
