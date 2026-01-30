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

        // Unified gizmo drawing for both Typed and Generic joints
        let (anchor_local, axis_local, limits, motor_data, is_prismatic) = match &joint.data {
            TypedJoint::RevoluteJoint(rev) => {
                let raw = rev.data.raw.as_revolute().unwrap();
                (rev.local_anchor1(), Vec2::X, rev.limits(), &raw.data.motors[2], false)
            }
            TypedJoint::PrismaticJoint(prism) => {
                let raw = prism.data.raw.as_prismatic().unwrap();
                (prism.local_anchor1(), prism.local_axis1(), prism.limits(), &raw.data.motors[0], true)
            }
            TypedJoint::GenericJoint(generic) => {
                if generic.locked_axes() == JointAxesMask::LOCKED_REVOLUTE_AXES {
                    // Revolute - Index 2
                    (generic.local_anchor1(), Vec2::X, Some(&generic.raw.limits[2]), &generic.raw.motors[2], false)
                } else if generic.locked_axes() == JointAxesMask::LOCKED_PRISMATIC_AXES {
                    // Prismatic (assuming X axis) - Index 0
                    (generic.local_anchor1(), generic.local_axis1(), Some(&generic.raw.limits[0]), &generic.raw.motors[0], true)
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        let anchor_a_world = t_a.transform_point(Vec3::new(anchor_local.x, anchor_local.y, 0.0));

        if is_prismatic {
             // Axis
             // Transform vector
             let axis_world = t_a.affine().transform_vector3(Vec3::new(axis_local.x, axis_local.y, 0.0)).truncate().normalize_or_zero();

             // Draw axis guideline
             gizmos.line_2d(
                anchor_a_world.truncate() - axis_world * 20.0,
                anchor_a_world.truncate() + axis_world * 20.0,
                Color::srgb(0.5, 0.5, 0.5).with_alpha(0.5)
             );

             if let Some(limits) = limits {
                 // Draw limit segment
                 let p1 = anchor_a_world.truncate() + axis_world * limits.min;
                 let p2 = anchor_a_world.truncate() + axis_world * limits.max;

                 gizmos.line_2d(p1, p2, Color::srgb(0.0, 1.0, 1.0));

                 // Endcaps
                 let perp = Vec2::new(-axis_world.y, axis_world.x) * 3.0;
                 gizmos.line_2d(p1 - perp, p1 + perp, Color::srgb(0.0, 1.0, 1.0));
                 gizmos.line_2d(p2 - perp, p2 + perp, Color::srgb(0.0, 1.0, 1.0));

                 // Current position tick (at anchor)
                 let center = anchor_a_world.truncate();
                 gizmos.line_2d(center - perp, center + perp, Color::WHITE);
             }

             if motor_data.max_force > 0.0 && motor_data.target_vel.abs() > 0.01 {
                 let vel = motor_data.target_vel;
                 let start = anchor_a_world.truncate();
                 let dir = if vel > 0.0 { axis_world } else { -axis_world };

                 gizmos.arrow_2d(start, start + dir * 15.0, Color::srgb(0.0, 1.0, 0.0));
             }
        } else {
             // Revolute
             // Draw limits
             if let Some(limits) = limits {
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

                 // Current position indicator
                 let dir_zero = Vec2::from_angle(rot_a);
                 gizmos.line_2d(center, center + dir_zero * 5.0, Color::WHITE);
             }

             // Draw motor
             if motor_data.max_force > 0.0 && motor_data.target_vel.abs() > 0.01 {
                 let speed = motor_data.target_vel;
                 let color = if speed > 0.0 { Color::srgb(0.0, 1.0, 0.0) } else { Color::srgb(1.0, 0.0, 0.0) };

                 // Draw a small circle indicating active motor
                 gizmos.circle_2d(anchor_a_world.truncate(), 3.0, color);
             }
        }
    }
}
