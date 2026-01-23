//! Geometry processing and rendering.
//!
//! This module handles vector graphics rendering using `bevy_prototype_lyon` and
//! Constructive Solid Geometry (CSG) operations using `clipper2`.

use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::rapier::geometry::TypedShape;

pub mod csg;

/// Component to specify the color of the gizmo rendering.
#[derive(Component, Clone, Copy, Debug)]
pub struct DebugColor(pub Color);

/// Plugin for geometry and vector rendering.
///
/// Initializes the `bevy_prototype_lyon` ShapePlugin for drawing shapes.
pub struct GeometryPlugin;

impl Plugin for GeometryPlugin {
    fn build(&self, app: &mut App) {
        // Lyon setup for vector rendering (Legacy/Deprecated by Gizmos)
        app.add_plugins(ShapePlugin);
        app.add_systems(Update, render_shapes_gizmos);
    }
}

fn render_shapes_gizmos(
    query: Query<(&Collider, &GlobalTransform, Option<&DebugColor>)>,
    mut gizmos: Gizmos,
) {
    for (collider, global_transform, debug_color) in query.iter() {
        let (scale, rotation, translation) = global_transform.to_scale_rotation_translation();
        let rotation_z = rotation.to_euler(EulerRot::XYZ).2;
        let rot2 = Rot2::radians(rotation_z);
        let color = debug_color.map(|c| c.0).unwrap_or(Color::srgb(0.0, 1.0, 0.0));

        // Access the raw Rapier shape
        // Collider in bevy_rapier2d is a wrapper around SharedShape
        let shared_shape = collider.raw.as_ref(); // &SharedShape
        let shape = shared_shape.as_typed_shape();

        match shape {
            TypedShape::Cuboid(c) => {
                let half_extents = Vec2::from(c.half_extents);
                gizmos.rect_2d(
                    Isometry2d::new(translation.truncate(), rot2),
                    half_extents * 2.0 * scale.truncate(),
                    color,
                );
            }
            TypedShape::Ball(b) => {
                gizmos.circle_2d(
                    Isometry2d::new(translation.truncate(), rot2),
                    b.radius * scale.x.max(scale.y),
                    color,
                );
            }
            TypedShape::ConvexPolygon(p) => {
                let points: Vec<Vec2> = p.points().iter().map(|p| Vec2::new(p.x, p.y)).collect();
                let transform_mat = global_transform.compute_matrix();

                for i in 0..points.len() {
                    let p1 = points[i];
                    let p2 = points[(i + 1) % points.len()];

                    let p1_world = transform_mat.transform_point3(p1.extend(0.0)).truncate();
                    let p2_world = transform_mat.transform_point3(p2.extend(0.0)).truncate();

                    gizmos.line_2d(p1_world, p2_world, color);
                }
            }
            TypedShape::Compound(c) => {
                 for (iso, shape) in c.shapes() {
                     // Handle compound shapes (recursively or just one level)
                     // Transform the isometry to world space
                     // This is getting complicated. For now, skip compound or implement simple recursion if needed.
                     // The Polygon tool uses ConvexDecomposition which creates a Compound of ConvexPolygons.
                     if let TypedShape::ConvexPolygon(p) = shape.as_typed_shape() {
                         // We need to combine global_transform with iso
                         // Global * Local(iso)

                         // Global Transform Matrix
                         let global_mat = global_transform.compute_matrix();

                         // Local Isometry Matrix
                         let local_rot = Quat::from_rotation_z(iso.rotation.angle());
                         let local_trans = Vec3::new(iso.translation.vector.x, iso.translation.vector.y, 0.0);
                         let local_mat = Mat4::from_rotation_translation(local_rot, local_trans);

                         let final_mat = global_mat * local_mat;

                         let points: Vec<Vec2> = p.points().iter().map(|p| Vec2::new(p.x, p.y)).collect();
                         for i in 0..points.len() {
                            let p1 = points[i];
                            let p2 = points[(i + 1) % points.len()];

                            let p1_world = final_mat.transform_point3(p1.extend(0.0)).truncate();
                            let p2_world = final_mat.transform_point3(p2.extend(0.0)).truncate();

                            gizmos.line_2d(p1_world, p2_world, color);
                        }
                     }
                 }
            }
            _ => {}
        }
    }
}
