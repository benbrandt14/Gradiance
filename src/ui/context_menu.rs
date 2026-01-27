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
use rand::Rng;

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
    mut collision_groups: Query<&mut CollisionGroups>,
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

            // Collision Depth (Layers)
            ui.collapsing("Depth (Collision Layers)", |ui| {
                // Determine current range from selection (approximate from first entity)
                let (current_start, current_end) = if let Some(first) = selection.0.iter().next() {
                    if let Ok(groups) = collision_groups.get(*first) {
                        let bits = groups.memberships.bits();
                        let start = bits.trailing_zeros();
                        let end = 31 - bits.leading_zeros();
                        if start <= end && bits != 0 {
                            (start, end)
                        } else {
                            (0, 0)
                        }
                    } else {
                        (0, 0)
                    }
                } else {
                    (0, 0)
                };

                let mut start = current_start;
                let mut end = current_end;
                let mut changed = false;

                ui.horizontal(|ui| {
                    ui.label("Start Layer:");
                    if ui.add(egui::DragValue::new(&mut start).range(0..=31)).changed() {
                        changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("End Layer:  ");
                    if ui.add(egui::DragValue::new(&mut end).range(0..=31)).changed() {
                        changed = true;
                    }
                });

                // Ensure valid range
                if start > end {
                    end = start;
                }

                if changed {
                    let mut mask = 0u32;
                    for i in start..=end {
                        mask |= 1 << i;
                    }

                    let new_groups = Group::from_bits_truncate(mask);

                    for &entity in &selection.0 {
                        // Apply same mask to memberships AND filters to ensure self-collision
                        // and collision with others in this range.
                        if let Ok(mut groups) = collision_groups.get_mut(entity) {
                            groups.memberships = new_groups;
                            groups.filters = new_groups;
                        } else {
                             commands.entity(entity).insert(CollisionGroups::new(new_groups, new_groups));
                        }
                    }
                }

                if ui.button("Randomize Layers").clicked() {
                     let mut rng = rand::rng();
                     let range_size = end.saturating_sub(start) + 1;

                     for &entity in &selection.0 {
                         // Pick a random layer within the start..=end range
                         let random_offset = rng.random_range(0..range_size);
                         let layer = start + random_offset;
                         let mask = 1 << layer;
                         let new_groups = Group::from_bits_truncate(mask);

                         if let Ok(mut groups) = collision_groups.get_mut(entity) {
                             groups.memberships = new_groups;
                             groups.filters = new_groups;
                         } else {
                             commands.entity(entity).insert(CollisionGroups::new(new_groups, new_groups));
                         }
                     }
                }

                if ui.button("Distribute Layers").clicked() {
                    let mut entities: Vec<_> = selection.0.iter().copied().collect();
                    // Sort by Entity ID for deterministic distribution
                    entities.sort();

                    let count = entities.len();
                    if count > 1 {
                        let span = (end as f32 - start as f32).max(0.0);
                        let step = span / (count - 1) as f32;

                        for (i, entity) in entities.into_iter().enumerate() {
                            let layer = (start as f32 + i as f32 * step).round() as u32;
                            // Clamp to be safe, though math should hold
                            let layer = layer.clamp(0, 31);

                            let mask = 1 << layer;
                            let new_groups = Group::from_bits_truncate(mask);

                            if let Ok(mut groups) = collision_groups.get_mut(entity) {
                                groups.memberships = new_groups;
                                groups.filters = new_groups;
                            } else {
                                commands.entity(entity).insert(CollisionGroups::new(new_groups, new_groups));
                            }
                        }
                    } else if count == 1 {
                        // Single entity, just assign start
                         let layer = start;
                         let mask = 1 << layer;
                         let new_groups = Group::from_bits_truncate(mask);
                         if let Ok(mut groups) = collision_groups.get_mut(entities[0]) {
                             groups.memberships = new_groups;
                             groups.filters = new_groups;
                         } else {
                             commands.entity(entities[0]).insert(CollisionGroups::new(new_groups, new_groups));
                         }
                    }
                }
            });

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
