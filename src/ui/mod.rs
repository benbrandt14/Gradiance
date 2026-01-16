use crate::commands::property::{
    SetColorCommand, SetFrictionCommand, SetRestitutionCommand, SetRigidBodyCommand,
};
use crate::commands::{BatchCommand, CommandStack, GameCommand, SubmitGameCommand};
use crate::tools::move_tool::MoveToolState;
use crate::tools::ToolState;
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use std::collections::HashMap;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin {
            enable_multipass_for_primary_context: true,
        });
        app.init_resource::<ContextMenuState>();
        app.init_resource::<PropertyEditState>();
        app.add_systems(Update, (ui_system, context_menu_system));
    }
}

#[derive(Resource, Default)]
struct ContextMenuState {
    open: bool,
    position: Vec2,
}

#[derive(Resource, Default)]
struct PropertyEditState {
    friction_starts: HashMap<Entity, Friction>,
    restitution_starts: HashMap<Entity, Restitution>,
}

#[allow(clippy::too_many_arguments)]
fn context_menu_system(
    mut contexts: EguiContexts,
    mut menu_state: ResMut<ContextMenuState>,
    mut edit_state: ResMut<PropertyEditState>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    move_tool_state: Res<MoveToolState>,
    mut commands: Commands,
    mut query: Query<(
        Entity,
        Option<&mut Friction>,
        Option<&mut Restitution>,
        Option<&mut RigidBody>,
        Option<&mut Sprite>,
        Option<&MeshMaterial2d<ColorMaterial>>,
    )>,
    materials: Res<Assets<ColorMaterial>>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };

    // Open Logic
    if mouse.just_pressed(MouseButton::Right) {
        // If we have a selection, open menu
        if !move_tool_state.selected_entities.is_empty() {
            if let Some(pos) = window.cursor_position() {
                menu_state.open = true;
                menu_state.position = pos;
            }
        }
    }

    // UI Logic
    if menu_state.open {
        let mut open = true;

        egui::Window::new("Properties")
            .fixed_pos([menu_state.position.x, menu_state.position.y])
            .open(&mut open)
            .collapsible(false)
            .show(contexts.ctx_mut(), |ui| {
                ui.label("Selected Properties");
                ui.separator();

                let entities: Vec<Entity> =
                    move_tool_state.selected_entities.iter().cloned().collect();
                if entities.is_empty() {
                    return;
                }

                let first_entity = entities[0];

                // --- FRICTION ---
                let mut current_friction = 0.5;
                if let Ok((_, f, _, _, _, _)) = query.get(first_entity) {
                    if let Some(f) = f {
                        current_friction = f.dynamic_coefficient;
                    }
                }

                let mut friction_val = current_friction;
                let response =
                    ui.add(egui::Slider::new(&mut friction_val, 0.0..=1.0).text("Friction"));

                if response.drag_started() {
                    for &e in &entities {
                        if let Ok((_, Some(f), _, _, _, _)) = query.get(e) {
                            edit_state.friction_starts.insert(e, *f);
                        }
                    }
                }

                if response.changed() {
                    for &e in &entities {
                        if !edit_state.friction_starts.contains_key(&e) {
                            if let Ok((_, Some(f), _, _, _, _)) = query.get(e) {
                                edit_state.friction_starts.insert(e, *f);
                            }
                        }
                        if let Ok((_, Some(mut f), _, _, _, _)) = query.get_mut(e) {
                            f.dynamic_coefficient = friction_val;
                            f.static_coefficient = friction_val;
                        }
                    }
                }

                if response.drag_stopped() {
                    let mut cmds: Vec<Box<dyn GameCommand>> = Vec::new();
                    for &e in &entities {
                        let mut new_f = Friction::default();
                        if let Ok((_, Some(f), _, _, _, _)) = query.get(e) {
                            new_f = *f;
                        }
                        let old_f = edit_state.friction_starts.remove(&e);

                        cmds.push(Box::new(SetFrictionCommand {
                            entity: e,
                            new_friction: new_f,
                            old_friction: old_f,
                        }));
                    }
                    if !cmds.is_empty() {
                        commands.queue(SubmitGameCommand(Box::new(BatchCommand(cmds))));
                    }
                }

                // --- RESTITUTION ---
                let mut current_restitution = 0.5;
                if let Ok((_, _, r, _, _, _)) = query.get(first_entity) {
                    if let Some(r) = r {
                        current_restitution = r.coefficient;
                    }
                }

                let mut restitution_val = current_restitution;
                let response =
                    ui.add(egui::Slider::new(&mut restitution_val, 0.0..=1.0).text("Restitution"));

                if response.drag_started() {
                    for &e in &entities {
                        if let Ok((_, _, Some(r), _, _, _)) = query.get(e) {
                            edit_state.restitution_starts.insert(e, *r);
                        }
                    }
                }

                if response.changed() {
                    for &e in &entities {
                        if !edit_state.restitution_starts.contains_key(&e) {
                            if let Ok((_, _, Some(r), _, _, _)) = query.get(e) {
                                edit_state.restitution_starts.insert(e, *r);
                            }
                        }
                        if let Ok((_, _, Some(mut r), _, _, _)) = query.get_mut(e) {
                            r.coefficient = restitution_val;
                        }
                    }
                }

                if response.drag_stopped() {
                    let mut cmds: Vec<Box<dyn GameCommand>> = Vec::new();
                    for &e in &entities {
                        let mut new_r = Restitution::default();
                        if let Ok((_, _, Some(r), _, _, _)) = query.get(e) {
                            new_r = *r;
                        }
                        let old_r = edit_state.restitution_starts.remove(&e);

                        cmds.push(Box::new(SetRestitutionCommand {
                            entity: e,
                            new_restitution: new_r,
                            old_restitution: old_r,
                        }));
                    }
                    if !cmds.is_empty() {
                        commands.queue(SubmitGameCommand(Box::new(BatchCommand(cmds))));
                    }
                }

                // --- RIGID BODY ---
                let mut current_rb = RigidBody::Dynamic;
                if let Ok((_, _, _, rb, _, _)) = query.get(first_entity) {
                    if let Some(rb) = rb {
                        current_rb = *rb;
                    }
                }

                let selected_text = match current_rb {
                    RigidBody::Dynamic => "Dynamic",
                    RigidBody::Static => "Static",
                    RigidBody::Kinematic => "Kinematic",
                };

                ui.horizontal(|ui| {
                    ui.label("Body Type");
                    egui::ComboBox::from_id_salt("rb_combo")
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            let mut changed = false;
                            let mut new_val = current_rb;

                            if ui
                                .selectable_label(current_rb == RigidBody::Dynamic, "Dynamic")
                                .clicked()
                            {
                                new_val = RigidBody::Dynamic;
                                changed = true;
                            }
                            if ui
                                .selectable_label(current_rb == RigidBody::Static, "Static")
                                .clicked()
                            {
                                new_val = RigidBody::Static;
                                changed = true;
                            }
                            if ui
                                .selectable_label(current_rb == RigidBody::Kinematic, "Kinematic")
                                .clicked()
                            {
                                new_val = RigidBody::Kinematic;
                                changed = true;
                            }

                            if changed {
                                let mut cmds: Vec<Box<dyn GameCommand>> = Vec::new();
                                for &e in &entities {
                                    // Get old value for Undo
                                    let mut old_rb = None;
                                    if let Ok((_, _, _, Some(rb), _, _)) = query.get(e) {
                                        old_rb = Some(*rb);
                                    }

                                    cmds.push(Box::new(SetRigidBodyCommand {
                                        entity: e,
                                        new_body_type: new_val,
                                        old_body_type: old_rb,
                                    }));
                                }
                                if !cmds.is_empty() {
                                    commands.queue(SubmitGameCommand(Box::new(BatchCommand(cmds))));
                                }
                            }
                        });
                });

                // --- COLOR ---
                // Try to get color from first entity
                let mut current_color = Color::WHITE;
                if let Ok((_, _, _, _, sprite, mat_handle)) = query.get(first_entity) {
                    if let Some(s) = sprite {
                        current_color = s.color;
                    } else if let Some(h) = mat_handle {
                        if let Some(mat) = materials.get(h) {
                            current_color = mat.color;
                        }
                    }
                }

                let mut rgba = current_color.to_linear().to_f32_array();
                ui.horizontal(|ui| {
                    ui.label("Color");
                    let response = ui.color_edit_button_rgba_unmultiplied(&mut rgba);

                    if response.changed() {
                        let new_color = Color::linear_rgba(rgba[0], rgba[1], rgba[2], rgba[3]);

                        let mut cmds: Vec<Box<dyn GameCommand>> = Vec::new();
                        for &e in &entities {
                            // Get old color
                            let mut old_color = None;
                            if let Ok((_, _, _, _, sprite, mat_handle)) = query.get(e) {
                                if let Some(s) = sprite {
                                    old_color = Some(s.color);
                                } else if let Some(h) = mat_handle {
                                    if let Some(mat) = materials.get(h) {
                                        old_color = Some(mat.color);
                                    }
                                }
                            }

                            cmds.push(Box::new(SetColorCommand {
                                entity: e,
                                new_color,
                                old_color,
                            }));
                        }
                        if !cmds.is_empty() {
                            commands.queue(SubmitGameCommand(Box::new(BatchCommand(cmds))));
                        }
                    }
                });
            });

        menu_state.open = open;
    }
}

