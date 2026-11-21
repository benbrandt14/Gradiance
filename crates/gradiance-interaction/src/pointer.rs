//! Pointer button edges, tracked independently of Bevy's frame-cleared
//! `just_pressed` (whose lifetime ends in `PreUpdate`, before tools run —
//! and which cannot be injected reliably in headless tests). Tools use
//! this resource exclusively.

use bevy::prelude::*;

/// Screen-space deadzone (logical px) that separates a right **click** from
/// a right **drag**: a release within this many pixels of its press is a
/// click (the context menu opens); motion past it engages the right-button
/// gestures instead (select-tool rotate, camera pan). One number so the
/// three right-button consumers can never disagree about which one owns
/// the gesture.
pub const RIGHT_CLICK_DEADZONE_PX: f32 = 8.0;

#[derive(Default, Debug, Clone, Copy)]
struct ButtonEdges {
    current: bool,
    previous: bool,
}

impl ButtonEdges {
    fn advance(&mut self, now: bool) {
        self.previous = self.current;
        self.current = now;
    }
}

/// Edge-tracked pointer buttons for tool gestures.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PointerButtons {
    left: ButtonEdges,
    right: ButtonEdges,
}

impl PointerButtons {
    fn edges(self, button: MouseButton) -> ButtonEdges {
        match button {
            MouseButton::Right => self.right,
            _ => self.left,
        }
    }

    /// Held this frame.
    pub fn pressed(&self, button: MouseButton) -> bool {
        self.edges(button).current
    }

    /// Went down this frame.
    pub fn just_pressed(&self, button: MouseButton) -> bool {
        let e = self.edges(button);
        e.current && !e.previous
    }

    /// Went up this frame.
    pub fn just_released(&self, button: MouseButton) -> bool {
        let e = self.edges(button);
        !e.current && e.previous
    }
}

/// Advances the edge tracker from the raw input state (`PreUpdate`).
pub fn update_pointer_buttons(
    input: Res<ButtonInput<MouseButton>>,
    mut pointer: ResMut<PointerButtons>,
) {
    pointer.left.advance(input.pressed(MouseButton::Left));
    pointer.right.advance(input.pressed(MouseButton::Right));
}
