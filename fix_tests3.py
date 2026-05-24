import re

with open('tests/prop_extended.rs', 'r') as f:
    content = f.read()

content = content.replace(
    'original_solver_groups: None,',
    'original_solver_groups: None,\n                original_collision_groups: None,'
)

with open('tests/prop_extended.rs', 'w') as f:
    f.write(content)
