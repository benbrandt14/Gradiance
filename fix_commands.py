import re

with open('src/input/commands.rs', 'r') as f:
    content = f.read()

# Add original_collision_groups to SpawnFixedJointCommand
pattern = re.compile(r'(pub struct SpawnFixedJointCommand \{.*?)(\s+pub original_solver_groups: Option<SolverGroups>,)(\s+\})', re.DOTALL)
content = pattern.sub(r'\1\2\n    /// Previous collision groups of the pinned body.\n    pub original_collision_groups: Option<CollisionGroups>,\3', content)

# Remove TODO comments about CollisionGroups
content = re.sub(r'\s+// TODO: Implement CollisionGroups filtering for the pin entity\.\n\s+// Currently, we only modify SolverGroups, which disables contact resolution but not broadphase intersection\.\n\s+// This can still cause instability or explosions in some Rapier configurations\.\n\s+// Add `CollisionGroups::new\(PIN_GROUP, Group::ALL\)` \(or similar filter\) to the pin spawn logic\n\s+// in `resolve_joint_targets` or here\.', '', content)

with open('src/input/commands.rs', 'w') as f:
    f.write(content)
