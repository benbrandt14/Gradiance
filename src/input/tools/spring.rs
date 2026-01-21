//! Spring tool implementation.
//!
//! Handles the creation and simulation of springs using manual force application.

use crate::input::commands::{CommandStack, SpawnSpringCommand, SpringProperties, SpringVisual};
use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use bevy::prelude::*;
use bevy_egui::EguiContexts;
use bevy_prototype_lyon::prelude::*;

/// Plugin for the Spring Tool.
pub struct SpringToolPlugin;

impl Plugin for SpringToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_spring_tool.run_if(in_state(ToolState::Spring)),
                update_spring_visuals,
                apply_spring_forces,
            ),
        );
    }
}

/// State for the spring tool (dragging).
#[derive(Default)]
struct DragState {
    active: bool,
    start_entity: Option<Entity>,
    start_pos: Vec2, // World position at start
    temp_visual: Option<Entity>,
}

fn update_spring_tool(
    mut commands: Commands,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut drag_state: Local<DragState>,
    mut contexts: EguiContexts,
    rapier_context_query: Query<&RapierContext>,
    transforms: Query<&GlobalTransform>,
    bodies: Query<Entity, With<RigidBody>>,
    parents: Query<&Parent>,
) {
    if is_pointer_over_ui(&mut contexts) {
        return;
    }

    let Some(world_pos) = cursor_pos.0 else {
        return;
    };

    // Helper to find body under cursor
    let find_body = |pos: Vec2| -> Option<Entity> {
        let rapier_context = rapier_context_query.iter().next()?;
        let mut hit_entity = None;
        rapier_context.intersections_with_point(
            pos,
            QueryFilter::default().exclude_sensors(),
            |e| {
                hit_entity = Some(e);
                false // Stop on first hit
            },
        );

        if let Some(mut e) = hit_entity {
            // Traverse up to find RigidBody
            loop {
                if bodies.get(e).is_ok() {
                    return Some(e);
                }
                if let Ok(parent) = parents.get(e) {
                    e = parent.get();
                } else {
                    break;
                }
            }
        }
        None
    };

    // Start Drag
    if mouse.just_pressed(MouseButton::Left) {
        drag_state.active = true;
        drag_state.start_pos = world_pos;
        drag_state.start_entity = find_body(world_pos);

        // Spawn temporary visual line
        let path = GeometryBuilder::build_as(&shapes::Line(Vec2::ZERO, Vec2::ZERO));
        drag_state.temp_visual = Some(
            commands
                .spawn((
                    ShapeBundle {
                        path,
                        ..default()
                    },
                    Stroke::new(Color::BLACK, 2.0),
                ))
                .id(),
        );
    }

    // Update Drag
    if drag_state.active {
        // Update temporary visual
        if let Some(visual_id) = drag_state.temp_visual {
            let start = drag_state.start_pos;
            let end = world_pos;
            let path = GeometryBuilder::build_as(&shapes::Line(start, end));
            if let Some(mut entity_cmd) = commands.get_entity(visual_id) {
                entity_cmd.insert(path);
            }
        }

        // Finish Drag
        if mouse.just_released(MouseButton::Left) {
            drag_state.active = false;

            // Cleanup temp visual
            if let Some(visual_id) = drag_state.temp_visual {
                commands.entity(visual_id).despawn();
                drag_state.temp_visual = None;
            }

            let end_entity = find_body(world_pos);
            let start_entity = drag_state.start_entity;

            // If both are None (background to background), maybe allow creating a static rod?
            // For now, allow it (will pin both ends).

            // Resolve Anchors
            // We need LOCAL anchors for the bodies.
            // If body exists, transform world_pos to local.
            // If body is None, local anchor is just world_pos (since we pin to world,
            // but effectively the pin is created at that pos, so local anchor is 0).

            // Wait, logic in ResolveJointTargets expects anchor relative to the body if body exists.

            let get_anchor = |entity: Option<Entity>, pos: Vec2| -> Vec2 {
                if let Some(e) = entity {
                    if let Ok(t) = transforms.get(e) {
                         t.affine().inverse().transform_point3(Vec3::new(pos.x, pos.y, 0.0)).truncate()
                    } else {
                        pos
                    }
                } else {
                    pos // For Pin, we use world pos as "anchor" initially, creating pin at that pos.
                }
            };

            let anchor_a = get_anchor(start_entity, drag_state.start_pos);
            let anchor_b = get_anchor(end_entity, world_pos);

            // If we have no start entity, we must create a pin.
            // But SpawnSpringCommand expects `entity_a` to be the MAIN entity that holds the component.
            // If `start_entity` is None, we need to create a pin FIRST, or let the command handle it.
            // SpawnSpringCommand only handles `entity_b` as optional (Pin).
            // `entity_a` MUST exist.

            // So if start_entity is None, we need to spawn a Pin for A manually here?
            // Or we swap A and B if B exists and A doesn't?

            let (final_entity_a, final_entity_b, final_anchor_a, final_anchor_b) =
                if start_entity.is_none() && end_entity.is_some() {
                    // Swap
                    (end_entity.unwrap(), start_entity, anchor_b, anchor_a)
                } else if start_entity.is_none() && end_entity.is_none() {
                    // Both None. We need to create a pin for A.
                    // We can spawn a static body here.
                    let pin_id = commands.spawn((
                        RigidBody::Fixed,
                        Transform::from_translation(Vec3::new(drag_state.start_pos.x, drag_state.start_pos.y, 0.0))
                    )).id();
                    (pin_id, None, Vec2::ZERO, anchor_b)
                } else {
                    // A exists (or A exists and B exists/None)
                    (start_entity.unwrap(), end_entity, anchor_a, anchor_b)
                };

            let cmd = SpawnSpringCommand {
                entity_a: final_entity_a,
                entity_b: final_entity_b,
                anchor_a: final_anchor_a,
                anchor_b: final_anchor_b,
                stiffness: 10.0, // Default
                damping: 0.5,    // Default
                visual_entity: None,
                pin_entity: None,
            };

            commands.queue(move |world: &mut World| {
                world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                    stack.push(Box::new(cmd), world);
                });
            });
        }
    }
}

