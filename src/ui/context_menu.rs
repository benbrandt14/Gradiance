//! Context menu system.
//!
//! Provides a right-click context menu for entities, allowing actions like deletion,
//! property inspection, and state toggling.

use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{cursor::CursorWorldPos, selection::Selection};
use crate::prelude::*;
use crate::ui::icons::GameIcons;
use bevy_egui::{egui, EguiContexts};

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
        app.add_systems(Update, (context_menu_input, context_menu_ui));
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
    rapier_context_query: Query<&RapierContext>,
    mut contexts: EguiContexts,
) {
    // If over area, don't trigger game context menu unless we are already showing it (logic handled later)
    if is_pointer_over_ui(&mut contexts) {
        if mouse.just_pressed(MouseButton::Right) {
            return;
        }
    }

    if mouse.just_pressed(MouseButton::Right) {
        let Some(world_pos) = cursor_pos.0 else {
            return;
        };
        let Some(rapier_context) = rapier_context_query.iter().next() else {
            return;
        };

        // Raycast to find entity
        let filter = QueryFilter::default().exclude_sensors();
        let mut hit_entity: Option<Entity> = None;

        rapier_context.intersections_with_point(world_pos, filter, |entity| {
            hit_entity = Some(entity);
            false
        });

        if let Some(entity) = hit_entity {
            selection.clear();
            selection.add(entity);

            let ctx = contexts.ctx_mut();
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
        if !is_pointer_over_ui(&mut contexts) {
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
    game_icons: Res<GameIcons>,
) {
    let Some(pos) = state.position else {
        return;
    };
    let Some(entity) = state.entity else {
        return;
    };

    let settings_icon = contexts.add_image(game_icons.settings.clone_weak());
    let delete_icon = contexts.add_image(game_icons.delete.clone_weak());
    let freeze_icon = contexts.add_image(game_icons.freeze.clone_weak());

    let ctx = contexts.ctx_mut();

    // We use a Window behaving like a popup
    egui::Window::new("Context Menu")
        .fixed_pos(pos)
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .frame(egui::Frame::popup(ctx.style().as_ref()))
        .show(ctx, |ui| {
            if ui
                .add(egui::Button::image_and_text(
                    (settings_icon, egui::Vec2::new(16.0, 16.0)),
                    "Properties",
                ))
                .clicked()
            {
                // Inspector handles selection.
                state.position = None;
            }
            if ui
                .add(egui::Button::image_and_text(
                    (delete_icon, egui::Vec2::new(16.0, 16.0)),
                    "Delete",
                ))
                .clicked()
            {
                commands.entity(entity).despawn_recursive();
                selection.remove(entity);
                state.position = None;
                state.entity = None;
            }
            if ui
                .add(egui::Button::image_and_text(
                    (freeze_icon, egui::Vec2::new(16.0, 16.0)),
                    "Freeze/Unfreeze",
                ))
                .clicked()
            {
                // Toggle static/dynamic logic would go here
            }
        });
}
