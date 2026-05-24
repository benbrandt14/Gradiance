import re

with open('src/physics/controllers.rs', 'r') as f:
    content = f.read()

# Update get_angle and get_dist to properly handle anchors
pattern = r'''
        // Helper to get angle
        let get_angle = \|e_a, e_b, transforms: &Query<&GlobalTransform>\| \{
             if let Ok\(t_a\) = transforms\.get\(e_a\)
                && let Ok\(t_b\) = transforms\.get\(e_b\) \{
                let rot_a = t_a\.to_scale_rotation_translation\(\)\.1\.to_euler\(EulerRot::XYZ\)\.2;
                let rot_b = t_b\.to_scale_rotation_translation\(\)\.1\.to_euler\(EulerRot::XYZ\)\.2;
                rot_b - rot_a
            \} else \{
                0\.0
            \}
        \};

        let get_dist = \|e_a, e_b, transforms: &Query<&GlobalTransform>\| \{
             if let Ok\(t_a\) = transforms\.get\(e_a\)
                && let Ok\(t_b\) = transforms\.get\(e_b\) \{
                let diff = t_b\.translation\(\) - t_a\.translation\(\);
                diff\.length\(\)
            \} else \{
                0\.0
            \}
        \};'''

replacement = r'''
        // Helper to get angle
        let get_angle = |e_a, e_b, local_anchor1: Vec2, local_anchor2: Vec2, transforms: &Query<&GlobalTransform>| {
             if let Ok(t_a) = transforms.get(e_a)
                && let Ok(t_b) = transforms.get(e_b) {
                // The angle is relative rotation, local anchors don't affect rotation difference,
                // but we should compute it consistently.
                let rot_a = t_a.compute_transform().rotation.to_euler(EulerRot::XYZ).2;
                let rot_b = t_b.compute_transform().rotation.to_euler(EulerRot::XYZ).2;

                // Keep it in [-PI, PI] range
                let mut diff = rot_a - rot_b;
                while diff > std::f32::consts::PI {
                    diff -= std::f32::consts::TAU;
                }
                while diff < -std::f32::consts::PI {
                    diff += std::f32::consts::TAU;
                }
                diff
            } else {
                0.0
            }
        };

        let get_dist = |e_a, e_b, local_anchor1: Vec2, local_anchor2: Vec2, transforms: &Query<&GlobalTransform>| {
             if let Ok(t_a) = transforms.get(e_a)
                && let Ok(t_b) = transforms.get(e_b) {

                let world_anchor1 = t_a.compute_transform().transform_point(local_anchor1.extend(0.0));
                let world_anchor2 = t_b.compute_transform().transform_point(local_anchor2.extend(0.0));

                let diff = world_anchor1 - world_anchor2;
                diff.length()
            } else {
                0.0
            }
        };'''

content = re.sub(pattern, replacement, content, flags=re.DOTALL)


# Update the callers
calls = [
    (r'\(get_angle\(entity, joint\.parent, &transforms\), min, max\)', r'(get_angle(entity, joint.parent, rev.local_anchor1, rev.local_anchor2, &transforms), min, max)'),
    (r'\(get_dist\(entity, joint\.parent, &transforms\), min, max\)', r'(get_dist(entity, joint.parent, prism.local_anchor1, prism.local_anchor2, &transforms), min, max)'),
    (r'\(get_angle\(entity, joint\.parent, &transforms\), min, max\)', r'(get_angle(entity, joint.parent, g.local_anchor1, g.local_anchor2, &transforms), min, max)'),
    (r'\(get_dist\(entity, joint\.parent, &transforms\), min, max\)', r'(get_dist(entity, joint.parent, g.local_anchor1, g.local_anchor2, &transforms), min, max)')
]

for call in calls:
    content = re.sub(call[0], call[1], content)


with open('src/physics/controllers.rs', 'w') as f:
    f.write(content)
