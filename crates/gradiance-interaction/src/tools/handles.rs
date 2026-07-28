//! Bounding-box scale handles for the selection.
//!
//! Eight grabbable handles on the selection's bounding box — corner
//! handles scale both axes (hold `Shift` for uniform), edge-midpoint
//! handles scale one axis — anchored at the **opposite** handle (CAD
//! behavior). The box is computed in the active [`ScaleFrame`]:
//! `Global` = world axes, `Local` = the primary body's rotated axes
//! (toggle with `F`).

use crate::overlay::OverlayGizmos;
use crate::selection::Selection;
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::polygonize::polygonize;

/// Which coordinate system the selection box (and thus scaling) uses.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleFrame {
    /// World axes.
    #[default]
    Global,
    /// The primary (first-selected) body's axes.
    Local,
}

/// One of the eight scale handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    /// Corner `(sx, sy)` with `s* ∈ {-1, +1}`: scales both axes.
    Corner(i8, i8),
    /// Left/right edge midpoint: scales the frame X axis only.
    EdgeX(i8),
    /// Bottom/top edge midpoint: scales the frame Y axis only.
    EdgeY(i8),
}

impl HandleKind {
    /// All eight handles.
    pub const ALL: [Self; 8] = [
        Self::Corner(-1, -1),
        Self::Corner(1, -1),
        Self::Corner(1, 1),
        Self::Corner(-1, 1),
        Self::EdgeX(-1),
        Self::EdgeX(1),
        Self::EdgeY(-1),
        Self::EdgeY(1),
    ];

    /// Whether this is a corner (both axes) rather than an edge midpoint.
    pub fn is_corner(&self) -> bool {
        matches!(self, Self::Corner(_, _))
    }

    /// The handle's normalized box position (`-1..1` per axis).
    pub fn unit(&self) -> Vec2 {
        match self {
            Self::Corner(x, y) => Vec2::new(f32::from(*x), f32::from(*y)),
            Self::EdgeX(x) => Vec2::new(f32::from(*x), 0.0),
            Self::EdgeY(y) => Vec2::new(0.0, f32::from(*y)),
        }
    }

    /// The anchor (fixed point) for this handle: the opposite handle.
    pub fn anchor_unit(&self) -> Vec2 {
        -self.unit()
    }

    /// Which axes this handle scales.
    pub fn scales(&self) -> (bool, bool) {
        match self {
            Self::Corner(..) => (true, true),
            Self::EdgeX(_) => (true, false),
            Self::EdgeY(_) => (false, true),
        }
    }
}

/// The selection's oriented bounding box in the active frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionBox {
    /// Box center, world space.
    pub center: Vec2,
    /// Frame rotation (radians).
    pub rot: f32,
    /// Half extents along the frame axes.
    pub half: Vec2,
}

impl SelectionBox {
    /// World position of a normalized box point (`-1..1` per axis).
    pub fn point(&self, unit: Vec2) -> Vec2 {
        self.center + Vec2::from_angle(self.rot).rotate(unit * self.half)
    }

    /// Converts a world point into frame coordinates relative to `origin`
    /// (another world point).
    pub fn to_frame(&self, world: Vec2, origin: Vec2) -> Vec2 {
        Vec2::from_angle(-self.rot).rotate(world - origin)
    }
}

/// Computes the selection's bounding box in the active frame, if anything
/// scalable is selected. Half-plane grounds are excluded (infinite).
pub fn selection_box(
    selection: &Selection,
    frame: ScaleFrame,
    bodies: &Query<(&ShapeDef, &Transform), With<Body>>,
) -> Option<SelectionBox> {
    let rot = match frame {
        ScaleFrame::Global => 0.0,
        ScaleFrame::Local => selection
            .primary()
            .and_then(|e| bodies.get(e).ok())
            .map_or(0.0, |(_, t)| {
                gradiance_core::units::PosRot::from_transform(t).rot
            }),
    };
    let inv = Vec2::from_angle(-rot);
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    let mut any = false;
    for entity in selection.iter() {
        let Ok((shape, transform)) = bodies.get(entity) else {
            continue;
        };
        if shape.contains_half_plane() {
            continue;
        }
        let affine = transform.compute_affine();
        for ring in polygonize(shape).rings() {
            for v in ring {
                let w = affine.transform_point3(v.extend(0.0)).truncate();
                let f = inv.rotate(w);
                min = min.min(f);
                max = max.max(f);
                any = true;
            }
        }
    }
    if !any {
        return None;
    }
    let center_f = (min + max) / 2.0;
    Some(SelectionBox {
        center: Vec2::from_angle(rot).rotate(center_f),
        rot,
        half: (max - min) / 2.0,
    })
}

