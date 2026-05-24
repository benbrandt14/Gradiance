import re

with open('src/input/commands.rs', 'r') as f:
    content = f.read()

# Add original_collision_groups to SpawnFixedJointCommand (handle rot_a/rot_b fields correctly)
pattern = re.compile(r'(pub struct SpawnFixedJointCommand \{.*?)(    pub rot_b: f32,\n)(\})', re.DOTALL)
content = pattern.sub(r'\1\2    /// Previous collision groups of the pinned body.\n    pub original_collision_groups: Option<CollisionGroups>,\n\3', content)

with open('src/input/commands.rs', 'w') as f:
    f.write(content)
