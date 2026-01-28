//! Context menu system.
//!
//! Provides a right-click context menu for entities, allowing actions like deletion,
//! grouping, and ungrouping.

use crate::geometry::extrusion::ExtrudableShape;
use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{
    ToolState,
    cursor::CursorWorldPos,
    editable_shape::{EditableShape, generate_shape_components},
    selection::{NextGroupID, Selection, SelectionFilter, SelectionGroup},
    tools::connector::Connector,
};
use crate::physics::floor::GroundPlane;
use crate::prelude::*;
use crate::ui::icons::GameIcons;
use bevy::ecs::system::SystemParam;
use bevy_egui::{EguiContexts, egui};
use bevy_prototype_lyon::prelude::{Fill, Stroke};
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
        } else {
            selection.clear();
        }

        let ctx = contexts.ctx_mut();
        if let Some(pointer_pos) = ctx.input(|i| i.pointer.hover_pos()) {
            state.position = Some(pointer_pos);
        }
    } else if mouse.just_pressed(MouseButton::Left) && !is_pointer_over_ui(&mut contexts) {
        state.position = None;
    }
}

#[derive(SystemParam)]
struct ContextMenuData<'w, 's> {
    collision_groups: Query<'w, 's, &'static mut CollisionGroups>,
    transform: Query<'w, 's, &'static Transform>,
    shape: Query<'w, 's, &'static EditableShape>,
    rb: Query<'w, 's, &'static RigidBody>,
    friction: Query<'w, 's, &'static Friction>,
    restitution: Query<'w, 's, &'static Restitution>,
    sensor: Query<'w, 's, &'static Sensor>,
    locked: Query<'w, 's, &'static LockedAxes>,
    gravity: Query<'w, 's, &'static GravityScale>,
    mass: Query<'w, 's, &'static ColliderMassProperties>,
    fill: Query<'w, 's, &'static Fill>,
    stroke: Query<'w, 's, &'static Stroke>,
    extrudable: Query<'w, 's, &'static ExtrudableShape>,
    materials: Query<'w, 's, &'static MeshMaterial3d<StandardMaterial>>,
    assets: ResMut<'w, Assets<StandardMaterial>>,
}

