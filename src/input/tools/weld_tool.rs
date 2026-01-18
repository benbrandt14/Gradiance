//! Tool for creating fixed joints (welds).
//!
//! Allows fixing two bodies together or fixing a body to the background.

use crate::input::tools::utils::{is_pointer_over_ui, calculate_local_anchor};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use avian2d::prelude::*;
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
    spatial_query: SpatialQuery,
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
        let filter = SpatialQueryFilter::default();
        let intersections = spatial_query.point_intersections(current_pos, &filter);

        if intersections.is_empty() {
             return;
        }

        let point = current_pos;

        if intersections.len() == 1 {
            let entity = intersections[0];
            spawn_weld(&mut commands, &transforms, entity, None, point);
        } else {
            let entity_a = intersections[0];
            let entity_b = intersections[1];
            spawn_weld(&mut commands, &transforms, entity_a, Some(entity_b), point);
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
    let get_local = |e: Entity| -> (DVec2, f64) {
        if let Ok(t) = transforms.get(e) {
            let rotation = t.rotation.to_euler(EulerRot::XYZ).2 as f64;
            let local_anchor = calculate_local_anchor(t, anchor_world);
            (local_anchor, rotation)
        } else {
            (DVec2::ZERO, 0.0)
        }
    };

    let (anchor_a, rot_a) = get_local(entity_a);

    // Visual X for weld
    let spawn_visuals = |commands: &mut Commands, parent: Entity| {
        let v1 = commands.spawn((
            ShapeBuilder::with(&shapes::Line(Vec2::new(-3.0, -3.0), Vec2::new(3.0, 3.0)))
                .stroke(Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0))
                .build(),
             Transform::from_translation(Vec3::Z * 0.1),
        )).id();
        let v2 = commands.spawn((
            ShapeBuilder::with(&shapes::Line(Vec2::new(-3.0, 3.0), Vec2::new(3.0, -3.0)))
                .stroke(Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0))
                .build(),
             Transform::from_translation(Vec3::Z * 0.1),
        )).id();
        commands.entity(parent).add_children(&[v1, v2]);
    };

    if let Some(entity_b) = entity_b {
        let (anchor_b, rot_b) = get_local(entity_b);
        // Angle adjustment? FixedJoint tries to hold relative angle.
        // If we want to lock them IN PLACE, we need to set the rest angle to current relative angle?
        // FixedJoint constructor sets it to hold current relative transform if initialized correctly?
        // Avian's FixedJoint defaults to aligned axes. We want to lock CURRENT alignment.
        // `with_local_anchor` sets positions.
        // We also need `with_local_rotation`? Or `with_rest_angle`?
        // Avian 0.5 `FixedJoint` has `with_local_anchor1`, `with_local_anchor2`.
        // Also `with_local_rotation1`, `with_local_rotation2`.
        // To lock in place:
        // Local Rot 1 = 0?
        // We want T_a * L_1 = T_b * L_2
        // If we set local anchors, we handle position.
        // For rotation, we want R_a * R_local_1 = R_b * R_local_2
        // So R_local_1 = 0, R_local_2 = R_b^-1 * R_a ?
        // Effectively, we want to freeze the angle difference `rot_a - rot_b`.

        // NOTE: FixedJoint in Avian 0.5 uses `with_local_basis2` for rotation, but it likely expects a Rotation type (Rot2/f64).
        // The error log suggested `with_local_basis2`.
        // However, `Rot2` construction from radians is standard.
        // Let's assume `with_local_rotation` or equivalent is unavailable or named differently.
        // If `with_local_basis2` takes `impl Into<Rot2>`, we can pass radians or Rot2::radians(angle).
        // Wait, Avian f64 2D uses f64 for angle in some places, but Rot2 in others.
        // Let's try `with_local_basis2` with `Rot2::radians(...)`.

        // We need Rot2.
        // Imports: `avian2d::math::Rot2` or just `Rot2` via prelude (it is in bevy::math::Rot2? No, avian uses its own or bevy's?)
        // Avian re-exports math types. `use crate::prelude::*` should have it.

        let joint_entity = commands.spawn((
            FixedJoint::new(entity_a, entity_b)
                .with_local_anchor1(anchor_a)
                .with_local_anchor2(anchor_b)
                .with_local_basis2(Rot2::radians((rot_a - rot_b) as f32)), // Lock relative rotation
            Transform::from_xyz(anchor_world.x as f32, anchor_world.y as f32, 11.0),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            // Add sensor collider for selection
            Collider::circle(0.5),
            Sensor,
        )).id();
        spawn_visuals(commands, joint_entity);

    } else {
        // Pin to world (Weld)
        let pin = commands.spawn((
            RigidBody::Static,
            Transform::from_xyz(anchor_world.x as f32, anchor_world.y as f32, 0.0),
            // Add sensor collider for selection
            Collider::circle(0.5),
            Sensor,
        )).id();
        spawn_visuals(commands, pin);

        let anchor_b = DVec2::ZERO;
        // World rotation is 0.
        // We want to lock entity_a's rotation `rot_a` relative to world `0`.
        // So relative angle = rot_a.

        commands.spawn((
            FixedJoint::new(entity_a, pin)
                .with_local_anchor1(anchor_a)
                .with_local_anchor2(anchor_b)
                .with_local_basis2(Rot2::radians(rot_a as f32)),
        ));
    }
}
