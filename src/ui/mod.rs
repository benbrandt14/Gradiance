use crate::commands::CommandStack;
use crate::tools::ToolState;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin);
        app.add_systems(Update, ui_system);
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
                if ui.button("Hinge").clicked() {
                    tool_state.set(ToolState::Hinge);
                }
                if ui.button("Spring").clicked() {
                    tool_state.set(ToolState::Spring);
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                let label = if time.is_paused() { "▶ Play" } else { "⏸ Pause" };
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
                    // Clear all Dynamic bodies?
                    commands.queue(|world: &mut World| {
                        // Query all RigidBody entities
                        let mut to_despawn = Vec::new();
                        // We need to query world.
                        // world.query::<&RigidBody>() ...
                        // This is tricky inside a closure with &mut World if we don't handle lifetimes carefully,
                        // but queries can be created.
                        let mut query =
                            world.query_filtered::<Entity, With<avian2d::prelude::RigidBody>>();
                        for entity in query.iter(world) {
                            to_despawn.push(entity);
                        }
                        for entity in to_despawn {
                            world.despawn(entity);
                        }
                        // Also clear undo stack?
                        if let Some(mut _stack) = world.get_resource_mut::<CommandStack>() {
                            // stack.clear(); // If implemented
                        }
                    });
                }
            });

            ui.label(format!("Tool: {:?}", current_tool.get()));
        });
}
