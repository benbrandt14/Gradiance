//! Debug overlays: gizmo visualizations of editor/physics internals,
//! toggled from the Debug settings tab ([`DebugSettings`]).
//!
//! Everything here reads authored components, the pure geometry layer,
//! and the `physics::queries` read facade.

use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_core::ids::IdIndex;
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::field::FieldSource;
use gradiance_domain::joint::{JointDef, JointKind};
use gradiance_domain::layers::layer_hue;
use gradiance_domain::settings::DebugSettings;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::polygonize::polygonize;
use gradiance_geometry::sdf;
use gradiance_interaction::overlay::OverlayGizmos;
use gradiance_physics::SubstepTrace;
use gradiance_physics::fields::Fields;
use gradiance_physics::queries::PhysicsQueries;

fn world_point(transform: &Transform, local: Vec2) -> Vec2 {
    transform
        .compute_affine()
        .transform_point3(local.extend(0.0))
        .truncate()
}

/// Draws the enabled debug overlays.
pub fn draw_debug_overlays(
    debug: Res<DebugSettings>,
    scale: Res<gradiance_interaction::camera::CameraScale>,
    bodies: Query<(Entity, &ShapeDef, &Transform, &DepthBand), With<Body>>,
    field_sources: Query<(&Transform, &FieldSource), With<Body>>,
    fields: Fields,
    cameras: Query<(&Transform, &Projection), (With<Camera3d>, Without<Body>)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    substeps: Res<SubstepTrace>,
    joints: Query<&JointDef>,
    index: Res<IdIndex>,
    poses: Query<&Transform, With<Body>>,
    physics: PhysicsQueries,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    // Marker glyphs are sized in pixels; `px` (world-per-pixel) converts them
    // to world units so they stay a fixed on-screen size at any zoom. Sizes
    // derived from real geometry (rings, AABBs) and vectors proportional to a
    // physical quantity (velocity/field arrows) are already world-consistent.
    let px = scale.0;
    for body in &bodies {
        draw_body_overlays(&debug, &physics, &mut gizmos, body, px);
    }

    if debug.show_joint_anchors {
        draw_joint_anchors(&joints, &index, &poses, &mut gizmos, px);
    }

    for (transform, source) in &field_sources {
        draw_field_marker(transform, *source, &mut gizmos, px);
    }

    if debug.show_fields
        && let Ok((cam, projection)) = cameras.single()
    {
        draw_field_overlay(&fields, cam, projection, &windows, &mut gizmos);
    }

    if debug.show_substeps {
        // Ghost dots at each substep position of the last physics step,
        // brightening toward the final substep.
        let n = substeps.0.len().max(1) as f32;
        for (i, frame) in substeps.0.iter().enumerate() {
            let alpha = 0.25 + 0.75 * ((i + 1) as f32 / n);
            let color = css::MEDIUM_ORCHID.with_alpha(alpha);
            for pos in frame {
                gizmos.circle_2d(Isometry2d::from_translation(*pos), 2.5 * px, color);
            }
        }
    }

    if debug.show_contacts {
        draw_contacts(&physics, &mut gizmos, px);
    }
}

/// Per-body debug glyphs: collision set, collider contours, AABB, origin
/// cross/centroid, and velocity arrows. `px` is world-per-pixel so pixel-
/// sized markers stay a fixed on-screen size (geometry-derived outlines and
/// the quantity-proportional velocity arrow are already world-consistent).
fn draw_body_overlays(
    debug: &DebugSettings,
    physics: &PhysicsQueries,
    gizmos: &mut Gizmos<OverlayGizmos>,
    body: (Entity, &ShapeDef, &Transform, &DepthBand),
    px: f32,
) {
    let (entity, shape, transform, band) = body;
    let infinite = shape.contains_half_plane();

    // Collision-set visualization: outline in the front-most derived
    // layer's hue, matching the depth panel's gridline swatches.
    if debug.show_layers && !infinite {
        let color = Color::hsl(layer_hue(band.bits().trailing_zeros()), 0.75, 0.55);
        for ring in polygonize(shape).rings() {
            let mut pts: Vec<Vec2> = ring.iter().map(|v| world_point(transform, *v)).collect();
            if let Some(first) = pts.first().copied() {
                pts.push(first);
            }
            gizmos.linestrip_2d(pts, color);
        }
    }

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
        gizmos.line_2d(
            origin - Vec2::X * 6.0 * px,
            origin + Vec2::X * 6.0 * px,
            css::WHITE,
        );
        gizmos.line_2d(
            origin - Vec2::Y * 6.0 * px,
            origin + Vec2::Y * 6.0 * px,
            css::WHITE,
        );
        if !infinite {
            // Centroid may sit off-origin after CSG reshapes.
            let centroid = world_point(transform, polygonize(shape).centroid());
            gizmos.circle_2d(
                Isometry2d::from_translation(centroid),
                3.0 * px,
                css::HOT_PINK,
            );
        }
    }

    if debug.show_velocities
        && let Some((lin, ang)) = physics.velocity_of(entity)
    {
        let (lin, ang) = (lin.value(), ang.value());
        let origin = transform.translation.truncate();
        if physics.is_sleeping(entity) {
            gizmos.circle_2d(Isometry2d::from_translation(origin), 8.0 * px, css::GRAY);
        } else {
            // Velocity arrow is a world-space vector (both world and velocity
            // flipped by the same factor, so its screen size is preserved).
            gizmos.arrow_2d(origin, origin + lin * 0.25, css::YELLOW);
            if ang.abs() > 0.05 {
                // Angular velocity (rad/s, scale-invariant): an arc-ish tick
                // whose pixel radius grows with spin.
                gizmos.circle_2d(
                    Isometry2d::from_translation(origin),
                    (4.0 + ang.abs().min(20.0)) * px,
                    css::ORANGE,
                );
            }
        }
    }
}

