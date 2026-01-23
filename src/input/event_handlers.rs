//! Event handlers for modifying the world.
//!
//! Handles spawning shapes, joints, and modifying entity properties.

use crate::geometry::DebugColor;
use crate::input::events::*;
use crate::input::editable::{EditableBox, EditableCircle};
use crate::input::tools::connector::Connector;
use crate::input::ZIndex;
use crate::physics::floor::GroundPlane;
use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::dynamics::TypedJoint;
use bevy_rapier2d::rapier::dynamics::{
    FixedJointBuilder as RapierFixedJointBuilder, GenericJoint,
    PrismaticJointBuilder as RapierPrismaticJointBuilder,
    RevoluteJointBuilder as RapierRevoluteJointBuilder,
    RopeJointBuilder as RapierRopeJointBuilder, SpringJointBuilder as RapierSpringJointBuilder,
};
use bevy_rapier2d::rapier::geometry::SharedShape;
use nalgebra::{Isometry2, Point2, Vector2};

/// Helper to spawn a physics entity with common components.
fn spawn_physics_entity(
    commands: &mut Commands,
    position: Vec2,
    collider: Collider,
    extra_bundle: impl Bundle,
    z_index: f32,
) -> Entity {
    commands
        .spawn((
            Transform::from_xyz(position.x, position.y, z_index),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RigidBody::Dynamic,
            collider,
            extra_bundle,
        ))
        .id()
}

/// System to handle `SpawnBoxEvent`.
pub fn handle_spawn_box(
    mut events: EventReader<SpawnBoxEvent>,
    mut commands: Commands,
    mut z_index: ResMut<ZIndex>,
) {
    for event in events.read() {
        spawn_physics_entity(
            &mut commands,
            event.position,
            Collider::cuboid(event.width / 2.0, event.height / 2.0),
            (
                EditableBox {
                    width: event.width as f64,
                    height: event.height as f64,
                },
                DebugColor(Color::srgb(0.5, 0.5, 1.0)),
            ),
            z_index.next(),
        );
    }
}

/// System to handle `SpawnCircleEvent`.
pub fn handle_spawn_circle(
    mut events: EventReader<SpawnCircleEvent>,
    mut commands: Commands,
    mut z_index: ResMut<ZIndex>,
) {
    for event in events.read() {
        spawn_physics_entity(
            &mut commands,
            event.position,
            Collider::ball(event.radius),
            (
                EditableCircle {
                    radius: event.radius as f64,
                },
                DebugColor(Color::srgb(1.0, 0.5, 0.5)),
            ),
            z_index.next(),
        );
    }
}

/// System to handle `SpawnPolygonEvent`.
pub fn handle_spawn_polygon(
    mut events: EventReader<SpawnPolygonEvent>,
    mut commands: Commands,
    mut z_index: ResMut<ZIndex>,
) {
    for event in events.read() {
        if event.vertices.len() < 3 {
            warn!("Polygon must have at least 3 vertices");
            continue;
        }

        let vertices: Vec<Point2<f32>> = event
            .vertices
            .iter()
            .map(|v| Point2::new(v.x, v.y))
            .collect();
        let indices: Vec<[u32; 2]> = (0..vertices.len())
            .map(|i| [i as u32, ((i + 1) % vertices.len()) as u32])
            .collect();

        let rapier_shape = SharedShape::convex_decomposition(&vertices, &indices);
        let collider = Collider::from(rapier_shape);

        spawn_physics_entity(
            &mut commands,
            event.position,
            collider,
            DebugColor(Color::srgb(0.5, 1.0, 0.5)),
            z_index.next(),
        );
    }
}

