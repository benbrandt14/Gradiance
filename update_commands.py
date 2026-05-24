import re

with open('src/input/commands.rs', 'r') as f:
    content = f.read()

# 1. Update struct definitions to include original_collision_groups
structs_to_update = ['SpawnJointCommand', 'SpawnPrismaticJointCommand', 'SpawnFixedJointCommand']
for struct in structs_to_update:
    pattern = re.compile(r'(pub struct ' + struct + r' \{.*?)(\s+pub original_solver_groups: Option<SolverGroups>,)(\s+\})', re.DOTALL)
    content = pattern.sub(r'\1\2\n    /// Previous collision groups of the pinned body.\n    pub original_collision_groups: Option<CollisionGroups>,\3', content)

# 2. Update resolve_joint_targets to add CollisionGroups to the pin entity
pattern = re.compile(r'(\s+Collider::ball\(CONNECTOR_COLLIDER_RADIUS\),)(\s+pin_solver_groups,)', re.DOTALL)
content = pattern.sub(r'\1\2\n                CollisionGroups::new(PIN_GROUP, Group::ALL),', content)

# 3. Update the apply and undo methods for each command
# It's safer to use python strings replacements.
commands = ['SpawnJointCommand', 'SpawnPrismaticJointCommand', 'SpawnFixedJointCommand']
for cmd in commands:
    # Add to apply
    apply_pattern = r'''
        if self.entity_b.is_none() {
            let old_groups = world.get::<SolverGroups>(self.entity_a.0).copied();
            self.original_solver_groups = old_groups;

            let mut new_groups = old_groups.unwrap_or(SolverGroups {
                memberships: Group::ALL,
                filters: Group::ALL,
            });
            new_groups.filters &= !PIN_GROUP;

            world.entity_mut(self.entity_a.0).insert(new_groups);
        }'''

    apply_replacement = r'''
        if self.entity_b.is_none() {
            let old_groups = world.get::<SolverGroups>(self.entity_a.0).copied();
            self.original_solver_groups = old_groups;

            let mut new_groups = old_groups.unwrap_or(SolverGroups {
                memberships: Group::ALL,
                filters: Group::ALL,
            });
            new_groups.filters &= !PIN_GROUP;

            world.entity_mut(self.entity_a.0).insert(new_groups);

            let old_cg = world.get::<CollisionGroups>(self.entity_a.0).copied();
            self.original_collision_groups = old_cg;

            let mut new_cg = old_cg.unwrap_or(CollisionGroups {
                memberships: Group::ALL,
                filters: Group::ALL,
            });
            new_cg.filters &= !PIN_GROUP;
            world.entity_mut(self.entity_a.0).insert(new_cg);
        }'''

    content = content.replace(apply_pattern, apply_replacement)

    undo_pattern = r'''
        if self.entity_b.is_none() {
            if let Some(groups) = self.original_solver_groups {
                if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                    e.insert(groups);
                }
            } else if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                e.remove::<SolverGroups>();
            }
        }'''

    undo_replacement = r'''
        if self.entity_b.is_none() {
            if let Some(groups) = self.original_solver_groups {
                if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                    e.insert(groups);
                }
            } else if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                e.remove::<SolverGroups>();
            }
            if let Some(cg) = self.original_collision_groups {
                if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                    e.insert(cg);
                }
            } else if let Ok(mut e) = world.get_entity_mut(self.entity_a.0) {
                e.remove::<CollisionGroups>();
            }
        }'''

    content = content.replace(undo_pattern, undo_replacement)

with open('src/input/commands.rs', 'w') as f:
    f.write(content)
