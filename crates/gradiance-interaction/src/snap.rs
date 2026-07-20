//! The snap engine: object snaps (CAD priority) then grid snap.
//!
//! One resource — [`SnappedCursor`] — is the contract: every tool reads it
//! instead of the raw cursor, so snapping behavior is uniform and future
//! snap sources (tangents, intersections, gesture guides) plug in here
//! without touching any tool.

use crate::cursor::CursorWorldPos;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::settings::{GridSettings, GridSystem, SnapConfig};
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::polygonize::polygonize;
use gradiance_geometry::snapping::{
    cartesian_snap, isometric_snap, polar_snap, project_on_segment,
};
use gradiance_physics::queries::PhysicsQueries;

/// What the cursor snapped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    /// Grid lattice point.
    Grid,
    /// Shape outline vertex.
    Vertex,
    /// Edge midpoint.
    Midpoint,
    /// Body center.
    Center,
    /// Closest point on an edge.
    Edge,
}

/// Entities excluded from object snapping — the bodies currently being
/// dragged (CAD behavior: you never snap to what you're moving). Tools
/// set this at gesture start and clear it on release.
#[derive(Resource, Default, Debug)]
pub struct SnapExclusions(pub Vec<Entity>);

/// The snapped cursor — the position tools should use.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct SnappedCursor {
    /// Raw cursor position on the sandbox plane.
    pub raw: Option<Vec2>,
    /// Snap-adjusted position (equals `raw` when nothing snapped).
    pub position: Option<Vec2>,
    /// What was snapped to, if anything.
    pub kind: Option<SnapKind>,
}

impl SnappedCursor {
    /// The effective cursor position (snapped when available).
    pub fn effective(&self) -> Option<Vec2> {
        self.position.or(self.raw)
    }
}

/// Computes [`SnappedCursor`] each frame.
///
/// Object snaps (vertex > midpoint > center > edge by proximity) win over
/// the grid; the capture radius is screen-constant (scales with zoom).
pub fn update_snapped_cursor(
    cursor: Res<CursorWorldPos>,
    grid: Res<GridSettings>,
    config: Res<SnapConfig>,
    exclusions: Res<SnapExclusions>,
    physics: PhysicsQueries,
    bodies: Query<(&ShapeDef, &Transform), With<Body>>,
    cam_scale: Res<crate::camera::CameraScale>,
    mut out: ResMut<SnappedCursor>,
) {
    let raw = cursor.0;
    *out = SnappedCursor {
        raw,
        position: raw,
        kind: None,
    };
    let Some(p) = raw else {
        return;
    };

    let scale = cam_scale.0;
    let radius = config.max_screen_distance * scale;

    // --- Object snaps (CAD priority). ---
    if config.objects_enabled {
        let mut best: Option<(f32, Vec2, SnapKind)> = None;
        let mut consider = |dist: f32, pos: Vec2, kind: SnapKind| {
            if dist <= radius && best.is_none_or(|(d, _, _)| dist < d) {
                best = Some((dist, pos, kind));
            }
        };

        let pad = Vec2::splat(radius);
        for entity in physics.bodies_in_aabb(p - pad * 8.0, p + pad * 8.0) {
            if exclusions.0.contains(&entity) {
                continue;
            }
            let Ok((shape, transform)) = bodies.get(entity) else {
                continue;
            };
            if config.sources.centers {
                let c = transform.translation.truncate();
                consider(c.distance(p), c, SnapKind::Center);
            }
            if !(config.sources.vertices || config.sources.midpoints || config.sources.edges) {
                continue;
            }
            let contours = polygonize(shape);
            for ring in contours.rings() {
                let n = ring.len();
                for i in 0..n {
                    let a = world_point(transform, ring[i]);
                    if config.sources.vertices {
                        consider(a.distance(p), a, SnapKind::Vertex);
                    }
                    let b = world_point(transform, ring[(i + 1) % n]);
                    if config.sources.midpoints {
                        let m = (a + b) / 2.0;
                        consider(m.distance(p), m, SnapKind::Midpoint);
                    }
                    if config.sources.edges {
                        let e = project_on_segment(p, a, b);
                        // Slightly deprioritize bare edges so vertices and
                        // midpoints on the same edge win ties.
                        consider(e.distance(p) + radius * 0.05, e, SnapKind::Edge);
                    }
                }
            }
        }

        if let Some((_, pos, kind)) = best {
            out.position = Some(pos);
            out.kind = Some(kind);
            return;
        }
    }

    // --- Grid snap. ---
    // Grid snapping only applies when the grid is visible (invisible
    // magnetism is disorienting).
    if grid.snap_enabled && grid.visible {
        let snapped = match grid.system {
            GridSystem::Cartesian => cartesian_snap(p, grid.origin, grid.rotation, grid.spacing),
            GridSystem::Isometric => isometric_snap(p, grid.origin, grid.rotation, grid.spacing),
            GridSystem::Polar { angular_divisions } => polar_snap(
                p,
                grid.origin,
                grid.rotation,
                grid.spacing,
                angular_divisions,
            ),
        };
        out.position = Some(snapped);
        out.kind = Some(SnapKind::Grid);
    }
}

fn world_point(transform: &Transform, local: Vec2) -> Vec2 {
    transform
        .compute_affine()
        .transform_point3(local.extend(0.0))
        .truncate()
}