/// System to handle `SpawnGroundEvent`.
pub fn handle_spawn_ground(
    mut events: EventReader<SpawnGroundEvent>,
    mut commands: Commands,
) {
    for event in events.read() {
        let width = 100_000.0;
        let depth = 1000.0;

        let rot = Quat::from_rotation_z(event.rotation);
        let offset = rot * Vec3::new(0.0, -depth / 2.0, 0.0);
        let center = Vec3::new(event.position.x, event.position.y, 0.0) + offset;

        let z = -1.0;

        commands.spawn((
            Transform::from_translation(center + Vec3::new(0.0, 0.0, z)).with_rotation(rot),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RigidBody::Fixed,
            Collider::cuboid(width / 2.0, depth / 2.0),
            Friction::coefficient(0.5),
            Restitution::coefficient(0.0),
            GroundPlane,
            Name::new("Ground"),
            DebugColor(Color::srgb(0.2, 0.2, 0.2)),
        ));
    }
}

/// Helper to resolve joint targets and handle pinning.
fn resolve_joint_targets(
    commands: &mut Commands,
    _entity_a: Entity,
    entity_b: Option<Entity>,
    anchor_a: Vec2,
    anchor_b: Vec2,
    visual_entity: Option<Entity>,
) -> (Entity, Option<Entity>, Vec2, Vec2) {
    let mut pin_entity = None;
    let target_entity;
    let local_anchor_1 = anchor_a;
    let local_anchor_2;

    if let Some(e_b) = entity_b {
        target_entity = e_b;
        local_anchor_2 = anchor_b;
    } else {
        let world_pos = Vec3::new(anchor_b.x, anchor_b.y, 0.0);
        let pin_id = commands
            .spawn((
                RigidBody::Fixed,
                Transform::from_translation(world_pos),
                Collider::ball(0.1),
            ))
            .id();

        pin_entity = Some(pin_id);
        target_entity = pin_id;
        local_anchor_2 = Vec2::ZERO;

        if let Some(v_id) = visual_entity {
            commands.queue(move |world: &mut World| {
                if let Some(mut connector) = world.get_mut::<Connector>(v_id) {
                    connector.entity_b = Some(pin_id);
                }
            });
        }
    }

    (target_entity, pin_entity, local_anchor_1, local_anchor_2)
}

/// Helper to spawn connector visual.
fn spawn_connector_visual(
    commands: &mut Commands,
    entity_a: Entity,
    entity_b: Option<Entity>,
    anchor_a: Vec2,
    anchor_b: Vec2,
    children_builder: impl FnOnce(&mut ChildBuilder),
) -> Entity {
    let visual_id = commands
        .spawn((
            Transform::from_xyz(anchor_a.x, anchor_a.y, 0.1),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Collider::ball(0.5),
            Sensor,
            Connector {
                entity_a,
                entity_b,
                local_anchor_a: anchor_a,
                local_anchor_b: anchor_b,
            },
        ))
        .set_parent(entity_a)
        .with_children(children_builder)
        .id();

    visual_id
}

