//! The snap engine: object snaps (CAD priority) then grid snap.
//!
//! One resource — [`SnappedCursor`] — is the contract: every tool reads it
//! instead of the raw cursor, so snapping behavior is uniform and future
//! snap sources (intersections, gesture guides) plug in here
//! without touching any tool.

use crate::cursor::CursorWorldPos;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::settings::{GridSettings, GridSystem, SnapConfig};
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::snap_features::{
    SnapOutline, nearest_on_circle, snap_outline, tangent_points, tangent_radius,
};
use gradiance_geometry::snapping::{
    adaptive_step, cartesian_snap, isometric_snap, polar_snap, project_on_segment,
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
    /// A tangent touch point: where a segment from the draft's reference
    /// point grazes a circle, or where a circle being drawn would first
    /// touch an outline (see [`SnapReference`]).
    Tangent,
}

/// Bare edge candidates are scored this fraction of the capture radius
/// farther than they are, so a vertex or midpoint on the same edge wins
/// the tie.
const EDGE_TIE_BIAS: f32 = 0.05;

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

/// The point the active draft is drawing *from*, which turns on the
/// tangent snaps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DraftReference {
    /// A segment is being drawn from this point (the polygon's last vertex,
    /// the three-point box's last placed point, the strut's first anchor):
    /// nearby circles offer the points where that segment would be tangent.
    Line(Vec2),
    /// A circle is being drawn about this centre (the circle tool's radius
    /// drag): nearby outlines offer the radius at which it would just touch
    /// them, as their touch point.
    Radius(Vec2),
}

/// The active draft's [`DraftReference`], if any. Like [`SnapExclusions`]
/// this is tool-set: the tool drivers copy it from the active tool every
/// frame (`DraftTool::snap_reference`), and it is cleared when the tool
/// changes, so a draft abandoned by a tool switch leaves no reference.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct SnapReference(pub Option<DraftReference>);

/// The nearest object-snap candidates within the capture radius.
///
/// Tangents are a tier of their own: any tangent inside the capture radius
/// beats every other object snap. Under the plain nearest rule a circle's
/// own edge projection is never farther than its tangent point (it is the
/// nearest point on the circle by definition), so a tangent would only win
/// within the edge tie bias, well under a pixel. A tangent is offered only
/// while a draft has a [`SnapReference`], so the priority is the user's
/// stated intent, not a surprise.
struct Nearest {
    capture: f32,
    best: Option<(f32, Vec2, SnapKind)>,
    tangent: Option<(f32, Vec2)>,
}

impl Nearest {
    fn new(capture: f32) -> Self {
        Self {
            capture,
            best: None,
            tangent: None,
        }
    }

    fn consider(&mut self, dist: f32, pos: Vec2, kind: SnapKind) {
        if dist <= self.capture && self.best.is_none_or(|(d, _, _)| dist < d) {
            self.best = Some((dist, pos, kind));
        }
    }

    fn consider_tangent(&mut self, dist: f32, pos: Vec2) {
        if dist <= self.capture && self.tangent.is_none_or(|(d, _)| dist < d) {
            self.tangent = Some((dist, pos));
        }
    }

    fn winner(&self) -> Option<(Vec2, SnapKind)> {
        self.tangent
            .map(|(_, pos)| (pos, SnapKind::Tangent))
            .or(self.best.map(|(_, pos, kind)| (pos, kind)))
    }
}

/// Computes [`SnappedCursor`] each frame.
///
/// Object snaps win over the grid: a tangent (only while a
/// [`SnapReference`] is set) first, then vertex > midpoint > center > edge
/// by proximity; the capture radius is screen-constant (scales with zoom).
pub fn update_snapped_cursor(
    cursor: Res<CursorWorldPos>,
    grid: Res<GridSettings>,
    config: Res<SnapConfig>,
    exclusions: Res<SnapExclusions>,
    reference: Res<SnapReference>,
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
        let mut nearest = Nearest::new(radius);
        let line_from = match reference.0 {
            Some(DraftReference::Line(q)) => Some(q),
            _ => None,
        };
        let pad = Vec2::splat(radius);
        for entity in physics.bodies_in_aabb(p - pad * 8.0, p + pad * 8.0) {
            if exclusions.0.contains(&entity) {
                continue;
            }
            let Ok((shape, transform)) = bodies.get(entity) else {
                continue;
            };
            body_candidates(&mut nearest, p, shape, transform, &config, line_from);
        }
        if let Some(DraftReference::Radius(center)) = reference.0 {
            // Any body the growing circle could touch at this radius, not
            // only those near the cursor: the touch point can be anywhere
            // around the circle.
            let reach = Vec2::splat(center.distance(p) + radius);
            for entity in physics.bodies_in_aabb(center - reach, center + reach) {
                if exclusions.0.contains(&entity) {
                    continue;
                }
                let Ok((shape, transform)) = bodies.get(entity) else {
                    continue;
                };
                tangent_radius_candidate(&mut nearest, p, center, shape, transform);
            }
        }

        if let Some((pos, kind)) = nearest.winner() {
            out.position = Some(pos);
            out.kind = Some(kind);
            return;
        }
    }

    // --- Grid snap. ---
    // Grid snapping only applies when the grid is visible (invisible
    // magnetism is disorienting), and it snaps to the step the grid is
    // *drawn* at: the zoom-adapted step, from the same `CameraScale` the
    // renderer reads — never the raw authored spacing.
    // A degenerate spacing draws no grid, so it snaps to none either.
    if grid.snap_enabled
        && grid.visible
        && let Some(step) = adaptive_step(grid.spacing, scale)
    {
        let snapped = match grid.system {
            GridSystem::Cartesian => cartesian_snap(p, grid.origin, grid.rotation, step),
            GridSystem::Isometric => isometric_snap(p, grid.origin, grid.rotation, step),
            GridSystem::Polar { angular_divisions } => {
                polar_snap(p, grid.origin, grid.rotation, step, angular_divisions)
            }
        };
        out.position = Some(snapped);
        out.kind = Some(SnapKind::Grid);
    }
}