/// Renders the context menu UI if active.
fn context_menu_ui(
    mut state: ResMut<ContextMenuState>,
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut selection: ResMut<Selection>,
    mut next_group_id: ResMut<NextGroupID>,
    game_icons: Res<GameIcons>,
    mut data: ContextMenuData,
    selection_filter: Option<Res<SelectionFilter>>,
    selectable_query: Query<Entity, (With<Collider>, Without<GroundPlane>)>,
    connector_query: Query<&Connector>,
    mut next_tool_state: ResMut<NextState<ToolState>>,
) {
    let Some(pos) = state.position else {
        return;
    };

    let delete_icon = contexts.add_image(game_icons.delete.clone_weak());

    let ctx = contexts.ctx_mut();

    egui::Window::new("Context Menu")
        .fixed_pos(pos)
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .frame(egui::Frame::popup(ctx.style().as_ref()))
        .show(ctx, |ui| {
            if selection.0.is_empty() {
                ui.label("Global Actions");
                ui.separator();

                ui.menu_button("Add Shape", |ui| {
                    if ui.button("Box").clicked() {
                        next_tool_state.set(ToolState::Box);
                        state.position = None;
                        ui.close_menu();
                    }
                    if ui.button("Circle").clicked() {
                        next_tool_state.set(ToolState::Circle);
                        state.position = None;
                        ui.close_menu();
                    }
                    if ui.button("Polygon").clicked() {
                        next_tool_state.set(ToolState::Polygon);
                        state.position = None;
                        ui.close_menu();
                    }
                });

                if ui.button("Select All").clicked() {
                    if let Some(filter) = selection_filter {
                        let mut count = 0;
                        for entity in &selectable_query {
                            let is_connector = connector_query.contains(entity);
                            let pass = match *filter {
                                SelectionFilter::All => true,
                                SelectionFilter::Shapes => !is_connector,
                                SelectionFilter::Joints => is_connector,
                            };
                            if pass {
                                selection.add(entity);
                                count += 1;
                            }
                        }
                        info!("Selected {} entities", count);
                    } else {
                        for entity in &selectable_query {
                            selection.add(entity);
                        }
                    }
                    state.position = None;
                }
            } else {
                ui.label("Entity Actions");
                ui.separator();

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

                if ui.button("Duplicate").clicked() {
                    let mut new_selection = Vec::new();
                    for &entity in &selection.0 {
                        if let Ok(t) = data.transform.get(entity) {
                            let new_pos = t.translation + Vec3::new(20.0, -20.0, 0.0);
                            let mut cmd = commands.spawn(
                                Transform::from_translation(new_pos).with_rotation(t.rotation),
                            );

                            // Shape & Components
                            if let Ok(s) = data.shape.get(entity) {
                                cmd.insert(s.clone());

                                // Manually generate components to ensure Extrusion hook works
                                if let Some((path, collider)) = generate_shape_components(&s.shape) {
                                    cmd.insert(path);
                                    cmd.insert(collider);
                                }
                            }

                            if let Ok(rb) = data.rb.get(entity) {
                                cmd.insert(*rb);
                            }
                            if let Ok(f) = data.friction.get(entity) {
                                cmd.insert(*f);
                            }
                            if let Ok(r) = data.restitution.get(entity) {
                                cmd.insert(*r);
                            }
                            if let Ok(cg) = data.collision_groups.get(entity) {
                                cmd.insert(*cg);
                            }
                            if data.sensor.get(entity).is_ok() {
                                cmd.insert(Sensor);
                            }
                            if let Ok(la) = data.locked.get(entity) {
                                cmd.insert(*la);
                            }
                            if let Ok(g) = data.gravity.get(entity) {
                                cmd.insert(*g);
                            }
                            if let Ok(m) = data.mass.get(entity) {
                                cmd.insert(*m);
                            }
                            if let Ok(fill) = data.fill.get(entity) {
                                cmd.insert(*fill);
                            }
                            if let Ok(stroke) = data.stroke.get(entity) {
                                cmd.insert(*stroke);
                            }
                            // ExtrudableShape must be present for mesh generation
                            if data.extrudable.get(entity).is_ok() {
                                cmd.insert(ExtrudableShape);
                            }

                            cmd.insert(Sleeping::disabled());
                            new_selection.push(cmd.id());
                        }
                    }
                    selection.clear();
                    for e in new_selection {
                        selection.add(e);
                    }
                    state.position = None;
                }

                if ui.button("Lock/Unlock Rotation").clicked() {
                    for &entity in &selection.0 {
                        if let Ok(locked) = data.locked.get(entity) {
                            if locked.contains(LockedAxes::ROTATION_LOCKED) {
                                commands.entity(entity).insert(LockedAxes::empty());
                            } else {
                                commands.entity(entity).insert(LockedAxes::ROTATION_LOCKED);
                            }
                        } else {
                            commands.entity(entity).insert(LockedAxes::ROTATION_LOCKED);
                        }
                    }
                    state.position = None;
                }

                ui.separator();

                // Collision Depth (Layers)
                ui.collapsing("Depth (Collision Layers)", |ui| {
                    // Determine current range from selection (approximate from first entity)
                    let (current_start, current_end) =
                        if let Some(first) = selection.0.iter().next() {
                            if let Ok(groups) = data.collision_groups.get(*first) {
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
                        if ui
                            .add(egui::DragValue::new(&mut start).range(0..=31))
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("End Layer:  ");
                        if ui
                            .add(egui::DragValue::new(&mut end).range(0..=31))
                            .changed()
                        {
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
                            if let Ok(mut groups) = data.collision_groups.get_mut(entity) {
                                groups.memberships = new_groups;
                                groups.filters = new_groups;
                            } else if data.transform.contains(entity) {
                                commands
                                    .entity(entity)
                                    .insert(CollisionGroups::new(new_groups, new_groups));
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

                            if let Ok(mut groups) = data.collision_groups.get_mut(entity) {
                                groups.memberships = new_groups;
                                groups.filters = new_groups;
                            } else if data.transform.contains(entity) {
                                commands
                                    .entity(entity)
                                    .insert(CollisionGroups::new(new_groups, new_groups));
                            }
                        }
                    }

                    if ui.button("Distribute Layers").clicked() {
                        let mut entities: Vec<_> = selection.0.iter().copied().collect();
                        // Sort by Entity ID for deterministic distribution
                        entities.sort();

                        let count = entities.len();
                        if count > 0 {
                            let masks = calculate_layer_distribution(count, start, end);

                            for (entity, mask) in entities.into_iter().zip(masks.into_iter()) {
                                if mask == 0 {
                                    continue;
                                }
                                let new_groups = Group::from_bits_truncate(mask);

                                if let Ok(mut groups) = data.collision_groups.get_mut(entity) {
                                    groups.memberships = new_groups;
                                    groups.filters = new_groups;
                                } else if data.transform.contains(entity) {
                                    // Ensure entity exists before inserting
                                    commands
                                        .entity(entity)
                                        .insert(CollisionGroups::new(new_groups, new_groups));
                                }
                            }
                        }
                    }
                });

                ui.separator();

                ui.collapsing("Material Properties", |ui| {
                    let mut current_color = Color::WHITE;
                    let mut metallic = 0.0;
                    let mut roughness = 0.5;
                    let mut reflectance = 0.5;
                    let mut has_material = false;

                    // Get initial values from first selected entity
                    if let Some(first) = selection.0.iter().next() {
                        if let Ok(handle) = data.materials.get(*first) {
                            if let Some(mat) = data.assets.get(&handle.0) {
                                current_color = mat.base_color;
                                metallic = mat.metallic;
                                roughness = mat.perceptual_roughness;
                                reflectance = mat.reflectance;
                                has_material = true;
                            }
                        }
                    }

                    if has_material {
                        let mut changed = false;
                        let mut rgba = LinearRgba::from(current_color);
                        let mut color_arr = [rgba.red, rgba.green, rgba.blue];

                        ui.horizontal(|ui| {
                            ui.label("Color:");
                            if ui.color_edit_button_rgb(&mut color_arr).changed() {
                                rgba.red = color_arr[0];
                                rgba.green = color_arr[1];
                                rgba.blue = color_arr[2];
                                current_color = Color::LinearRgba(rgba);
                                changed = true;
                            }
                        });

                        if ui
                            .add(egui::Slider::new(&mut metallic, 0.0..=1.0).text("Metallic"))
                            .changed()
                        {
                            changed = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut roughness, 0.0..=1.0).text("Roughness"))
                            .changed()
                        {
                            changed = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut reflectance, 0.0..=1.0).text("Reflectance"))
                            .changed()
                        {
                            changed = true;
                        }

                        if changed {
                            for entity in &selection.0 {
                                if let Ok(handle) = data.materials.get(*entity) {
                                    if let Some(mat) = data.assets.get_mut(&handle.0) {
                                        mat.base_color = current_color;
                                        mat.metallic = metallic;
                                        mat.perceptual_roughness = roughness;
                                        mat.reflectance = reflectance;
                                    }
                                }
                            }
                        }
                    } else {
                        ui.label("No Standard Material on primary selection.");
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
            } // End else
        });
}

/// Calculates the layer masks for distributing 'count' entities across 'start' to 'end' layers.
pub fn calculate_layer_distribution(count: usize, start: u32, end: u32) -> Vec<u32> {
    if count == 0 {
        return Vec::new();
    }
    let total_layers = end.saturating_sub(start) + 1;
    if total_layers == 0 {
        return vec![0; count];
    }

    let layers_per_object = total_layers / count as u32;
    let mut remainder = total_layers % count as u32;
    let mut current_layer = start;

    let mut result = Vec::with_capacity(count);

    for _ in 0..count {
        let mut my_count = layers_per_object;
        if remainder > 0 {
            my_count += 1;
            remainder -= 1;
        }

        let mut mask = 0u32;
        for _ in 0..my_count {
            if current_layer <= 31 {
                mask |= 1 << current_layer;
                current_layer += 1;
            }
        }
        result.push(mask);
    }
    result
}