/// Draws each joint's anchor glyphs and its body links.
fn draw_joint_anchors(
    joints: &Query<&JointDef>,
    index: &IdIndex,
    poses: &Query<&Transform, With<Body>>,
    gizmos: &mut Gizmos<OverlayGizmos>,
    px: f32,
) {
    for def in joints {
        let color = match def.kind {
            JointKind::Hinge { .. } => css::ORANGE,
            JointKind::Slider { .. } => css::DEEP_SKY_BLUE,
            JointKind::Spring { .. } => css::SPRING_GREEN,
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
            gizmos.circle_2d(Isometry2d::from_translation(a), 4.0 * px, color);
        }
        if let Some(b) = world_b {
            gizmos.circle_2d(Isometry2d::from_translation(b), 6.0 * px, color);
            if def.body_b.is_none() {
                let d = 4.0 * px;
                gizmos.line_2d(b - Vec2::splat(d), b + Vec2::splat(d), color);
                gizmos.line_2d(b + Vec2::new(-d, d), b + Vec2::new(d, -d), color);
            }
        }
        if let (Some(a), Some(b)) = (world_a, world_b) {
            gizmos.line_2d(a, b, color);
        }
    }
}

/// Draws every touching contact point and its impulse-scaled normal.
fn draw_contacts(physics: &PhysicsQueries, gizmos: &mut Gizmos<OverlayGizmos>, px: f32) {
    for contact in physics.contact_points() {
        gizmos.circle_2d(
            Isometry2d::from_translation(contact.point),
            2.5 * px,
            css::MAGENTA,
        );
        // Screen-sized base plus a term growing with the contact impulse
        // (N·s, SI — clamped so hard hits stay on-screen). The impulse gain
        // is approximate; this is a dev overlay, not a visually pinned value.
        let length = (8.0 + contact.normal_impulse.value().min(2.0) * 12.0) * px;
        gizmos.arrow_2d(
            contact.point,
            contact.point + contact.normal * length,
            css::MAGENTA,
        );
    }
}

/// Vector plot of the superposed field, sampled on a screen-space grid
/// around the camera — reads the SAME `Fields::accel_at` cut-point the
/// solver forces and set-in-orbit use, so what you see is what acts.
///
/// The grid tracks the viewport: sample spacing is fixed in *screen* pixels
/// (so zooming re-samples the field at the new scale) and the extent covers
/// the whole window at any zoom (arrow count bounded by the clamps).
fn draw_field_overlay(
    fields: &Fields,
    cam: &Transform,
    projection: &Projection,
    windows: &Query<&Window, With<bevy::window::PrimaryWindow>>,
    gizmos: &mut Gizmos<OverlayGizmos>,
) {
    let s = match projection {
        Projection::Orthographic(o) => o.scale,
        _ => 1.0,
    };
    let viewport = windows.single().map_or(Vec2::new(1280.0, 720.0), |w| {
        Vec2::new(w.width(), w.height())
    });
    let center = cam.translation.truncate();
    let spacing = 56.0 * s;
    let nx = ((viewport.x / 112.0).ceil() as i32 + 1).clamp(4, 40);
    let ny = ((viewport.y / 112.0).ceil() as i32 + 1).clamp(4, 30);
    for gy in -ny..=ny {
        for gx in -nx..=nx {
            let p = center + Vec2::new(gx as f32, gy as f32) * spacing;
            let a = fields.accel_at(p, None).value();
            let mag = a.length();
            // Acceleration is m/s² (SI); the skip floor and saturation knees
            // are ÷100 from the former px/s² values.
            if mag < 0.01 {
                continue;
            }
            // Saturating arrow length; warmth ramps with magnitude.
            let len = spacing * 0.42 * (mag / (mag + 8.0));
            let dir = a / mag;
            let tip = p + dir * len;
            let heat = (mag / (mag + 20.0)).clamp(0.0, 1.0);
            let color = css::STEEL_BLUE.mix(&css::ORANGE_RED, heat);
            gizmos.line_2d(p, tip, color);
            let perp = Vec2::new(-dir.y, dir.x);
            gizmos.line_2d(tip, tip - dir * len * 0.35 + perp * len * 0.2, color);
            gizmos.line_2d(tip, tip - dir * len * 0.35 - perp * len * 0.2, color);
        }
    }
}

/// A field-bearing body's glyph (authored-state feedback, not a debug
/// toggle): double ring at the origin, blue = attraction (negative
/// strength), red = repulsion.
fn draw_field_marker(
    transform: &Transform,
    source: FieldSource,
    gizmos: &mut Gizmos<OverlayGizmos>,
    px: f32,
) {
    let p = transform.translation.truncate();
    let color = if source.strength < 0.0 {
        css::CORNFLOWER_BLUE
    } else {
        css::INDIAN_RED
    };
    // Screen-sized double ring (5 px / 8 px).
    gizmos.circle_2d(Isometry2d::from_translation(p), 5.0 * px, color);
    gizmos.circle_2d(
        Isometry2d::from_translation(p),
        8.0 * px,
        color.with_alpha(0.5),
    );
}