/// System to handle `SpawnJointEvent`.
pub fn handle_spawn_joint(
    mut events: EventReader<SpawnJointEvent>,
    mut commands: Commands,
) {
    for event in events.read() {
        let visual_id_opt = match &event.joint_type {
            JointType::Revolute => {
                 let vid = spawn_connector_visual(
                    &mut commands,
                    event.entity_a,
                    event.entity_b,
                    event.anchor_a,
                    event.anchor_b,
                    |parent| {
                        let circle_outer = GeometryBuilder::build_as(&shapes::Circle {
                            radius: 5.0,
                            ..default()
                        });
                        parent.spawn((
                            ShapeBundle {
                                path: circle_outer,
                                ..default()
                            },
                            Fill::color(Color::srgb(0.0, 0.0, 0.0)),
                        )).with_children(|parent| {
                            let circle_inner = GeometryBuilder::build_as(&shapes::Circle {
                                radius: 2.0,
                                ..default()
                            });
                            parent.spawn((
                                ShapeBundle {
                                    path: circle_inner,
                                    transform: Transform::from_translation(Vec3::Z * 0.1),
                                    ..default()
                                },
                                Fill::color(Color::srgb(1.0, 1.0, 1.0)),
                            ));
                        });
                    },
                );
                Some(vid)
            }
            JointType::Fixed { .. } => {
                let vid = spawn_connector_visual(
                    &mut commands,
                    event.entity_a,
                    event.entity_b,
                    event.anchor_a,
                    event.anchor_b,
                    |parent| {
                         let line1 = GeometryBuilder::build_as(&shapes::Line(
                            Vec2::new(-3.0, -3.0),
                            Vec2::new(3.0, 3.0),
                        ));
                        parent
                            .spawn((
                                ShapeBundle {
                                    path: line1,
                                    transform: Transform::from_translation(Vec3::Z * 0.1),
                                    ..default()
                                },
                                Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
                            ));

                        let line2 = GeometryBuilder::build_as(&shapes::Line(
                            Vec2::new(-3.0, 3.0),
                            Vec2::new(3.0, -3.0),
                        ));
                        parent
                            .spawn((
                                ShapeBundle {
                                    path: line2,
                                    transform: Transform::from_translation(Vec3::Z * 0.1),
                                    ..default()
                                },
                                Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
                            ));
                    }
                );
                Some(vid)
            }
            JointType::Prismatic { .. } => {
                let vid = spawn_connector_visual(
                    &mut commands,
                    event.entity_a,
                    event.entity_b,
                    event.anchor_a,
                    event.anchor_b,
                    |parent| {
                         let rect = GeometryBuilder::build_as(&shapes::Rectangle {
                            extents: Vec2::new(8.0, 4.0),
                            ..default()
                        });
                        parent.spawn((
                            ShapeBundle {
                                path: rect,
                                ..default()
                            },
                            Stroke::new(Color::srgb(0.0, 0.0, 1.0), 1.0),
                        ));
                    }
                );
                Some(vid)
            }
            JointType::Spring { .. } => {
                 let vid = spawn_connector_visual(
                    &mut commands,
                    event.entity_a,
                    event.entity_b,
                    event.anchor_a,
                    event.anchor_b,
                    |parent| {
                        let circle = GeometryBuilder::build_as(&shapes::Circle {
                            radius: 3.0,
                            ..default()
                        });
                        parent.spawn((
                            ShapeBundle {
                                path: circle,
                                ..default()
                            },
                            Fill::color(Color::srgb(0.0, 1.0, 0.0)),
                        ));
                    }
                );
                Some(vid)
            }
            JointType::Rope { .. } => {
                let vid = spawn_connector_visual(
                    &mut commands,
                    event.entity_a,
                    event.entity_b,
                    event.anchor_a,
                    event.anchor_b,
                    |parent| {
                        let circle = GeometryBuilder::build_as(&shapes::Circle {
                            radius: 3.0,
                            ..default()
                        });
                        parent.spawn((
                            ShapeBundle {
                                path: circle,
                                ..default()
                            },
                            Fill::color(Color::srgb(1.0, 0.5, 0.0)),
                        ));
                    }
                );
                Some(vid)
            }
        };

        let (target_entity, _pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            &mut commands,
            event.entity_a,
            event.entity_b,
            event.anchor_a,
            event.anchor_b,
            visual_id_opt,
        );

        let mut joint_data = match &event.joint_type {
            JointType::Revolute => {
                let builder = RapierRevoluteJointBuilder::new()
                    .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
                    .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
                 GenericJoint::from(builder)
            }
            JointType::Fixed { rot_a, rot_b } => {
                let frame2 = Isometry2::new(
                    Vector2::new(local_anchor_2.x, local_anchor_2.y),
                    rot_a - rot_b,
                );
                let builder = RapierFixedJointBuilder::new()
                    .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
                    .local_frame2(frame2);
                 GenericJoint::from(builder)
            }
            JointType::Prismatic { axis } => {
                let axis = Vector2::new(axis.x, axis.y);
                let unit_axis = nalgebra::Unit::new_normalize(axis);
                let builder = RapierPrismaticJointBuilder::new(unit_axis)
                    .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
                    .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
                GenericJoint::from(builder)
            }
            JointType::Spring { stiffness, damping, length } => {
                 let builder = RapierSpringJointBuilder::new(*length, *stiffness, *damping)
                    .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
                    .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
                GenericJoint::from(builder)
            }
            JointType::Rope { length } => {
                 let builder = RapierRopeJointBuilder::new(*length)
                    .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
                    .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
                GenericJoint::from(builder)
            }
        };

        joint_data.set_contacts_enabled(false);

        commands.entity(event.entity_a).insert(ImpulseJoint::new(
            target_entity,
            TypedJoint::GenericJoint(bevy_rapier2d::dynamics::GenericJoint { raw: joint_data }),
        ));
    }
}

