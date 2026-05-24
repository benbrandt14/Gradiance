import re

with open('src/physics/controllers.rs', 'r') as f:
    content = f.read()

content = content.replace(
    'let get_angle = |e_a, e_b, local_anchor1: Vec2, local_anchor2: Vec2, transforms: &Query<&GlobalTransform>| {',
    'let get_angle = |e_a, e_b, _local_anchor1: Vec2, _local_anchor2: Vec2, transforms: &Query<&GlobalTransform>| {'
)

with open('src/physics/controllers.rs', 'w') as f:
    f.write(content)
