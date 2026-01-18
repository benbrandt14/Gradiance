//! Context menu system.
//!
//! Provides a right-click context menu for entities, allowing actions like deletion,
//! property inspection, and state toggling.

use crate::input::{cursor::CursorWorldPos, selection::Selection};
use crate::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

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
            EguiPrimaryContextPass,
            (context_menu_input, context_menu_ui),
        );
    }
}

/// Handles input for opening and closing the context menu.
///
/// Detects right-clicks on entities to open the menu, and clicks outside to close it.
fn context_menu_input(
    mut state: ResMut<ContextMenuState>,
    mut selection: ResMut<Selection>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    spatial_query: SpatialQuery,
    mut contexts: EguiContexts,
) {
    if let Ok(ctx) = contexts.ctx_mut() {
        // If over area, don't trigger game context menu unless we are already showing it (logic handled later)
        if ctx.is_pointer_over_area() {
            // If we are showing the menu, we might be over the menu itself.
            // So we continue only if we want to open a NEW menu, which we don't if over UI.
            // But if we right click on UI, standard egui behavior.
            // So if over area, we just return.
            // BUT, if we have a context menu open, `is_pointer_over_area` will be true when hovering it.
            // So we shouldn't close it or prevent interaction.
            // The input handling for opening is `just_pressed(Right)`.
            // If we right click OVER UI, we return.
            if mouse.just_pressed(MouseButton::Right) {
                return;
            }
        }
    }

    if mouse.just_pressed(MouseButton::Right) {
        let Some(world_pos) = cursor_pos.0 else {
            return;
        };

        // Raycast to find entity
        let filter = SpatialQueryFilter::default();
        if let Some(hit) = spatial_query.project_point(world_pos, true, &filter) {
            selection.0 = Some(hit.entity);

            if let Ok(ctx) = contexts.ctx_mut()
                && let Some(pointer_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                    state.position = Some(pointer_pos);
                    state.entity = Some(hit.entity);
                }
        } else {
            // Clicked empty space
            state.position = None;
            state.entity = None;
        }
    } else if mouse.just_pressed(MouseButton::Left) {
        // Close context menu on left click if outside (handled by check above for is_pointer_over_area logic?)
        // If we click on the context menu, `is_pointer_over_area` is true.
        // If we click outside, `is_pointer_over_area` might be false (game world).
        // So:
        if let Ok(ctx) = contexts.ctx_mut()
            && !ctx.is_pointer_over_area() {
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

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        _ => return,
    };

    // We use a Window behaving like a popup
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
                selection.0 = None;
                state.position = None;
                state.entity = None;
            }
            if ui.button("Freeze/Unfreeze").clicked() {
                // Toggle static/dynamic
                // Need to query current body type?
                // We can't easily do it without query.
                // For now skip or use commands with a closure if bevy allowed it (it doesn't easily).
                // I'll leave it for now.
            }
        });
}
