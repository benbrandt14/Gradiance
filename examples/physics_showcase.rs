use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use bevy_rapier2d::rapier; // Access to raw rapier
use bevy_rapier2d::rapier::parry; // Access to raw parry
use bevy_rapier2d::rapier::parry::shape::Shape; // Trait needed for compute_local_aabb
// #[allow(unused_imports)]
// use salva2d::{FluidPipeline, LiquidWorld}; // Salva for fluids (Commented out due to API version mismatch)

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_systems(Startup, (setup_graphics, setup_physics))
        // .add_systems(Startup, setup_salva) // Salva setup disabled
        .add_systems(Update, (cast_ray, kinematic_controller))
        .run();
}

// #[derive(Resource)]
// struct SalvaContext {
//     #[allow(dead_code)]
//     pipeline: FluidPipeline,
//     #[allow(dead_code)]
//     liquid_world: LiquidWorld,
// }

// fn setup_salva(mut commands: Commands) {
//     // Basic Salva setup (Skeleton)
//     // Note: Salva 0.9.0 API differs significantly from newer versions and has dependency conflicts (nalgebra 0.32 vs 0.33).
//     //
//     // let pipeline = FluidPipeline::new(0.5, 1.0);
//     // let mut liquid_world = LiquidWorld::new();
//     //
//     // commands.insert_resource(SalvaContext {
//     //     pipeline,
//     //     liquid_world,
//     // });
//     // info!("Salva Fluid System Initialized (Resource Only)");
// }

fn setup_graphics(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_physics(mut commands: Commands) {
    // Ground
    commands
        .spawn((
            RigidBody::Fixed,
            Collider::cuboid(500.0, 50.0),
            Transform::from_xyz(0.0, -300.0, 0.0),
        ));

    // --- RigidBodies & Colliders ---

    // 1. Dynamic Box
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(20.0, 20.0),
        Transform::from_xyz(-200.0, 200.0, 0.0),
    ));

    // 2. Bouncy Ball
    commands.spawn((
        RigidBody::Dynamic,
        Collider::ball(20.0),
        Restitution::coefficient(0.9),
        Transform::from_xyz(-100.0, 200.0, 0.0),
    ));

    // 3. Friction Box & Ramp
    // Ramp
    commands.spawn((
        RigidBody::Fixed,
        Collider::cuboid(100.0, 10.0),
        Transform::from_xyz(0.0, -100.0, 0.0).with_rotation(Quat::from_rotation_z(0.3)),
    ));
    // Box on ramp
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(20.0, 20.0),
        Friction::coefficient(0.1), // Low friction to slide
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // 4. Kinematic Platform (Moving)
    commands.spawn((
        RigidBody::KinematicVelocityBased,
        Collider::cuboid(60.0, 10.0),
        Velocity::linear(Vec2::new(0.0, 50.0)), // Initial velocity
        Transform::from_xyz(200.0, 0.0, 0.0),
        KinematicPlatform, // Marker component
    ));

    // 5. Polygon
    let points = vec![
        Vec2::new(-20.0, -20.0),
        Vec2::new(20.0, -20.0),
        Vec2::new(0.0, 20.0),
    ];
    commands.spawn((
        RigidBody::Dynamic,
        Collider::convex_hull(&points).unwrap(),
        Transform::from_xyz(300.0, 200.0, 0.0),
    ));

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
    // Indices for the polygon edges (0-1, 1-2, etc.)
    let indices: Vec<[u32; 2]> = (0..concave_points.len())
        .map(|i| [i as u32, ((i + 1) % concave_points.len()) as u32])
        .collect();

    commands.spawn((
        RigidBody::Dynamic,
        Collider::convex_decomposition(&concave_points, &indices),
        Transform::from_xyz(400.0, 200.0, 0.0),
    ));

    // --- Parry Direct Interaction Example ---
    // Calculating properties of a shape directly using Parry
    let parry_cuboid = parry::shape::Cuboid::new(rapier::math::Vector::new(10.0, 10.0));
    let aabb = parry_cuboid.compute_local_aabb();
    info!("Direct Parry Interaction - Computed AABB: {:?}", aabb);


    // --- Constraints (Joints) ---
    // Positioned higher up to avoid clutter

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
    )).id();

    let joint = RevoluteJointBuilder::new()
        .local_anchor1(Vec2::new(0.0, 0.0))
        .local_anchor2(Vec2::new(0.0, 100.0)); // Anchor point relative to bob is above it

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
    )).id();

    let joint = PrismaticJointBuilder::new(Vec2::Y) // Slide along Y axis
        .local_anchor1(Vec2::new(20.0, 0.0))
        .local_anchor2(Vec2::new(0.0, 0.0))
        .limits([-50.0, 50.0]); // Limit slider range

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
    )).id();

    let joint = SpringJointBuilder::new(100.0, 50.0, 5.0) // rest_length, stiffness, damping
        .local_anchor1(Vec2::ZERO)
        .local_anchor2(Vec2::ZERO);

    commands.entity(weight_spring).insert(ImpulseJoint::new(anchor_spring, joint));
}

#[derive(Component)]
struct KinematicPlatform;

fn kinematic_controller(
    mut query: Query<(&mut Velocity, &Transform), With<KinematicPlatform>>,
) {
    for (mut velocity, transform) in query.iter_mut() {
        if transform.translation.y > 100.0 {
            velocity.linvel.y = -50.0;
        } else if transform.translation.y < -100.0 {
            velocity.linvel.y = 50.0;
        }
    }
}

fn cast_ray(
    rapier_context: Query<&RapierContext>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    // Only works if RapierContext is available (it should be)
    if let Ok(context) = rapier_context.get_single() {
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

        // Point projection example: Check if a point is inside any collider (e.g., inside ground)
        let point = Vec2::new(0.0, -300.0); // Inside ground
        let project_filter = QueryFilter::default();
        if let Some((_entity, projection)) = context.project_point(point, true, project_filter) {
             if projection.is_inside {
                 gizmos.circle_2d(point, 10.0, Color::srgb(1.0, 1.0, 0.0));
             }
        }
    }
}
