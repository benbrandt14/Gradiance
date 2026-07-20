//! Bounding-box scale handles for the selection.
//!
//! Eight grabbable handles on the selection's bounding box — corner
//! handles scale both axes (hold `Shift` for uniform), edge-midpoint
//! handles scale one axis — anchored at the **opposite** handle (CAD
//! behavior). The box is computed in the active [`ScaleFrame`]:
//! `Global` = world axes, `Local` = the primary body's rotated axes
//! (toggle with `F`).

use crate::domain::Body;
use crate::domain::shape::ShapeDef;
use crate::geometry::polygonize::polygonize;
use crate::interaction::selection::Selection;
use crate::interaction::overlay::OverlayGizmos;
use bevy::color::palettes::css;
use bevy::prelude::*;

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
                crate::core::units::PosRot::from_transform(t).rot
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
    HandleKind::ALL
        .into_iter()
        .find(|h| sbox.point(h.unit()).distance(cursor) <= radius)
}

/// Draws the selection box and its handles.
pub fn draw_handles(
    selection: Res<Selection>,
    frame: Res<ScaleFrame>,
    bodies: Query<(&ShapeDef, &Transform), With<Body>>,
    cam_scale: Res<crate::interaction::camera::CameraScale>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    let Some(sbox) = selection_box(&selection, *frame, &bodies) else {
        return;
    };
    let scale = cam_scale.0;
    let size = 5.0 * scale;
    let color = if *frame == ScaleFrame::Local {
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

    // Handles.
    for handle in HandleKind::ALL {
        let p = sbox.point(handle.unit());
        gizmos.rect_2d(
            Isometry2d::new(p, Rot2::radians(sbox.rot)),
            Vec2::splat(size * 2.0),
            color,
        );
    }
}
