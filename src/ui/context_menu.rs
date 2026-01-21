//! Context menu system.
//!
//! Provides a right-click context menu for entities, allowing actions like deletion,
//! property inspection, and state toggling.

use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{cursor::CursorWorldPos, selection::Selection};
use crate::prelude::*;
use crate::ui::icons::GameIcons;
use bevy_egui::{EguiContexts, egui};
use bevy_prototype_lyon::prelude::Fill;

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
    } else if mouse.just_pressed(MouseButton::Left)
        && !is_pointer_over_ui(&mut contexts) {
            state.position = None;
        }
}

/// Renders the context menu UI if active.
fn context_menu_ui(
    mut state: ResMut<ContextMenuState>,
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut selection: ResMut<Selection>,
    game_icons: Res<GameIcons>,
    physics_q: Query<(
        Option<&RigidBody>,
        Option<&Sensor>,
        Option<&GravityScale>,
        Option<&LockedAxes>,
        Option<&Sleeping>,
    )>,
    mut fill_q: Query<&mut Fill>,
) {
    let Some(pos) = state.position else {
        return;
    };

    if selection.0.is_empty() {
        return;
    }

    let settings_icon = contexts.add_image(game_icons.settings.clone_weak());
    let delete_icon = contexts.add_image(game_icons.delete.clone_weak());

    let ctx = contexts.ctx_mut();

    egui::Window::new("Context Menu")
        .fixed_pos(pos)
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .frame(egui::Frame::popup(ctx.style().as_ref()))
        .show(ctx, |ui| {
            // 1. Properties (Inspector)
            if ui
                .add(egui::Button::image_and_text(
                    (settings_icon, egui::Vec2::new(16.0, 16.0)),
                    "Properties",
                ))
                .clicked()
            {
                state.position = None;
            }

            ui.separator();

            // 2. Color Submenu
            ui.menu_button("Color", |ui| {
                ui.set_min_width(100.0);

                let mut color_to_set = None;

                if ui.button("Red").clicked() { color_to_set = Some(Color::srgb(1.0, 0.0, 0.0)); ui.close_menu(); }
                if ui.button("Green").clicked() { color_to_set = Some(Color::srgb(0.0, 1.0, 0.0)); ui.close_menu(); }
                if ui.button("Blue").clicked() { color_to_set = Some(Color::srgb(0.0, 0.0, 1.0)); ui.close_menu(); }
                if ui.button("Yellow").clicked() { color_to_set = Some(Color::srgb(1.0, 1.0, 0.0)); ui.close_menu(); }
                if ui.button("Cyan").clicked() { color_to_set = Some(Color::srgb(0.0, 1.0, 1.0)); ui.close_menu(); }
                if ui.button("Magenta").clicked() { color_to_set = Some(Color::srgb(1.0, 0.0, 1.0)); ui.close_menu(); }
                if ui.button("White").clicked() { color_to_set = Some(Color::WHITE); ui.close_menu(); }
                if ui.button("Black").clicked() { color_to_set = Some(Color::BLACK); ui.close_menu(); }

                ui.separator();

                // Custom Color Picker
                let mut current_color = Color::BLACK;
                if let Some(&first_e) = selection.0.iter().next() {
                    if let Ok(fill) = fill_q.get(first_e) {
                         current_color = fill.color;
                    }
                }

                let mut color_arr = current_color.to_srgba().to_f32_array();
                if ui.color_edit_button_rgba_unmultiplied(&mut color_arr).changed() {
                    let new_color = Color::srgba(color_arr[0], color_arr[1], color_arr[2], color_arr[3]);
                    color_to_set = Some(new_color);
                }

                if let Some(c) = color_to_set {
                    for &e in &selection.0 {
                        if let Ok(mut fill) = fill_q.get_mut(e) {
                            fill.color = c;
                        }
                    }
                }
            });

            ui.separator();

            // 3. Physics Submenu
            ui.menu_button("Physics", |ui| {
                ui.set_min_width(120.0);

                // Body Type
                ui.label("Body Type:");
                if ui.button("Dynamic").clicked() {
                    for &e in &selection.0 {
                        commands.entity(e).insert(RigidBody::Dynamic).insert(Sleeping::disabled());
                    }
                    ui.close_menu();
                }
                if ui.button("Fixed").clicked() {
                    for &e in &selection.0 {
                        commands.entity(e).insert(RigidBody::Fixed).insert(Sleeping::disabled());
                    }
                    ui.close_menu();
                }
                if ui.button("Kinematic").clicked() {
                    for &e in &selection.0 {
                        commands.entity(e).insert(RigidBody::KinematicPositionBased).insert(Sleeping::disabled());
                    }
                    ui.close_menu();
                }

                ui.separator();

                // Sensor (Toggle based on first entity)
                let first = selection.0.iter().next().unwrap();
                let is_sensor = if let Ok((_, sensor, _, _, _)) = physics_q.get(*first) {
                    sensor.is_some()
                } else {
                    false
                };

                if ui.checkbox(&mut {is_sensor}, "Sensor").clicked() {
                     if is_sensor {
                         // Was sensor, now remove (toggle OFF)
                         for &e in &selection.0 {
                             commands.entity(e).remove::<Sensor>();
                         }
                     } else {
                         // Was not sensor, now add (toggle ON)
                         for &e in &selection.0 {
                             commands.entity(e).insert(Sensor);
                         }
                     }
                }

                // Lock Rotation
                let is_locked = if let Ok((_, _, _, locked, _)) = physics_q.get(*first) {
                    locked.map(|l| l.contains(LockedAxes::ROTATION_LOCKED)).unwrap_or(false)
                } else {
                    false
                };

                if ui.checkbox(&mut {is_locked}, "Lock Rotation").clicked() {
                    for &e in &selection.0 {
                        if is_locked {
                             // Was locked, now unlock
                             commands.entity(e).insert(LockedAxes::empty());
                        } else {
                             // Was unlocked, now lock
                             commands.entity(e).insert(LockedAxes::ROTATION_LOCKED);
                        }
                         commands.entity(e).insert(Sleeping::disabled());
                    }
                }

                // Gravity
                let has_gravity = if let Ok((_, _, gravity, _, _)) = physics_q.get(*first) {
                    gravity.map(|g| g.0 != 0.0).unwrap_or(true) // Default is gravity on (1.0)
                } else {
                    true
                };

                if ui.checkbox(&mut {has_gravity}, "Gravity").clicked() {
                    for &e in &selection.0 {
                        if has_gravity {
                             // Was on, turn off
                             commands.entity(e).insert(GravityScale(0.0));
                        } else {
                             // Was off, turn on
                             commands.entity(e).insert(GravityScale(1.0));
                        }
                         commands.entity(e).insert(Sleeping::disabled());
                    }
                }
            });

            ui.separator();

            // 4. Delete
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
