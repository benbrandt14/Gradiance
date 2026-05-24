import re

with open('tests/test_joints.rs', 'r') as f:
    content = f.read()

content = content.replace(
    'original_solver_groups: None,',
    'original_solver_groups: None,\n        original_collision_groups: None,'
)

with open('tests/test_joints.rs', 'w') as f:
    f.write(content)