/// Offers one body's outline features (and, with a line reference, its
/// tangent points) to `nearest`.
fn body_candidates(
    nearest: &mut Nearest,
    p: Vec2,
    shape: &ShapeDef,
    transform: &Transform,
    config: &SnapConfig,
    line_from: Option<Vec2>,
) {
    let capture = nearest.capture;
    if config.sources.centers {
        let c = transform.translation.truncate();
        nearest.consider(c.distance(p), c, SnapKind::Center);
    }
    let outline_sources =
        config.sources.vertices || config.sources.midpoints || config.sources.edges;
    if !outline_sources && line_from.is_none() {
        return;
    }
    match snap_outline(shape) {
        // An exact circle: its real features only. Tessellation
        // vertices and midpoints are artifacts a few pixels apart
        // and made the glyph flicker along the curve (3.4).
        outline @ SnapOutline::Circle {
            center, radius: r, ..
        } => {
            if config.sources.vertices
                && let Some(quadrants) = outline.circle_quadrants()
            {
                for q in quadrants {
                    let a = world_point(transform, q);
                    nearest.consider(a.distance(p), a, SnapKind::Vertex);
                }
            }
            // Work in the shape's frame, where it is a circle.
            if config.sources.edges {
                let local = local_point(transform, p);
                let e = world_point(transform, nearest_on_circle(local, center, r));
                nearest.consider(e.distance(p) + capture * EDGE_TIE_BIAS, e, SnapKind::Edge);
            }
            if let Some(q) = line_from
                && let Some(points) = tangent_points(local_point(transform, q), center, r)
            {
                for t in points {
                    let t = world_point(transform, t);
                    nearest.consider_tangent(t.distance(p), t);
                }
            }
        }
        SnapOutline::Contours(contours) => {
            for ring in contours.rings() {
                let n = ring.len();
                for i in 0..n {
                    let a = world_point(transform, ring[i]);
                    if config.sources.vertices {
                        nearest.consider(a.distance(p), a, SnapKind::Vertex);
                    }
                    let b = world_point(transform, ring[(i + 1) % n]);
                    if config.sources.midpoints {
                        let m = (a + b) / 2.0;
                        nearest.consider(m.distance(p), m, SnapKind::Midpoint);
                    }
                    if config.sources.edges {
                        let e = project_on_segment(p, a, b);
                        nearest.consider(
                            e.distance(p) + capture * EDGE_TIE_BIAS,
                            e,
                            SnapKind::Edge,
                        );
                    }
                }
            }
        }
    }
}

/// Offers the touch point of a circle about `center` grown until it meets
/// this body, scored by how far the cursor's radius is from that radius —
/// so the snap holds anywhere around the circle, not only near the touch.
fn tangent_radius_candidate(
    nearest: &mut Nearest,
    p: Vec2,
    center: Vec2,
    shape: &ShapeDef,
    transform: &Transform,
) {
    let outline = snap_outline(shape);
    let Some(touch) = tangent_radius(local_point(transform, center), &outline) else {
        return;
    };
    let t = world_point(transform, touch.point);
    let off = (center.distance(p) - center.distance(t)).abs();
    nearest.consider_tangent(off, t);
}

fn local_point(transform: &Transform, world: Vec2) -> Vec2 {
    transform
        .compute_affine()
        .inverse()
        .transform_point3(world.extend(0.0))
        .truncate()
}

fn world_point(transform: &Transform, local: Vec2) -> Vec2 {
    transform
        .compute_affine()
        .transform_point3(local.extend(0.0))
        .truncate()
}
