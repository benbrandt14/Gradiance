use bevy::prelude::*;
use bevy_rapier2d::prelude as rapier2d;
use bevy_rapier3d::prelude as rapier3d;
use gradiance::GamePlugin;

// App States
#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
enum AppMode {
    #[default]
    Mode2D,
    Mode3D,
}

// Components
#[derive(Component)]
struct SimPlane {
    index: usize,
}

#[derive(Component)]
struct Sim3dEntity;

// Resources
#[derive(Resource, Default)]
struct ActivePlane(usize);

#[derive(Resource, Default)]
struct PlanesData(Vec<PlaneData>);

struct PlaneData {
    transform: Transform,
    entities: Vec<EntitySnapshot>,
}

#[derive(Clone)]
struct EntitySnapshot {
    position: Vec2,
    rotation: f32,
    shape: SimplifiedShape,
    color: Color,
}

#[derive(Clone)]
enum SimplifiedShape {
    Cuboid { half_extents: Vec2 },
    Ball { radius: f32 },
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GamePlugin)
        .add_plugins(rapier3d::RapierPhysicsPlugin::<rapier3d::NoUserData>::default())
        .add_plugins(rapier3d::RapierDebugRenderPlugin::default())
        .configure_sets(
            PostUpdate,
            (
                rapier2d::PhysicsSet::StepSimulation.run_if(in_state(AppMode::Mode2D)),
                rapier2d::PhysicsSet::Writeback.run_if(in_state(AppMode::Mode2D)),
            ),
        )
        .configure_sets(
            PostUpdate,
            (
                rapier3d::PhysicsSet::StepSimulation.run_if(in_state(AppMode::Mode3D)),
                rapier3d::PhysicsSet::Writeback.run_if(in_state(AppMode::Mode3D)),
            ),
        )
        .init_state::<AppMode>()
        .init_resource::<ActivePlane>()
        .init_resource::<PlanesData>()
        .add_systems(Startup, setup_example)
        .add_systems(Update, toggle_mode)
        .add_systems(
            Update,
            (plane_picking, plane_movement, draw_active_plane_gizmo).run_if(in_state(AppMode::Mode3D)),
        )
        .add_systems(OnEnter(AppMode::Mode3D), enter_3d)
        .add_systems(OnEnter(AppMode::Mode2D), enter_2d)
        .run();
}

fn plane_picking(
    q_camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    rapier_context: Query<&rapier3d::RapierContext>,
    q_planes: Query<(Entity, &SimPlane)>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut next_state: ResMut<NextState<AppMode>>,
    mut active_plane: ResMut<ActivePlane>,
    windows: Query<&Window>,
) {
    let Ok(window) = windows.get_single() else { return };
    let Ok((camera, camera_transform)) = q_camera.get_single() else { return };

    if let Some(cursor_position) = window.cursor_position() {
        if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) {
            let Ok(context) = rapier_context.get_single() else { return };

            if let Some((entity, _toi)) = context.cast_ray(
                ray.origin,
                *ray.direction,
                f32::MAX,
                true,
                rapier3d::QueryFilter::default(),
            ) {
                if let Ok((_e, plane)) = q_planes.get(entity) {
                    if mouse_button.just_pressed(MouseButton::Right) {
                        active_plane.0 = plane.index;
                        next_state.set(AppMode::Mode2D);
                        info!("Editing Plane {}", plane.index);
                    } else if mouse_button.just_pressed(MouseButton::Left) {
                         active_plane.0 = plane.index;
                         info!("Selected Plane {}", plane.index);
                    }
                }
            }
        }
    }
}

fn plane_movement(
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    active_plane: Res<ActivePlane>,
    mut planes_data: ResMut<PlanesData>,
    mut q_planes: Query<(&SimPlane, &mut Transform)>,
) {
    let speed = 10.0;
    let mut delta = Vec3::ZERO;
    if input.pressed(KeyCode::KeyW) { delta.z -= 1.0; }
    if input.pressed(KeyCode::KeyS) { delta.z += 1.0; }
    if input.pressed(KeyCode::KeyA) { delta.x -= 1.0; }
    if input.pressed(KeyCode::KeyD) { delta.x += 1.0; }
    if input.pressed(KeyCode::KeyQ) { delta.y += 1.0; }
    if input.pressed(KeyCode::KeyE) { delta.y -= 1.0; }

    let rot_speed = 2.0;
    let mut rot_delta = 0.0;
    if input.pressed(KeyCode::ArrowLeft) { rot_delta += 1.0; }
    if input.pressed(KeyCode::ArrowRight) { rot_delta -= 1.0; }

    if delta == Vec3::ZERO && rot_delta == 0.0 { return; }

    if let Some(plane_data) = planes_data.0.get_mut(active_plane.0) {
        plane_data.transform.translation += delta * speed * time.delta_secs();
        plane_data.transform.rotate_y(rot_delta * rot_speed * time.delta_secs());

        for (sim_plane, mut transform) in &mut q_planes {
            if sim_plane.index == active_plane.0 {
                *transform = plane_data.transform;
            }
        }
    }
}