/// Finds the handle under `cursor`, if any (world-space `radius`).
pub fn hit_handle(sbox: &SelectionBox, cursor: Vec2, radius: f32) -> Option<HandleKind> {
    // Nearest, not first-in-list. The capture radius is a fixed number of
    // *pixels*, so on a small or zoomed-out selection several handles are in
    // range at once; taking the first match then hands back whichever
    // happens to lead `ALL`, which is why grabbing a corner used to work
    // sometimes and give you an edge the rest of the time.
    //
    // Corners win ties, because a corner is the more specific request: it
    // asks for both axes, and an edge is always available a few pixels away.
    HandleKind::ALL
        .into_iter()
        .filter(|h| sbox.point(h.unit()).distance(cursor) <= radius)
        .min_by(|a, b| {
            let (da, db) = (
                sbox.point(a.unit()).distance_squared(cursor),
                sbox.point(b.unit()).distance_squared(cursor),
            );
            da.total_cmp(&db)
                .then_with(|| b.is_corner().cmp(&a.is_corner()))
        })
}

/// Draws the selection box and its handles.
pub fn draw_handles(
    selection: Res<Selection>,
    frame: Res<ScaleFrame>,
    keys: Res<ButtonInput<KeyCode>>,
    keyboard_captured: Res<crate::KeyboardCaptured>,
    bodies: Query<(&ShapeDef, &Transform), With<Body>>,
    cam_scale: Res<crate::camera::CameraScale>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    let Some(sbox) = selection_box(&selection, *frame, &bodies) else {
        return;
    };
    let scale = cam_scale.0;
    let size = 5.0 * scale;
    // Holding the copy modifier repaints the handles: a drag from here will
    // repeat the selection rather than resize it, and that is worth knowing
    // *before* committing to the gesture rather than after.
    let copying = !keyboard_captured.0
        && (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight));
    let color = if copying {
        css::SPRING_GREEN
    } else if *frame == ScaleFrame::Local {
        css::MEDIUM_PURPLE
    } else {
        css::LIGHT_SKY_BLUE
    };

    // Box outline.
    let corners = [
        sbox.point(Vec2::new(-1.0, -1.0)),
        sbox.point(Vec2::new(1.0, -1.0)),
        sbox.point(Vec2::new(1.0, 1.0)),
        sbox.point(Vec2::new(-1.0, 1.0)),
        sbox.point(Vec2::new(-1.0, -1.0)),
    ];
    gizmos.linestrip_2d(corners, color.with_alpha(0.7));

    // Handles. In copy mode each one gains a ghost square offset outward —
    // the icon *is* the operation: one box becoming two, in the direction
    // the drag will lay them down. It follows the frame's rotation, so a
    // local-frame selection shows the offset along its own axes.
    let along = Vec2::from_angle(sbox.rot);
    for handle in HandleKind::ALL {
        let p = sbox.point(handle.unit());
        let iso = Isometry2d::new(p, Rot2::radians(sbox.rot));
        gizmos.rect_2d(iso, Vec2::splat(size * 2.0), color);
        if copying {
            let unit = handle.unit();
            // Edge handles offset along their own axis; corners along both.
            let dir = along.rotate(if unit == Vec2::ZERO { Vec2::X } else { unit });
            let ghost = p + dir.normalize_or_zero() * size * 2.2;
            gizmos.rect_2d(
                Isometry2d::new(ghost, Rot2::radians(sbox.rot)),
                Vec2::splat(size * 1.6),
                color.with_alpha(0.55),
            );
        }
    }
}
