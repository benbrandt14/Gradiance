//! Snapping logic for the cursor.
//!
//! Handles snapping to grid lines and object features (centers, edges, midpoints).

use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy::math::bounding::Aabb2d;
use bevy_rapier2d::rapier::math::{Isometry, Point, Vector};
use bevy_rapier2d::rapier::parry::shape::TypedShape;

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
    /// Snapped to the corner of an object.
    ObjectCorner,
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

/// Calculates the snapped position based on settings and environment.
///
/// # Arguments
///
/// * `raw_pos` - The raw world position of the cursor.
/// * `grid_settings` - The current grid and snapping settings.
/// * `spatial_query` - A closure that performs an AABB query. It receives an `Aabb2d` and a callback `FnMut(Entity) -> bool`.
/// * `get_collider_data` - A closure that retrieves `(Collider, GlobalTransform)` for a given `Entity`.
pub fn calculate_snapping<F>(
    raw_pos: Vec2,
    grid_settings: &GridSettings,
    mut spatial_query: Option<F>,
    get_collider_data: impl Fn(Entity) -> Option<(Collider, GlobalTransform)>,
) -> (Vec2, bool, Option<SnapSource>)
where
    F: FnMut(Aabb2d, &mut dyn FnMut(Entity) -> bool),
{
    let mut best_snap_pos = raw_pos;
    let mut snapped = false;
    let mut best_snap_dist_sq = grid_settings.snap_distance.powi(2);
    let mut snap_source = None;

    // 1. Object Snapping
    if grid_settings.snap_to_objects
        && let Some(query_fn) = spatial_query.as_mut()
    {
        let search_range = grid_settings.snap_distance * 2.0;
        // Aabb2d takes center and half-extents
        let aabb = Aabb2d::new(raw_pos, Vec2::splat(search_range));

        query_fn(aabb, &mut |entity| {
            if let Some((collider, transform)) = get_collider_data(entity) {
                // Center Snap
                if grid_settings.snap_to_object_centers {
                    let center = transform.translation().truncate();
                    let d2 = center.distance_squared(raw_pos);
                    if d2 < best_snap_dist_sq {
                        best_snap_dist_sq = d2;
                        best_snap_pos = center;
                        snapped = true;
                        snap_source = Some(SnapSource::ObjectCenter);
                    }
                }

                // Corner/Edge/Midpoint Snap
                if grid_settings.snap_to_object_edges
                    || grid_settings.snap_to_object_midpoints
                    || grid_settings.snap_to_object_corners
                {
                    let transform_iso = Isometry::new(
                        Vector::new(transform.translation().x, transform.translation().y),
                        transform
                            .compute_transform()
                            .rotation
                            .to_euler(EulerRot::XYZ)
                            .2,
                    );

                    // Discrete Midpoints (Simplified for Box and Segment)
                    // Prioritize Midpoints over Edges by checking them first
                    if grid_settings.snap_to_object_midpoints {
                        let candidates = match collider.raw.as_typed_shape() {
                            TypedShape::Cuboid(c) => {
                                let hx = c.half_extents.x;
                                let hy = c.half_extents.y;
                                vec![
                                    Point::new(0.0, hy),
                                    Point::new(0.0, -hy),
                                    Point::new(hx, 0.0),
                                    Point::new(-hx, 0.0),
                                ]
                            }
                            TypedShape::Segment(s) => {
                                let mid = (s.a.coords + s.b.coords) * 0.5;
                                vec![Point::from(mid)]
                            }
                            _ => vec![],
                        };

                        for local_pt in candidates {
                            let world_pt_na = transform_iso.transform_point(&local_pt);
                            let world_pt = Vec2::new(world_pt_na.x, world_pt_na.y);
                            let d2 = world_pt.distance_squared(raw_pos);
                            if d2 < best_snap_dist_sq {
                                best_snap_dist_sq = d2;
                                best_snap_pos = world_pt;
                                snapped = true;
                                snap_source = Some(SnapSource::ObjectMidpoint);
                            }
                        }
                    }

                    // Corner Snap (Vertices)
                    // Prioritize Corners over Edges by checking them first (same priority level as Midpoints)
                    if grid_settings.snap_to_object_corners {
                        let candidates = match collider.raw.as_typed_shape() {
                            TypedShape::Cuboid(c) => {
                                let hx = c.half_extents.x;
                                let hy = c.half_extents.y;
                                vec![
                                    Point::new(hx, hy),
                                    Point::new(hx, -hy),
                                    Point::new(-hx, hy),
                                    Point::new(-hx, -hy),
                                ]
                            }
                            TypedShape::Segment(s) => {
                                vec![Point::from(s.a.coords), Point::from(s.b.coords)]
                            }
                            TypedShape::Triangle(t) => {
                                vec![
                                    Point::from(t.a.coords),
                                    Point::from(t.b.coords),
                                    Point::from(t.c.coords),
                                ]
                            }
                            _ => vec![],
                        };

                        for local_pt in candidates {
                            let world_pt_na = transform_iso.transform_point(&local_pt);
                            let world_pt = Vec2::new(world_pt_na.x, world_pt_na.y);
                            let d2 = world_pt.distance_squared(raw_pos);
                            if d2 < best_snap_dist_sq {
                                best_snap_dist_sq = d2;
                                best_snap_pos = world_pt;
                                snapped = true;
                                snap_source = Some(SnapSource::ObjectCorner);
                            }
                        }
                    }

                    // Edge Snap
                    if grid_settings.snap_to_object_edges {
                        let pos = transform.translation().truncate();
                        let rot = transform
                            .compute_transform()
                            .rotation
                            .to_euler(EulerRot::XYZ)
                            .2;
                        let projection = collider.project_point(pos, rot, raw_pos, true);
                        let world_closest_pt = projection.point; // Vec2 (World space)

                        let d2 = world_closest_pt.distance_squared(raw_pos);
                        // Use < to respect priority of previous snaps (Midpoints) if distances are equal
                        if d2 < best_snap_dist_sq {
                            best_snap_dist_sq = d2;
                            best_snap_pos = world_closest_pt;
                            snapped = true;
                            snap_source = Some(SnapSource::ObjectEdge);
                        }
                    }
                }
            }
            true // Continue traversal
        });
    }

    // 2. Grid Snapping (only if not snapped to object)
    if !snapped && grid_settings.snap_to_grid {
        let grid_pos = snap_to_grid(raw_pos, grid_settings.spacing);
        best_snap_pos = grid_pos;
        snapped = true;
        snap_source = Some(SnapSource::Grid);
    }

    (best_snap_pos, snapped, snap_source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    // Helper for no-op spatial query
    fn no_spatial_query(_aabb: Aabb2d, _callback: &mut dyn FnMut(Entity) -> bool) {}

    // Helper for no-op collider data
    fn no_collider_data(_entity: Entity) -> Option<(Collider, GlobalTransform)> {
        None
    }

    #[rstest]
    #[case(Vec2::new(1.1, 1.1), 1.0, Vec2::new(1.0, 1.0))]
    #[case(Vec2::new(1.6, 1.6), 1.0, Vec2::new(2.0, 2.0))]
    fn test_grid_snapping(#[case] raw: Vec2, #[case] spacing: f32, #[case] expected: Vec2) {
        let mut settings = GridSettings::default();
        settings.snap_to_grid = true;
        settings.spacing = spacing;
        settings.snap_to_objects = false;

        let (pos, snapped, source) =
            calculate_snapping(raw, &settings, Some(no_spatial_query), no_collider_data);

        assert!(snapped);
        assert_eq!(source, Some(SnapSource::Grid));
        assert!((pos - expected).length() < 0.0001);
    }

    #[rstest]
    fn test_grid_snapping_disabled() {
        let mut settings = GridSettings::default();
        settings.snap_to_grid = false;
        settings.snap_to_objects = false;

        let raw = Vec2::new(1.1, 1.1);
        let (pos, snapped, source) =
            calculate_snapping(raw, &settings, Some(no_spatial_query), no_collider_data);

        assert!(!snapped);
        assert_eq!(source, None);
        assert_eq!(pos, raw);
    }

    #[rstest]
    fn test_object_center_snapping() {
        let mut settings = GridSettings::default();
        settings.snap_to_objects = true;
        settings.snap_to_object_centers = true;
        settings.snap_distance = 0.5;
        // Disable grid snapping to be sure we are testing object snapping
        settings.snap_to_grid = false;

        let raw = Vec2::new(10.1, 10.1);
        let object_center = Vec2::new(10.0, 10.0);

        let spatial_query = |aabb: Aabb2d, callback: &mut dyn FnMut(Entity) -> bool| {
            // Mock finding entity 1 inside AABB
            callback(Entity::from_raw(1));
        };

        let get_collider_data = |entity: Entity| {
            if entity.index() == 1 {
                Some((
                    Collider::cuboid(0.5, 0.5),
                    GlobalTransform::from_translation(object_center.extend(0.0)),
                ))
            } else {
                None
            }
        };

        let (pos, snapped, source) =
            calculate_snapping(raw, &settings, Some(spatial_query), get_collider_data);

        assert!(snapped);
        assert_eq!(source, Some(SnapSource::ObjectCenter));
        assert_eq!(pos, object_center);
    }
}