fn draw_active_plane_gizmo(
    mut gizmos: Gizmos,
    active_plane: Res<ActivePlane>,
    planes_data: Res<PlanesData>,
) {
    if let Some(plane_data) = planes_data.0.get(active_plane.0) {
        gizmos.cuboid(plane_data.transform, Color::srgb(1.0, 1.0, 0.0));
    }
}

fn toggle_mode(
    mut next_state: ResMut<NextState<AppMode>>,
    state: Res<State<AppMode>>,
    input: Res<ButtonInput<KeyCode>>,
) {
    if input.just_pressed(KeyCode::Space) {
        match state.get() {
            AppMode::Mode2D => next_state.set(AppMode::Mode3D),
            AppMode::Mode3D => next_state.set(AppMode::Mode2D),
        }
    }
}

fn enter_3d(
    mut commands: Commands,
    mut planes_data: ResMut<PlanesData>,
    active_plane: Res<ActivePlane>,
    q_2d_entities: Query<(Entity, &Transform, &rapier2d::Collider), With<rapier2d::RigidBody>>,
    mut q_cam_2d: Query<&mut Camera, (With<Camera2d>, Without<Camera3d>)>,
    mut q_cam_3d: Query<&mut Camera, (With<Camera3d>, Without<Camera2d>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // 1. Save 2D scene
    let mut snapshots = Vec::new();
    for (e, t, c) in &q_2d_entities {
        let shape = match c.raw.as_typed_shape() {
            bevy_rapier2d::rapier::parry::shape::TypedShape::Cuboid(c) => SimplifiedShape::Cuboid {
                half_extents: Vec2::new(c.half_extents.x, c.half_extents.y),
            },
            bevy_rapier2d::rapier::parry::shape::TypedShape::Ball(b) => SimplifiedShape::Ball {
                radius: b.radius,
            },
            _ => continue,
        };
        snapshots.push(EntitySnapshot {
            position: t.translation.truncate(),
            rotation: t.rotation.to_euler(EulerRot::XYZ).2,
            shape,
            color: Color::srgb(0.8, 0.2, 0.2),
        });

        commands.entity(e).despawn_recursive();
    }

    if let Some(plane) = planes_data.0.get_mut(active_plane.0) {
        plane.entities = snapshots;
    }

    // 2. Spawn 3D scene
    let mut plane_entities_ids = Vec::new();

    for (i, plane_data) in planes_data.0.iter().enumerate() {
        // Spawn Plane Visual + RB
        let plane_entity = commands.spawn((
            Mesh3d(meshes.add(Mesh::from(Cuboid::new(20.0, 0.1, 20.0)))),
            MeshMaterial3d(materials.add(StandardMaterial::from(Color::WHITE.with_alpha(0.2)))),
            Transform::from(plane_data.transform),
            rapier3d::RigidBody::Fixed,
            SimPlane { index: i },
            Name::new(format!("Plane {}", i)),
        )).id();

        let mut current_plane_ids = Vec::new();

        // Spawn Entities on this plane
        for snap in &plane_data.entities {
            let (collider, mesh) = match snap.shape {
                SimplifiedShape::Cuboid { half_extents } => (
                    rapier3d::Collider::cuboid(half_extents.x, half_extents.y, 0.5),
                    Mesh::from(Cuboid::new(half_extents.x * 2.0, half_extents.y * 2.0, 1.0)),
                ),
                SimplifiedShape::Ball { radius } => (
                    rapier3d::Collider::cylinder(0.5, radius),
                    Mesh::from(Cylinder::new(radius, 1.0)),
                ),
            };

            let local_pos = Vec3::new(snap.position.x, snap.position.y, 0.0);
            let local_rot = Quat::from_rotation_z(snap.rotation);
            let world_transform = plane_data.transform * Transform::from_translation(local_pos).with_rotation(local_rot);

            let entity_3d = commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(materials.add(StandardMaterial::from(snap.color))),
                Transform::from(world_transform),
                rapier3d::RigidBody::Dynamic,
                collider,
                Sim3dEntity,
            )).id();

            current_plane_ids.push(entity_3d);

            // Planar constraint (GenericJoint)
            // Lock Z translation, X/Y rotation relative to plane.
            let joint = rapier3d::GenericJointBuilder::new(rapier3d::JointAxesMask::LOCKED_FIXED_AXES)
                .locked_axes(rapier3d::JointAxesMask::LIN_Z | rapier3d::JointAxesMask::ANG_X | rapier3d::JointAxesMask::ANG_Y)
                .local_anchor1(local_pos)
                .local_anchor2(Vec3::ZERO)
                .build();

            commands.entity(entity_3d).insert(rapier3d::ImpulseJoint::new(plane_entity, rapier3d::TypedJoint::GenericJoint(joint)));
        }
        plane_entities_ids.push(current_plane_ids);
    }

    // Demo: Cross-plane constraint (Spring)
    // Connect first entity of Plane 0 to first entity of Plane 1
    if plane_entities_ids.len() >= 2 && !plane_entities_ids[0].is_empty() && !plane_entities_ids[1].is_empty() {
        let e1 = plane_entities_ids[0][0];
        let e2 = plane_entities_ids[1][0];

        // Spawn a spring between them
        let joint = rapier3d::SpringJointBuilder::new(0.0, 10.0, 0.5) // rest_length, stiffness, damping
            .local_anchor1(Vec3::ZERO)
            .local_anchor2(Vec3::ZERO)
            .build();

        // We need to attach this joint to one of the entities, targeting the other.
        // We can add a SECOND ImpulseJoint component?
        // Bevy Rapier usually supports one ImpulseJoint component per entity (parent relationship).
        // If we want multiple joints, we should spawn a child entity to hold the joint, or use MultibodyJoint.
        // For simplicity, let's spawn a child of e1 that has the joint to e2.

        let joint_entity = commands.spawn((
            rapier3d::ImpulseJoint::new(e2, rapier3d::TypedJoint::SpringJoint(joint)),
            Transform::default(),
            GlobalTransform::default(),
        )).id();

        commands.entity(e1).add_child(joint_entity);
        info!("Created cross-plane spring between {:?} and {:?}", e1, e2);
    }

    // Disable 2D camera
    for mut cam in &mut q_cam_2d {
        cam.is_active = false;
    }
    // Enable 3D camera
    for mut cam in &mut q_cam_3d {
        cam.is_active = true;
    }

    info!("Switched to 3D Mode");
}

