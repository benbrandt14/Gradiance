//! Context menu system.
//!
//! Provides a right-click context menu for entities, allowing actions like deletion,
//! grouping, and ungrouping.

use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{
    cursor::CursorWorldPos,
    selection::{NextGroupID, Selection, SelectionGroup},
};
use crate::prelude::*;
use crate::ui::icons::GameIcons;
use bevy_egui::{EguiContexts, egui};

/// State for the context menu.
#[derive(Resource, Default)]
pub struct ContextMenuState {
    /// The screen position where the context menu was opened.
    pub position: Option<egui::Pos2>,
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
fn context_menu_input(
    mut state: ResMut<ContextMenuState>,
    mut selection: ResMut<Selection>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    rapier_context_query: Query<&RapierContext>,
    mut contexts: EguiContexts,
) {
    if is_pointer_over_ui(&mut contexts) && mouse.just_pressed(MouseButton::Right) {
        return;
    }

    if mouse.just_pressed(MouseButton::Right) {
        let Some(world_pos) = cursor_pos.0 else {
            return;
        };
        let Some(rapier_context) = rapier_context_query.iter().next() else {
            return;
        };

        let filter = QueryFilter::default().exclude_sensors();
        let mut hit_entity: Option<Entity> = None;

        rapier_context.intersections_with_point(world_pos, filter, |entity| {
            hit_entity = Some(entity);
            false
        });

        if let Some(entity) = hit_entity {
            if !selection.0.contains(&entity) {
                selection.clear();
                selection.add(entity);
            }
            let ctx = contexts.ctx_mut();
            if let Some(pointer_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                state.position = Some(pointer_pos);
            }
        } else {
            state.position = None;
        }
    } else if mouse.just_pressed(MouseButton::Left) && !is_pointer_over_ui(&mut contexts) {
        state.position = None;
    }
}

/// Renders the context menu UI if active.
fn context_menu_ui(
    mut state: ResMut<ContextMenuState>,
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut selection: ResMut<Selection>,
    mut next_group_id: ResMut<NextGroupID>,
    game_icons: Res<GameIcons>,
) {
    let Some(pos) = state.position else {
        return;
    };

    if selection.0.is_empty() {
        return;
    }

    let delete_icon = contexts.add_image(game_icons.delete.clone_weak());

    let ctx = contexts.ctx_mut();

    egui::Window::new("Context Menu")
        .fixed_pos(pos)
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .frame(egui::Frame::popup(ctx.style().as_ref()))
        .show(ctx, |ui| {
            // Group
            if ui.button("Group").clicked() {
                let id = next_group_id.0;
                next_group_id.0 += 1;
                let mut count = 0;
                for &entity in &selection.0 {
                    commands.entity(entity).insert(SelectionGroup(id));
                    count += 1;
                }
                if count > 0 {
                    info!("Grouped {} entities into Group {}", count, id);
                }
                state.position = None;
            }

            // Ungroup
            if ui.button("Ungroup").clicked() {
                let mut count = 0;
                for &entity in &selection.0 {
                    commands.entity(entity).remove::<SelectionGroup>();
                    count += 1;
                }
                if count > 0 {
                    info!("Ungrouped {} entities", count);
                }
                state.position = None;
            }

            ui.separator();

            // Delete
            if ui
                .add(egui::Button::image_and_text(
                    (delete_icon, egui::Vec2::new(16.0, 16.0)),
                    "Delete",
                ))
                .clicked()
            {
                for entity in selection.0.drain() {
                    commands.entity(entity).despawn_recursive();
                }
                state.position = None;
            }
        });
}
