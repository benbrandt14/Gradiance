use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

/// System to draw gizmos for joint limits and motors.
pub fn draw_joint_gizmos(
    mut gizmos: Gizmos,
    joints: Query<(&ImpulseJoint, &GlobalTransform)>,
    transforms: Query<&GlobalTransform>,
) {
    for (joint, t_a) in &joints {
        let parent = joint.parent;
        // Body B is joint.parent.
        if transforms.get(parent).is_err() {
            continue;
        }

        match &joint.data {
            TypedJoint::RevoluteJoint(rev) => {
                 let anchor_local = rev.local_anchor1();
                 let anchor_a_world = t_a.transform_point(Vec3::new(anchor_local.x, anchor_local.y, 0.0));

                 // Draw limits
                 if let Some(limits) = rev.limits() {
                     let rot_a = t_a.compute_transform().rotation.to_euler(EulerRot::XYZ).2;

                     let center = anchor_a_world.truncate();
                     let radius = 6.0;
                     let start = limits.min + rot_a;
                     let sweep = limits.max - limits.min;
                     let steps = 12;

                     for i in 0..steps {
                        let t1 = start + sweep * (i as f32) / (steps as f32);
                        let t2 = start + sweep * ((i + 1) as f32) / (steps as f32);
                        let p1 = center + Vec2::from_angle(t1) * radius;
                        let p2 = center + Vec2::from_angle(t2) * radius;
                        gizmos.line_2d(p1, p2, Color::srgb(0.0, 1.0, 1.0));
                     }

                     // Draw limit lines
                     let dir_min = Vec2::from_angle(limits.min + rot_a);
                     let dir_max = Vec2::from_angle(limits.max + rot_a);
                     gizmos.line_2d(center, center + dir_min * 6.0, Color::srgb(0.0, 1.0, 1.0));
                     gizmos.line_2d(center, center + dir_max * 6.0, Color::srgb(0.0, 1.0, 1.0));
                 }

                 // Draw motor
                 let raw_rev = rev.data.raw.as_revolute().unwrap();
                 let motor = &raw_rev.data.motors[2];
                 if motor.max_force > 0.0 && motor.target_vel.abs() > 0.01 {
                     let speed = motor.target_vel;
                     let color = if speed > 0.0 { Color::srgb(0.0, 1.0, 0.0) } else { Color::srgb(1.0, 0.0, 0.0) };

                     // Draw a small circle indicating active motor
                     gizmos.circle_2d(anchor_a_world.truncate(), 3.0, color);
                 }
            }
            TypedJoint::PrismaticJoint(prism) => {
                 let anchor_local = prism.local_anchor1();
                 let anchor_a_world = t_a.transform_point(Vec3::new(anchor_local.x, anchor_local.y, 0.0));

                 // Axis
                 let axis_local = prism.local_axis1(); // Local to A
                 // Transform vector
                 let axis_world = t_a.affine().transform_vector3(Vec3::new(axis_local.x, axis_local.y, 0.0)).truncate().normalize_or_zero();

                 // Draw axis guideline
                 gizmos.line_2d(
                    anchor_a_world.truncate() - axis_world * 20.0,
                    anchor_a_world.truncate() + axis_world * 20.0,
                    Color::srgb(0.5, 0.5, 0.5).with_alpha(0.5)
                 );

                 if let Some(limits) = prism.limits() {
                     // Draw limit segment
                     let p1 = anchor_a_world.truncate() + axis_world * limits.min;
                     let p2 = anchor_a_world.truncate() + axis_world * limits.max;

                     gizmos.line_2d(p1, p2, Color::srgb(0.0, 1.0, 1.0));

                     // Endcaps
                     let perp = Vec2::new(-axis_world.y, axis_world.x) * 3.0;
                     gizmos.line_2d(p1 - perp, p1 + perp, Color::srgb(0.0, 1.0, 1.0));
                     gizmos.line_2d(p2 - perp, p2 + perp, Color::srgb(0.0, 1.0, 1.0));
                 }

                 let raw_prism = prism.data.raw.as_prismatic().unwrap();
                 let motor = &raw_prism.data.motors[0];
                 if motor.max_force > 0.0 && motor.target_vel.abs() > 0.01 {
                     let vel = motor.target_vel;
                     let start = anchor_a_world.truncate();
                     let dir = if vel > 0.0 { axis_world } else { -axis_world };

                     gizmos.arrow_2d(start, start + dir * 15.0, Color::srgb(0.0, 1.0, 0.0));
                 }
            }
            _ => {}
        }
    }
}
