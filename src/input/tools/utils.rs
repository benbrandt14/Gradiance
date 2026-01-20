//! Shared utilities for tools.

use crate::input::cursor::CursorWorldPos;
use crate::ui::grid::{snap_to_grid, GridSettings};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

/// Calculates the local position of a point relative to a body's transform.
///
/// This transforms a point in world space into the local space of the given transform,
/// accounting for both translation and rotation (assuming 2D rotation around Z).
pub fn calculate_local_anchor(transform: &Transform, world_point: Vec2) -> Vec2 {
    let rotation = transform.rotation.to_euler(EulerRot::XYZ).2;
    let translation = transform.translation.truncate();
    let relative = world_point - translation;

    let cos = rotation.cos();
    let sin = rotation.sin();

    // Rotate relative vector by -rotation (inverse rotation)
    // R^-1 = R^T
    // [ cos  sin ] [ x ]   [ x cos + y sin ]
    // [ -sin cos ] [ y ]   [ -x sin + y cos ]
    Vec2::new(
        relative.x * cos + relative.y * sin,
        -relative.x * sin + relative.y * cos,
    )
}

/// Checks if the mouse pointer is currently over an Egui area (window/panel).
///
/// Returns true if the pointer is over an Egui area, false otherwise.
/// Returns false if Egui context is not available (e.g. headless tests).
pub fn is_pointer_over_ui(contexts: &mut EguiContexts) -> bool {
    if let Some(ctx) = contexts.try_ctx_mut() {
        ctx.is_pointer_over_area()
    } else {
        false
    }
}

/// The status of a drag operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragStatus {
    /// No drag is currently active or happening.
    None,
    /// A drag has just started (Left mouse button just pressed).
    Started,
    /// A drag is in progress (Left mouse button held).
    Dragging,
    /// A drag has just finished (Left mouse button just released).
    Finished,
}

/// The result of processing input for a drag tool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragResult {
    /// The drag start position (snapped if grid is enabled).
    pub start: Vec2,
    /// The current cursor position (snapped if grid is enabled).
    pub current: Vec2,
    /// The current status of the drag.
    pub status: DragStatus,
}

impl DragResult {
    /// Returns true if the drag is active (Started or Dragging).
    pub fn is_dragging(&self) -> bool {
        matches!(self.status, DragStatus::Started | DragStatus::Dragging)
    }

    /// Returns true if the drag just finished.
    pub fn is_finished(&self) -> bool {
        matches!(self.status, DragStatus::Finished)
    }
}

/// Handles common input logic for drag-based creation tools (e.g. Box, Circle).
///
/// This function:
/// 1. Checks if the pointer is over UI (blocks input).
/// 2. Gets the cursor position.
/// 3. Applies grid snapping.
/// 4. Manages the `drag_start` state based on mouse input.
pub fn handle_drag_input(
    // Inputs
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    grid_settings: Res<GridSettings>,
    mut contexts: EguiContexts,
    // State
    drag_start: &mut Option<Vec2>,
) -> Option<DragResult> {
    // If not dragging, check UI block
    if drag_start.is_none() && is_pointer_over_ui(&mut contexts) {
        return None;
    }

    let raw_pos = cursor_pos.0?;

    let mut current_pos = raw_pos;
    if grid_settings.show && grid_settings.snap {
        current_pos = snap_to_grid(current_pos, grid_settings.spacing);
    }

    // State Transition Logic
    if let Some(start) = *drag_start {
        // Currently Dragging
        if mouse.just_released(MouseButton::Left) {
            *drag_start = None;
            Some(DragResult {
                start,
                current: current_pos,
                status: DragStatus::Finished,
            })
        } else if mouse.pressed(MouseButton::Left) {
            Some(DragResult {
                start,
                current: current_pos,
                status: DragStatus::Dragging,
            })
        } else {
            // Mouse not pressed and not just released? (e.g. lost focus or released outside)
            // Treat as cancelled or just reset.
            *drag_start = None;
            None
        }
    } else {
        // Not Dragging
        if mouse.just_pressed(MouseButton::Left) {
            *drag_start = Some(current_pos);
            Some(DragResult {
                start: current_pos,
                current: current_pos,
                status: DragStatus::Started,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        Transform::from_translation(Vec3::new(10.0, 10.0, 0.0)),
        Vec2::new(15.0, 15.0),
        Vec2::new(5.0, 5.0)
    )]
    #[case(
        // Rotated 90 degrees around Z (counter-clockwise)
        Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, -1.0)
    )]
    fn test_calculate_local_anchor(
        #[case] transform: Transform,
        #[case] world_point: Vec2,
        #[case] expected: Vec2,
    ) {
        let result = calculate_local_anchor(&transform, world_point);
        assert!((result.x - expected.x).abs() < 1e-5);
        assert!((result.y - expected.y).abs() < 1e-5);
    }
}
