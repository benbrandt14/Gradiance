//! Tool for creating fixed joints (welds).
//!
//! Allows fixing two bodies together or fixing a body to the background.

use crate::input::tools::utils::{is_pointer_over_ui, calculate_local_anchor};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use bevy::math::DVec2;
use bevy_egui::EguiContexts;
use bevy_prototype_lyon::prelude::*;

/// Plugin for the Weld Tool.
pub struct WeldToolPlugin;

impl Plugin for WeldToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, weld_tool_update.run_if(in_state(ToolState::Weld)));
    }
}

fn weld_tool_update(
    mut commands: Commands,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    rapier_context: Res<RapierContext>,
    mut contexts: EguiContexts,
    transforms: Query<&Transform>,
) {
    if is_pointer_over_ui(&mut contexts) {
        return;
    }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        let point = Vec2::new(current_pos.x as f32, current_pos.y as f32);
        let mut intersections = Vec::new();
        rapier_context.intersections_with_point(point, QueryFilter::default(), |e| {
            intersections.push(e);
            true
        });

        if intersections.is_empty() {
             return;
        }

        if intersections.len() == 1 {
            let entity = intersections[0];
            spawn_weld(&mut commands, &transforms, entity, None, current_pos);
        } else {
            let entity_a = intersections[0];
            let entity_b = intersections[1];
            spawn_weld(&mut commands, &transforms, entity_a, Some(entity_b), current_pos);
        }
    }
}

fn spawn_weld(
    commands: &mut Commands,
    transforms: &Query<&Transform>,
    entity_a: Entity,
    entity_b: Option<Entity>,
    anchor_world: DVec2,
) {
    let get_local = |e: Entity| -> (Vec2, f32) {
        if let Ok(t) = transforms.get(e) {
            let rotation = t.rotation.to_euler(EulerRot::XYZ).2 as f32;
            let local_d = calculate_local_anchor(t, anchor_world);
            (Vec2::new(local_d.x as f32, local_d.y as f32), rotation)
        } else {
            (Vec2::ZERO, 0.0)
        }
    };

    let (anchor_a, rot_a) = get_local(entity_a);

    // Visual X for weld
    let spawn_visuals = |commands: &mut Commands, parent: Entity| {
        let v1 = commands.spawn((
            ShapeBundle {
                path: GeometryBuilder::build_as(&shapes::Line(Vec2::new(-3.0, -3.0), Vec2::new(3.0, 3.0))),
                ..default()
            },
             Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
             Transform::from_translation(Vec3::Z * 0.1),
             GlobalTransform::default(),
             VisibilityBundle::default(),
        )).id();
        let v2 = commands.spawn((
            ShapeBundle {
                path: GeometryBuilder::build_as(&shapes::Line(Vec2::new(-3.0, 3.0), Vec2::new(3.0, -3.0))),
                ..default()
            },
             Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
             Transform::from_translation(Vec3::Z * 0.1),
             GlobalTransform::default(),
             VisibilityBundle::default(),
        )).id();
        commands.entity(parent).add_child(v1).add_child(v2);
    };

    if let Some(entity_b) = entity_b {
        let (anchor_b, rot_b) = get_local(entity_b);

        // Spawn "Axle" (Weld Node)
        let axle = commands.spawn((
            RigidBody::Dynamic,
            AdditionalMassProperties::Mass(0.01),
            Transform::from_xyz(anchor_world.x as f32, anchor_world.y as f32, 11.0),
            GlobalTransform::default(),
            VisibilityBundle::default(),
            Collider::ball(0.5),
            Sensor,
        )).id();

        spawn_visuals(commands, axle);

        // A -> Axle (Fixed)
        commands.entity(entity_a).insert(
            ImpulseJoint::new(axle, FixedJointBuilder::new().local_anchor1(anchor_a).local_anchor2(Vec2::ZERO).local_basis2(rot_a))
        );

        // B -> Axle (Fixed)
        commands.entity(entity_b).insert(
             ImpulseJoint::new(axle, FixedJointBuilder::new().local_anchor1(anchor_b).local_anchor2(Vec2::ZERO).local_basis2(rot_b))
        );

    } else {
        // Pin to world (Weld)
        let pin = commands.spawn((
            RigidBody::Fixed,
            Transform::from_xyz(anchor_world.x as f32, anchor_world.y as f32, 0.0),
            Collider::ball(0.5),
            Sensor,
            GlobalTransform::default(),
            VisibilityBundle::default(),
        )).id();
        spawn_visuals(commands, pin);

        // A -> Pin (Fixed)
        // rot_a is angle of A. Pin angle is 0.
        // Basis2 (Pin) = RotA - RotPin = RotA.
        commands.entity(entity_a).insert(
            ImpulseJoint::new(pin, FixedJointBuilder::new().local_anchor1(anchor_a).local_anchor2(Vec2::ZERO).local_basis2(rot_a))
        );
    }
}
