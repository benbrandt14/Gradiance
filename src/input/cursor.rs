//! Cursor position tracking.
//!
//! Tracks the mouse cursor's world position, accounting for camera transforms and UI blocking.

use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy::math::bounding::Aabb2d;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use bevy_rapier2d::rapier::math::{Point, Isometry, Vector};
use bevy_rapier2d::rapier::parry::shape::TypedShape;

/// Custom cursor position resource.
///
/// Stores the world-space coordinates of the mouse cursor.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorWorldPos(pub Option<Vec2>);

/// Source of the snapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum SnapSource {
    /// Snapped to the grid.
    Grid,
    /// Snapped to the center of an object.
    ObjectCenter,
    /// Snapped to the edge of an object.
    ObjectEdge,
    /// Snapped to the midpoint of an object's edge.
    ObjectMidpoint,
}

/// Resource to track the snapping status of the cursor.
#[derive(Resource, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Resource)]
pub struct SnappingStatus {
    /// Whether the cursor is currently snapped.
    pub snapped: bool,
    /// The snapped position in world space.
    pub snap_pos: Vec2,
    /// The source of the snap (Grid, Object, etc.).
    pub snap_source: Option<SnapSource>,
}

/// Updates the `CursorWorldPos` resource.
///
/// Projects the window cursor position to world coordinates.
/// Returns `None` if the cursor is over an Egui UI area.
pub fn update_cursor_pos(
    mut cursor_pos: ResMut<CursorWorldPos>,
    mut snap_status: ResMut<SnappingStatus>,
    grid_settings: Res<GridSettings>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform)>,
    mut contexts: EguiContexts,
    // Physics queries for object snapping
    q_rapier: Query<&RapierContext>,
    q_collider: Query<(&Collider, &GlobalTransform)>,
) {
    // Reset snap status
    snap_status.snapped = false;
    snap_status.snap_source = None;

    // Use iter().next() instead of get_single() as per AGENTS.md/memory
    let Some(window) = q_window.iter().next() else {
        return;
    };
    let Some((camera, camera_transform)) = q_camera.iter().next() else {
        return;
    };

    // Check if mouse is over UI
    let ctx = contexts.ctx_mut();
    if ctx.is_pointer_over_area() {
        cursor_pos.0 = None;
        return;
    }

    let mut raw_pos = None;
    if let Some(screen_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            raw_pos = Some(world_pos);
        }
    }

    cursor_pos.0 = raw_pos;

    let Some(raw) = raw_pos else {
        return;
    };

    let mut best_snap_pos = raw;
    let mut snapped = false;
    let mut best_snap_dist_sq = grid_settings.snap_distance.powi(2);
    let mut snap_source = None;

    // 1. Object Snapping
    if grid_settings.snap_to_objects {
        if let Some(rapier_context) = q_rapier.iter().next() {
            let search_range = grid_settings.snap_distance * 2.0;
            // Aabb2d takes center and half-extents
            let aabb = Aabb2d::new(raw, Vec2::splat(search_range));

            rapier_context.colliders_with_aabb_intersecting_aabb(aabb, |entity| {
                if let Ok((collider, transform)) = q_collider.get(entity) {
                    // Center Snap
                    if grid_settings.snap_to_object_centers {
                        let center = transform.translation().truncate();
                        let d2 = center.distance_squared(raw);
                        if d2 < best_snap_dist_sq {
                            best_snap_dist_sq = d2;
                            best_snap_pos = center;
                            snapped = true;
                            snap_source = Some(SnapSource::ObjectCenter);
                        }
                    }

                    // Edge/Midpoint Snap
                    if grid_settings.snap_to_object_edges || grid_settings.snap_to_object_midpoints {

                         let transform_iso = Isometry::new(
                             Vector::new(transform.translation().x, transform.translation().y),
                             transform.compute_transform().rotation.to_euler(EulerRot::XYZ).2
                         );

                         // Edge Snap
                         if grid_settings.snap_to_object_edges {
                             let pos = transform.translation().truncate();
                             let rot = transform.compute_transform().rotation.to_euler(EulerRot::XYZ).2;
                             let projection = collider.project_point(pos, rot, raw, true);
                             let world_closest_pt = projection.point; // Vec2 (World space)

                             let d2 = world_closest_pt.distance_squared(raw);
                             if d2 < best_snap_dist_sq {
                                 best_snap_dist_sq = d2;
                                 best_snap_pos = world_closest_pt;
                                 snapped = true;
                                 snap_source = Some(SnapSource::ObjectEdge);
                             }
                         }

                         // Discrete Midpoints (Simplified for Box and Segment)
                         if grid_settings.snap_to_object_midpoints {
                             let candidates = match collider.raw.as_typed_shape() {
                                 TypedShape::Cuboid(c) => {
                                     let hx = c.half_extents.x;
                                     let hy = c.half_extents.y;
                                     vec![
                                         Point::new(0.0, hy), Point::new(0.0, -hy),
                                         Point::new(hx, 0.0), Point::new(-hx, 0.0)
                                     ]
                                 },
                                 TypedShape::Segment(s) => {
                                     let mid = (s.a.coords + s.b.coords) * 0.5;
                                     vec![Point::from(mid)]
                                 },
                                 _ => vec![]
                             };

                             for local_pt in candidates {
                                 let world_pt_na = transform_iso.transform_point(&local_pt);
                                 let world_pt = Vec2::new(world_pt_na.x, world_pt_na.y);
                                 let d2 = world_pt.distance_squared(raw);
                                 if d2 < best_snap_dist_sq {
                                     best_snap_dist_sq = d2;
                                     best_snap_pos = world_pt;
                                     snapped = true;
                                     snap_source = Some(SnapSource::ObjectMidpoint);
                                 }
                             }
                         }
                    }
                }
                true // Continue traversal
            });
        }
    }

    // 2. Grid Snapping (only if not snapped to object)
    if !snapped && grid_settings.snap_to_grid {
        let grid_pos = snap_to_grid(raw, grid_settings.spacing);
        best_snap_pos = grid_pos;
        snapped = true;
        snap_source = Some(SnapSource::Grid);
    }

    if snapped {
        cursor_pos.0 = Some(best_snap_pos);
        snap_status.snapped = true;
        snap_status.snap_pos = best_snap_pos;
        snap_status.snap_source = snap_source;
    }
}

/// System to visualize the snapping point.
pub fn draw_snap_indicators(
    snap_status: Res<SnappingStatus>,
    mut gizmos: Gizmos,
    q_camera: Query<&Projection, With<Camera2d>>,
) {
    if !snap_status.snapped {
        return;
    }

    let pos = snap_status.snap_pos;
    let color = match snap_status.snap_source {
        Some(SnapSource::Grid) => Color::srgb(0.0, 1.0, 0.0), // Green
        Some(_) => Color::srgb(0.0, 1.0, 1.0), // Cyan
        None => Color::WHITE,
    };

    let scale = if let Some(projection) = q_camera.iter().next() {
        if let Projection::Orthographic(ortho) = projection {
             ortho.scale
        } else {
             1.0
        }
    } else {
        1.0
    };

    let radius = 5.0 * scale * 0.01;

    gizmos.circle_2d(pos, radius, color);

    // Crosshair
    let arm_len = radius * 2.0;
    gizmos.line_2d(pos - Vec2::new(arm_len, 0.0), pos + Vec2::new(arm_len, 0.0), color);
    gizmos.line_2d(pos - Vec2::new(0.0, arm_len), pos + Vec2::new(0.0, arm_len), color);
}