fn ui_system(
    mut contexts: EguiContexts,
    mut tool_state: ResMut<NextState<ToolState>>,
    current_tool: Res<State<ToolState>>,
    mut commands: Commands,
    mut time: ResMut<Time<Virtual>>,
) {
    egui::Window::new("Tools")
        .anchor(egui::Align2::LEFT_TOP, [10.0, 10.0])
        .collapsible(false)
        .title_bar(false)
        .show(contexts.ctx_mut(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Gradiance");
                ui.separator();

                if ui.button("Move").clicked() {
                    tool_state.set(ToolState::Move);
                }
                if ui.button("Box").clicked() {
                    tool_state.set(ToolState::Box);
                }
                if ui.button("Circle").clicked() {
                    tool_state.set(ToolState::Circle);
                }
                if ui.button("Plane").clicked() {
                    tool_state.set(ToolState::Plane);
                }
                if ui.button("Hinge").clicked() {
                    tool_state.set(ToolState::Hinge);
                }
                if ui.button("Spring").clicked() {
                    tool_state.set(ToolState::Spring);
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                let label = if time.is_paused() {
                    "▶ Play"
                } else {
                    "⏸ Pause"
                };
                if ui.button(label).clicked() {
                    if time.is_paused() {
                        time.unpause();
                    } else {
                        time.pause();
                    }
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Undo").clicked() {
                    commands.queue(|world: &mut World| {
                        if let Some(mut stack) = world.remove_resource::<CommandStack>() {
                            stack.undo(world);
                            world.insert_resource(stack);
                        }
                    });
                }
                if ui.button("Redo").clicked() {
                    commands.queue(|world: &mut World| {
                        if let Some(mut stack) = world.remove_resource::<CommandStack>() {
                            stack.redo(world);
                            world.insert_resource(stack);
                        }
                    });
                }

                if ui.button("Clear").clicked() {
                    commands.queue(|world: &mut World| {
                        let mut to_despawn = Vec::new();
                        let mut query =
                            world.query_filtered::<Entity, With<avian2d::prelude::RigidBody>>();
                        for entity in query.iter(world) {
                            to_despawn.push(entity);
                        }
                        for entity in to_despawn {
                            world.despawn(entity);
                        }
                    });
                }
            });

            ui.label(format!("Tool: {:?}", current_tool.get()));
        });
}
