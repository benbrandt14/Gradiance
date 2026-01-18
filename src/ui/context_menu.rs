//! Context menu system.
//!
//! Provides a right-click context menu for entities.

use crate::input::{cursor::CursorWorldPos, selection::Selection};
use crate::prelude::*;
use bevy_egui::{EguiContexts, egui};

#[derive(Resource, Default)]
struct ContextMenuState {
    position: Option<egui::Pos2>,
    entity: Option<Entity>,
}

/// Plugin that handles the context menu logic and UI.
pub struct ContextMenuPlugin;

impl Plugin for ContextMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ContextMenuState>();
        app.add_systems(
            Update,
            (context_menu_input, context_menu_ui),
        );
    }
}

/// Handles input for opening and closing the context menu.
fn context_menu_input(
    mut state: ResMut<ContextMenuState>,
    mut selection: ResMut<Selection>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    rapier_context: Res<RapierContext>,
    mut contexts: EguiContexts,
) {
    let ctx = contexts.ctx_mut();

    if ctx.is_pointer_over_area() {
        if mouse.just_pressed(MouseButton::Right) {
            return;
        }
    }

    if mouse.just_pressed(MouseButton::Right) {
        let Some(world_pos) = cursor_pos.0 else {
            return;
        };

        // Raycast to find entity
        // Cast DVec2 to Vec2
        let point = Vec2::new(world_pos.x as f32, world_pos.y as f32);
        let filter = QueryFilter::default();
        if let Some((entity, _proj)) = rapier_context.project_point(point, true, filter) {
            selection.clear();
            selection.add(entity);

            if let Some(pointer_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                state.position = Some(pointer_pos);
                state.entity = Some(entity);
            }
        } else {
            // Clicked empty space
            state.position = None;
            state.entity = None;
        }
    } else if mouse.just_pressed(MouseButton::Left) {
        if !ctx.is_pointer_over_area() {
            state.position = None;
            state.entity = None;
        }
    }
}

/// Renders the context menu UI if active.
fn context_menu_ui(
    mut state: ResMut<ContextMenuState>,
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut selection: ResMut<Selection>,
) {
    let Some(pos) = state.position else { return };
    let Some(entity) = state.entity else { return };

    let ctx = contexts.ctx_mut();

    egui::Window::new("Context Menu")
        .fixed_pos(pos)
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .frame(egui::Frame::popup(ctx.style().as_ref()))
        .show(ctx, |ui| {
            if ui.button("Properties").clicked() {
                // Inspector handles selection.
                state.position = None;
            }
            if ui.button("Delete").clicked() {
                commands.entity(entity).despawn();
                selection.remove(entity);
                state.position = None;
                state.entity = None;
            }
        });
}