/// System to apply spring forces based on Hooke's Law.
fn apply_spring_forces(
    mut spring_query: Query<(Entity, &SpringProperties, &GlobalTransform)>,
    mut bodies_query: Query<(&GlobalTransform, &mut ExternalForce)>,
) {
    for (entity_a, props, transform_a) in &mut spring_query {
        // Resolve World Positions of anchors
        let world_a = transform_a.compute_transform().transform_point(Vec3::new(props.local_anchor_a.x, props.local_anchor_a.y, 0.0)).truncate();

        // Resolve Target
        // We need GlobalTransform of entity B.
        // And we need to apply force to A and B.
        // We already have entity_a reference from the query, but we need mutable access to its ExternalForce.
        // We can't query `spring_query` and `bodies_query` safely if they overlap and we need mutability.
        // But `spring_query` only needs read access to Entity, Props, Transform.
        // `bodies_query` needs write access to ExternalForce.
        // This is safe as long as we use `get_mut` carefully.

        // Use get_many_mut if entities differ.
        if entity_a != props.connection_b {
            if let Ok([
                (t_a, mut f_a),
                (t_b, mut f_b)
            ]) = bodies_query.get_many_mut([entity_a, props.connection_b]) {
                 // Re-calculate positions using the borrowed transforms
                 let pos_a = t_a.compute_transform().transform_point(Vec3::new(props.local_anchor_a.x, props.local_anchor_a.y, 0.0)).truncate();
                 let pos_b = t_b.compute_transform().transform_point(Vec3::new(props.local_anchor_b.x, props.local_anchor_b.y, 0.0)).truncate();

                 let diff = pos_b - pos_a;
                 let dist = diff.length();
                 let dir = if dist > 0.0001 { diff / dist } else { Vec2::ZERO };

                 let force_mag = props.stiffness * (dist - props.rest_length);
                 let force = dir * force_mag;

                 // Apply to A
                 f_a.force += force;
                 // Apply to B (opposite)
                 f_b.force -= force;
            } else {
                 // Only A is valid (B might be static/pin without ExternalForce?)
                 // Or get_many_mut failed for some reason.
                 // Fallback to single body calc if needed, or just ignore (spring broken).
            }
        }
    }
}

/// System to update spring visuals.
fn update_spring_visuals(
    mut visuals: Query<(&mut Path, &mut Stroke, &SpringVisual)>,
    transforms: Query<&GlobalTransform>,
    spring_props: Query<&SpringProperties>,
) {
    for (mut path, mut stroke, visual) in &mut visuals {
        let Ok(t_a) = transforms.get(visual.entity_a) else { continue };
        let world_a = t_a.compute_transform().transform_point(Vec3::new(visual.local_anchor_a.x, visual.local_anchor_a.y, 0.0)).truncate();

        let world_b = if let Some(e_b) = visual.entity_b {
             if let Ok(t_b) = transforms.get(e_b) {
                t_b.compute_transform().transform_point(Vec3::new(visual.local_anchor_b.x, visual.local_anchor_b.y, 0.0)).truncate()
            } else {
                continue;
            }
        } else {
             // Should not happen if we use Pin, but strictly speaking visual.entity_b is Option.
             // If None, maybe use local anchor as world pos?
             Vec2::new(visual.local_anchor_b.x, visual.local_anchor_b.y)
        };

        // Draw Zig Zag
        let diff = world_b - world_a;
        let dist = diff.length();
        let dir = if dist > 0.0001 { diff / dist } else { Vec2::X };
        let normal = Vec2::new(-dir.y, dir.x);

        let segments = 10;
        let step = dist / segments as f32;
        let amp = 0.5; // Width of zig zag

        let mut points = Vec::new();
        points.push(world_a);

        for i in 1..segments {
            let t = i as f32 * step;
            let offset = if i % 2 == 0 { normal * amp } else { normal * -amp };
            points.push(world_a + dir * t + offset);
        }

        points.push(world_b);

        *path = GeometryBuilder::build_as(&shapes::Polygon {
            points,
            closed: false,
        });

        // Color based on extension
        if let Ok(props) = spring_props.get(visual.entity_a) {
            let extension = dist - props.rest_length;
            if extension.abs() > 0.1 {
                // Visualize strain
                let ratio = (extension.abs() / props.rest_length).clamp(0.0, 1.0);
                if extension > 0.0 {
                    // Stretch: Black -> Red
                    stroke.color = Color::mix(&Color::BLACK, &Color::srgb(1.0, 0.0, 0.0), ratio);
                } else {
                    // Compress: Black -> Blue
                    stroke.color = Color::mix(&Color::BLACK, &Color::srgb(0.0, 0.0, 1.0), ratio);
                }
            } else {
                stroke.color = Color::BLACK;
            }
        }
    }
}