fn enter_2d(
    mut commands: Commands,
    planes_data: Res<PlanesData>,
    active_plane: Res<ActivePlane>,
    q_3d_entities: Query<Entity, Or<(With<SimPlane>, With<Sim3dEntity>)>>,
    mut q_cam_2d: Query<&mut Camera, (With<Camera2d>, Without<Camera3d>)>,
    mut q_cam_3d: Query<&mut Camera, (With<Camera3d>, Without<Camera2d>)>,
) {
    // Despawn 3D
    for e in &q_3d_entities {
        commands.entity(e).despawn_recursive();
    }

    // Spawn 2D
    if let Some(plane) = planes_data.0.get(active_plane.0) {
        for snap in &plane.entities {
            let collider = match snap.shape {
                SimplifiedShape::Cuboid { half_extents } => rapier2d::Collider::cuboid(half_extents.x, half_extents.y),
                SimplifiedShape::Ball { radius } => rapier2d::Collider::ball(radius),
            };

            commands.spawn((
                Transform::from_xyz(snap.position.x, snap.position.y, 0.0)
                    .with_rotation(Quat::from_rotation_z(snap.rotation)),
                rapier2d::RigidBody::Dynamic,
                collider,
            ));
        }
    }

    // Enable 2D camera
    for mut cam in &mut q_cam_2d {
        cam.is_active = true;
    }
    // Disable 3D camera
    for mut cam in &mut q_cam_3d {
        cam.is_active = false;
    }

    info!("Switched to 2D Mode");
}

fn setup_example(mut commands: Commands, mut planes_data: ResMut<PlanesData>) {
    // Initial planes
    // Plane 0: Active 2D
    planes_data.0.push(PlaneData {
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        entities: vec![],
    });
    // Plane 1: Background 3D
    planes_data.0.push(PlaneData {
        transform: Transform::from_xyz(5.0, 2.0, -5.0).looking_at(Vec3::ZERO, Vec3::Y),
        entities: vec![],
    });

    // Populate Plane 0 with 2D entities (Directly in World)
    commands.spawn((
        Transform::from_xyz(0.0, 5.0, 0.0),
        rapier2d::RigidBody::Dynamic,
        rapier2d::Collider::cuboid(1.0, 1.0),
        // No visualization components added here assuming debug render or user tool usage
        Name::new("Box2D"),
    ));

    // Populate Plane 1 with data (for 3D reconstruction)
    planes_data.0[1].entities.push(EntitySnapshot {
        position: Vec2::new(2.0, 5.0),
        rotation: 0.0,
        shape: SimplifiedShape::Ball { radius: 1.0 },
        color: Color::srgb(0.2, 0.2, 0.8),
    });

    // Setup 3D camera (disabled initially)
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 10.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
        Camera {
            is_active: false,
            ..default()
        },
        Name::new("Camera3D"),
    ));

    // Light
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(10.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
