use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use bevy_rapier2d::rapier; // Access to raw rapier
use bevy_rapier2d::rapier::parry; // Access to raw parry
use bevy_rapier2d::rapier::parry::shape::Shape; // Trait needed for compute_local_aabb

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_systems(Startup, (setup_graphics, setup_physics))
        .add_systems(Update, (cast_ray, joint_interactivity))
        .run();
}

fn setup_graphics(mut commands: Commands) {
    commands.spawn(Camera2d);
}

// Markers for interactive joints
#[derive(Component)]
struct PendulumBob;

#[derive(Component)]
struct SliderBody;

#[derive(Component)]
struct SpringWeight;

fn setup_physics(mut commands: Commands) {
    // Instructions
    commands.spawn((
        Text::new(
            "Controls:\n\
            1: Push Pendulum\n\
            2: Push Slider\n\
            3: Pull Spring\n\
            Space: Reset Scene (Not Implemented)",
        ),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));

    // Ground
    commands
        .spawn((
            RigidBody::Fixed,
            Collider::cuboid(500.0, 50.0),
            Transform::from_xyz(0.0, -300.0, 0.0),
        ));

    // --- Constraints (Joints) ---

    // 1. Revolute Joint (Pendulum)
    let anchor_revolute = commands.spawn((
        RigidBody::Fixed,
        Collider::ball(5.0),
        Transform::from_xyz(-400.0, 300.0, 0.0),
    )).id();

    let bob_revolute = commands.spawn((
        RigidBody::Dynamic,
        Collider::ball(15.0),
        Transform::from_xyz(-400.0, 200.0, 0.0),
        PendulumBob, // Marker
    )).id();

    let joint = RevoluteJointBuilder::new()
        .local_anchor1(Vec2::new(0.0, 0.0))
        .local_anchor2(Vec2::new(0.0, 100.0));

    commands.entity(bob_revolute).insert(ImpulseJoint::new(anchor_revolute, joint));


    // 2. Prismatic Joint (Slider)
    let anchor_prismatic = commands.spawn((
        RigidBody::Fixed,
        Collider::cuboid(5.0, 50.0),
        Transform::from_xyz(-300.0, 300.0, 0.0),
    )).id();

    let slider = commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(20.0, 10.0),
        Transform::from_xyz(-280.0, 300.0, 0.0),
        SliderBody, // Marker
    )).id();

    let joint = PrismaticJointBuilder::new(Vec2::Y)
        .local_anchor1(Vec2::new(20.0, 0.0))
        .local_anchor2(Vec2::new(0.0, 0.0))
        .limits([-50.0, 50.0]);

    commands.entity(slider).insert(ImpulseJoint::new(anchor_prismatic, joint));


    // 3. Fixed Joint (Weld)
    let box1 = commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(15.0, 15.0),
        Transform::from_xyz(-200.0, 400.0, 0.0),
    )).id();

    let box2 = commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(15.0, 15.0),
        Transform::from_xyz(-200.0, 440.0, 0.0),
    )).id();

    let joint = FixedJointBuilder::new()
        .local_anchor1(Vec2::new(0.0, 20.0))
        .local_anchor2(Vec2::new(0.0, -20.0));

    commands.entity(box2).insert(ImpulseJoint::new(box1, joint));


    // 4. Rope Joint
    let anchor_rope = commands.spawn((
        RigidBody::Fixed,
        Collider::ball(5.0),
        Transform::from_xyz(0.0, 400.0, 0.0),
    )).id();

    let weight_rope = commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(15.0, 15.0),
        Transform::from_xyz(50.0, 350.0, 0.0), // Start offset
    )).id();

    let joint = RopeJointBuilder::new(100.0) // Max length
        .local_anchor1(Vec2::ZERO)
        .local_anchor2(Vec2::ZERO);

    commands.entity(weight_rope).insert(ImpulseJoint::new(anchor_rope, joint));


    // 5. Distance Joint (Spring)
    let anchor_spring = commands.spawn((
        RigidBody::Fixed,
        Collider::ball(5.0),
        Transform::from_xyz(100.0, 400.0, 0.0),
    )).id();

    let weight_spring = commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(15.0, 15.0),
        Transform::from_xyz(100.0, 300.0, 0.0),
        SpringWeight, // Marker
    )).id();

    let joint = SpringJointBuilder::new(100.0, 50.0, 5.0) // rest_length, stiffness, damping
        .local_anchor1(Vec2::ZERO)
        .local_anchor2(Vec2::ZERO);

    commands.entity(weight_spring).insert(ImpulseJoint::new(anchor_spring, joint));

    // 6. Concave Polygon (Decomposition)
    // A 'U' shape
    let concave_points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(50.0, 0.0),
        Vec2::new(50.0, 50.0),
        Vec2::new(40.0, 50.0),
        Vec2::new(40.0, 10.0),
        Vec2::new(10.0, 10.0),
        Vec2::new(10.0, 50.0),
        Vec2::new(0.0, 50.0),
    ];
    let indices: Vec<[u32; 2]> = (0..concave_points.len())
        .map(|i| [i as u32, ((i + 1) % concave_points.len()) as u32])
        .collect();

    commands.spawn((
        RigidBody::Dynamic,
        Collider::convex_decomposition(&concave_points, &indices),
        Transform::from_xyz(400.0, 200.0, 0.0),
    ));

    // --- Parry Direct Interaction Example ---
    let parry_cuboid = parry::shape::Cuboid::new(rapier::math::Vector::new(10.0, 10.0));
    let aabb = parry_cuboid.compute_local_aabb();
    info!("Direct Parry Interaction - Computed AABB: {:?}", aabb);
}

fn joint_interactivity(
    input: Res<ButtonInput<KeyCode>>,
    mut pendulum: Query<&mut ExternalImpulse, With<PendulumBob>>,
    mut slider: Query<&mut ExternalImpulse, (With<SliderBody>, Without<PendulumBob>)>,
    mut spring: Query<&mut ExternalImpulse, (With<SpringWeight>, Without<PendulumBob>, Without<SliderBody>)>,
) {
    if input.just_pressed(KeyCode::Digit1) {
        if let Ok(mut impulse) = pendulum.get_single_mut() {
            impulse.impulse = Vec2::new(10000.0, 0.0);
        }
    }
    if input.just_pressed(KeyCode::Digit2) {
        if let Ok(mut impulse) = slider.get_single_mut() {
            impulse.impulse = Vec2::new(0.0, 5000.0);
        }
    }
    if input.just_pressed(KeyCode::Digit3) {
        if let Ok(mut impulse) = spring.get_single_mut() {
            impulse.impulse = Vec2::new(0.0, -5000.0);
        }
    }
}

fn cast_ray(
    context: Query<&RapierContext>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    if let Ok(context) = context.get_single() {
        let origin = Vec2::new(0.0, 0.0);
        let angle = time.elapsed_secs().cos();
        let direction = Vec2::new(angle.sin(), angle.cos()).normalize();
        let max_toi = 500.0;
        let solid = true;
        let filter = QueryFilter::default();

        if let Some((_entity, toi)) = context.cast_ray(origin, direction, max_toi, solid, filter) {
            let hit_point = origin + direction * toi;
            gizmos.line_2d(origin, hit_point, Color::srgb(0.0, 1.0, 0.0));
            gizmos.circle_2d(hit_point, 5.0, Color::srgb(1.0, 0.0, 0.0));
        } else {
            gizmos.line_2d(origin, origin + direction * max_toi, Color::srgb(0.0, 0.0, 1.0));
        }
    }
}
