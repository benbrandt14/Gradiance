//! Gesture constraints: axis locks and rotation quantization.
//!
//! CAD-style modifiers, updated every frame from the keyboard and consumed
//! by tools during drag gestures (and by future gesture behaviors):
//!
//! | Modifier | Effect |
//! |---|---|
//! | hold `X` | translate along the basis' first axis only |
//! | hold `Y` | translate along the basis' second axis only |
//! | hold `X`+`Y` | dominant-axis lock (the axis with the larger projection wins) |
//! | hold `Ctrl` while rotating | quantize to `SnapConfig::rotation_step_deg` |
//!
//! **The basis follows the grid you can see** ([`GestureConstraints::basis`],
//! from [`grid_basis`]): while the grid is visible, the lock axes are its
//! line families rotated by `GridSettings::rotation` — a rotated Cartesian
//! grid locks along its rotated axes, an isometric grid along its 30°/150°
//! families. A polar grid has no straight families; it locks along its own
//! frame (spoke 0 and its perpendicular, i.e. world X/Y rotated by the grid
//! rotation). With the grid hidden the basis is world X/Y. Whether grid
//! *snapping* is on does not matter — the lock follows what is drawn, and
//! [`crate::indicators::draw_constraint_guides`] draws the same axes.
//!
//! A lock is the orthogonal projection of the delta onto the lock axis
//! (`geometry::snapping::project_onto_axis`), which is also the definition
//! for the non-orthogonal isometric basis; see `dominant_axis` there.
//!
//! `Shift` is deliberately *not* an axis modifier: it belongs to selection
//! (toggle / additive band) and uniform scale. It used to double as the
//! dominant-axis lock, which silently warped move deltas and draft corners
//! whenever the user held it for selection (feedback 1.1, 2.2).

use bevy::prelude::*;
use gradiance_domain::settings::{GridSettings, GridSystem, SnapConfig};
use gradiance_geometry::snapping::{
    CARTESIAN_LINE_DIRECTIONS, ISOMETRIC_LINE_DIRECTIONS, dominant_axis, project_onto_axis,
    quantize_angle, rotated_basis,
};

/// Active translation constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AxisConstraint {
    /// Unconstrained.
    #[default]
    Free,
    /// Along the basis' first axis only (world X on an unrotated grid).
    Horizontal,
    /// Along the basis' second axis only (world Y on an unrotated grid).
    Vertical,
    /// Whichever axis dominates the delta.
    Dominant,
}

/// The current gesture modifiers, as a resource so every tool (and any
/// future gesture behavior) shares one source of truth.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct GestureConstraints {
    /// Translation constraint.
    pub axis: AxisConstraint,
    /// The two unit lock axes (`X` → `basis[0]`, `Y` → `basis[1]`): the
    /// active grid's frame, from [`grid_basis`]. World X/Y by default.
    pub basis: [Vec2; 2],
    /// Quantize rotation gestures to the configured step.
    pub quantize_rotation: bool,
}

impl Default for GestureConstraints {
    fn default() -> Self {
        Self {
            axis: AxisConstraint::Free,
            basis: CARTESIAN_LINE_DIRECTIONS,
            quantize_rotation: false,
        }
    }
}

impl GestureConstraints {
    /// The axis the current constraint locks `delta` to, or `None` when
    /// unconstrained.
    pub fn lock_axis(&self, delta: Vec2) -> Option<Vec2> {
        match self.axis {
            AxisConstraint::Free => None,
            AxisConstraint::Horizontal => Some(self.basis[0]),
            AxisConstraint::Vertical => Some(self.basis[1]),
            AxisConstraint::Dominant => Some(dominant_axis(delta, self.basis)),
        }
    }

    /// Applies the axis constraint to a translation delta: its projection
    /// onto the lock axis.
    pub fn constrain(&self, delta: Vec2) -> Vec2 {
        self.lock_axis(delta)
            .map_or(delta, |axis| project_onto_axis(delta, axis))
    }

    /// Applies rotation quantization (radians) when active.
    pub fn apply_rotation(&self, angle: f32, config: &SnapConfig) -> f32 {
        if self.quantize_rotation {
            quantize_angle(angle, config.rotation_step_deg.to_radians())
        } else {
            angle
        }
    }
}

/// The lock basis for a grid: its line families rotated by its rotation
/// while it is visible; world X/Y while hidden. Polar grids (no straight
/// families) use the Cartesian directions in the grid's frame.
pub fn grid_basis(grid: &GridSettings) -> [Vec2; 2] {
    if !grid.visible {
        return CARTESIAN_LINE_DIRECTIONS;
    }
    let directions = match grid.system {
        GridSystem::Cartesian | GridSystem::Polar { .. } => CARTESIAN_LINE_DIRECTIONS,
        GridSystem::Isometric => ISOMETRIC_LINE_DIRECTIONS,
    };
    rotated_basis(directions, grid.rotation)
}

/// Reads modifier keys (and the grid's frame) into [`GestureConstraints`].
pub fn update_gesture_constraints(
    keys: Res<ButtonInput<KeyCode>>,
    grid: Res<GridSettings>,
    mut constraints: ResMut<GestureConstraints>,
) {
    let basis = grid_basis(&grid);
    if constraints.basis != basis {
        constraints.basis = basis;
    }
    constraints.axis = match (keys.pressed(KeyCode::KeyX), keys.pressed(KeyCode::KeyY)) {
        (true, true) => AxisConstraint::Dominant,
        (true, false) => AxisConstraint::Horizontal,
        (false, true) => AxisConstraint::Vertical,
        (false, false) => AxisConstraint::Free,
    };
    constraints.quantize_rotation =
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
}