/// System to handle `ModifyTransformEvent`.
pub fn handle_modify_transform(
    mut events: EventReader<ModifyTransformEvent>,
    mut commands: Commands,
    mut query: Query<&mut Transform>,
) {
    for event in events.read() {
        if let Ok(mut transform) = query.get_mut(event.entity) {
            if let Some(translation) = event.translation {
                transform.translation = translation;
            }
            if let Some(rotation) = event.rotation {
                transform.rotation = rotation;
            }
            // Wake up body
            commands.entity(event.entity).insert(Sleeping::disabled());
        }
    }
}

/// System to handle `ModifyPhysicsEvent`.
pub fn handle_modify_physics(
    mut events: EventReader<ModifyPhysicsEvent>,
    mut commands: Commands,
) {
    for event in events.read() {
        let e = event.entity;
        if let Some(body_type) = event.body_type {
            commands.entity(e).insert(body_type);
        }
        if let Some(friction) = event.friction {
            commands.entity(e).insert(Friction::coefficient(friction));
        }
        if let Some(restitution) = event.restitution {
            commands.entity(e).insert(Restitution::coefficient(restitution));
        }
        if let Some(density) = event.density {
            commands.entity(e).insert(ColliderMassProperties::Density(density));
        }
        if let Some(gravity) = event.gravity_scale {
            commands.entity(e).insert(GravityScale(gravity));
        }
        if let Some(locked) = event.locked_axes {
            commands.entity(e).insert(locked);
        }
        if let Some(is_sensor) = event.is_sensor {
            if is_sensor {
                commands.entity(e).insert(Sensor);
            } else {
                commands.entity(e).remove::<Sensor>();
            }
        }
        commands.entity(e).insert(Sleeping::disabled());
    }
}

/// System to handle `ModifyShapeEvent`.
pub fn handle_modify_shape(
    mut events: EventReader<ModifyShapeEvent>,
    mut commands: Commands,
    mut query: Query<(Option<&mut EditableBox>, Option<&mut EditableCircle>)>,
) {
    for event in events.read() {
        if let Ok((box_opt, circle_opt)) = query.get_mut(event.entity) {
            if let Some(mut b) = box_opt {
                if let Some(size) = event.box_size {
                    b.width = size.x as f64;
                    b.height = size.y as f64;
                    // TODO: Re-generate collider? For now simple data update.
                    // If we need to resize collider, we might need to remove and re-add Collider.
                    // But EditableBox implies it might just be data until something else reads it?
                    // Actually, usually we should update the collider too.
                    commands.entity(event.entity).insert(Collider::cuboid(size.x / 2.0, size.y / 2.0));
                }
            }
            if let Some(mut c) = circle_opt {
                if let Some(radius) = event.circle_radius {
                    c.radius = radius as f64;
                    commands.entity(event.entity).insert(Collider::ball(radius));
                }
            }
            commands.entity(event.entity).insert(Sleeping::disabled());
        }
    }
}

/// System to handle `ModifyRenderEvent`.
pub fn handle_modify_render(
    mut events: EventReader<ModifyRenderEvent>,
    mut query: Query<(Option<&mut Fill>, Option<&mut Stroke>)>,
) {
    for event in events.read() {
        if let Ok((fill_opt, stroke_opt)) = query.get_mut(event.entity) {
            if let Some(mut fill) = fill_opt {
                if let Some(color) = event.fill_color {
                    fill.color = color;
                }
            }
            if let Some(mut stroke) = stroke_opt {
                if let Some(color) = event.stroke_color {
                    stroke.color = color;
                }
                if let Some(width) = event.stroke_width {
                    stroke.options.line_width = width;
                }
            }
        }
    }
}
