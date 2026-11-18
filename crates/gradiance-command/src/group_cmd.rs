//! Grouping commands (hierarchical — see [`SelectionGroup`]).
//!
//! Grouping pushes a fresh id onto every target's stack; ungrouping pops
//! each target's **outermost** id, so `ungroup(group(group(A,B,C), D))`
//! leaves `group(A,B,C)` intact.

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::group::{GroupIndex, GroupNames, SelectionGroup};

fn prior_groups(
    world: &mut World,
    targets: &[StableId],
) -> Result<Vec<(StableId, Option<SelectionGroup>)>, CommandError> {
    targets
        .iter()
        .map(|&id| {
            let entity = resolve(world, id)?;
            Ok((id, world.get::<SelectionGroup>(entity).cloned()))
        })
        .collect()
}

/// The live group index (members in `StableId` order).
fn group_index(world: &mut World) -> GroupIndex {
    let mut query = world.query::<(&StableId, &SelectionGroup)>();
    let mut rows: Vec<(StableId, Vec<u32>)> = query
        .iter(world)
        .map(|(id, g)| (*id, g.0.clone()))
        .collect();
    rows.sort_by_key(|(id, _)| id.0);
    GroupIndex::build(rows.iter().map(|(id, s)| (*id, s.as_slice())))
}

/// The next unused group id: past every stack in the world and every named
/// group ([`GroupIndex::fresh_id`]), so a new group never inherits a stale
/// name.
pub(crate) fn fresh_group_id(world: &mut World) -> u32 {
    let names = world
        .get_resource::<GroupNames>()
        .cloned()
        .unwrap_or_default();
    group_index(world).fresh_id(&names)
}

/// Wraps the targets in one fresh group (around any groups they already
/// form — hierarchical). Groups never overlap: a target's group that the
/// set only partly holds joins whole, and a set inside one group nests
/// inside it ([`GroupIndex::close_over`], brief A-2).
#[derive(Debug)]
pub struct GroupCommand {
    /// Bodies to group.
    pub targets: Vec<StableId>,
}

impl GroupCommand {
    /// Builds a group command.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self { targets }
    }
}

impl GameCommand for GroupCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.targets.len() < 2 {
            return Err(CommandError::NoEffect);
        }
        // Called for the `?`: every target must resolve before we touch any.
        prior_groups(world, &self.targets)?;
        let next = fresh_group_id(world);
        let index = group_index(world);
        let members = index.close_over(&self.targets);
        for &id in &members {
            if let Some(entity) = world.resource::<IdIndex>().entity(id) {
                let mut stack = world
                    .get::<SelectionGroup>(entity)
                    .cloned()
                    .unwrap_or(SelectionGroup(Vec::new()));
                let depth = index.insertion_depth(&stack.0, &members);
                stack.0.insert(depth, next);
                world.entity_mut(entity).insert(stack);
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::GROUP
    }
}

/// Pops each target's outermost group (inner groups survive).
#[derive(Debug)]
pub struct UngroupCommand {
    /// Bodies to ungroup.
    pub targets: Vec<StableId>,
}

impl UngroupCommand {
    /// Builds an ungroup command.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self { targets }
    }
}

impl GameCommand for UngroupCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let prior = prior_groups(world, &self.targets)?;
        if prior.iter().all(|(_, g)| g.is_none()) {
            return Err(CommandError::NoEffect);
        }
        for &id in &self.targets {
            let Some(entity) = world.resource::<IdIndex>().entity(id) else {
                continue;
            };
            let now_empty = match world.get_mut::<SelectionGroup>(entity) {
                Some(mut stack) => {
                    stack.0.pop();
                    stack.0.is_empty()
                }
                None => false,
            };
            if now_empty {
                world.entity_mut(entity).remove::<SelectionGroup>();
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::UNGROUP
    }
}

/// Names a group (an empty name removes it). The name is scene content
/// (`GroupNames`), so the stack's snapshot carries it and undo restores it.
#[derive(Debug)]
pub struct NameGroupCommand {
    /// The group's id.
    pub group: u32,
    /// The new name.
    pub name: String,
}

impl GameCommand for NameGroupCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if group_index(world).get(self.group).is_none() {
            return Err(CommandError::NoEffect);
        }
        let mut names = world
            .get_resource::<GroupNames>()
            .cloned()
            .unwrap_or_default();
        let old = names.get(self.group).unwrap_or_default().to_owned();
        if old == self.name {
            return Err(CommandError::NoEffect);
        }
        if self.name.is_empty() {
            names.0.remove(&self.group);
        } else {
            names.0.insert(self.group, self.name.clone());
        }
        world.insert_resource(names);
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::NAME_GROUP
    }
}

/// Splits one group into parts (see [`RegroupIntent`](crate::intent::RegroupIntent)):
/// each part of two or more members takes a fresh id in the old group's
/// place in every member's stack, so the nesting above and below is kept.
/// The old group's name goes with it.
#[derive(Debug)]
pub struct RegroupCommand {
    /// The group's id.
    pub group: u32,
    /// The parts.
    pub parts: Vec<Vec<StableId>>,
}

impl GameCommand for RegroupCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let Some(entry) = group_index(world).get(self.group).cloned() else {
            return Err(CommandError::NoEffect);
        };
        // One part holding every member is the group itself: nothing to do.
        let whole = self
            .parts
            .iter()
            .any(|p| p.len() == entry.members.len() && entry.members.iter().all(|m| p.contains(m)));
        if whole {
            return Err(CommandError::NoEffect);
        }
        for member in &entry.members {
            resolve(world, *member)?;
        }
        let mut next = fresh_group_id(world);
        let mut replacement: Vec<(StableId, Option<u32>)> = Vec::new();
        for part in &self.parts {
            let members: Vec<StableId> = part
                .iter()
                .copied()
                .filter(|m| entry.members.contains(m))
                .collect();
            let id = (members.len() >= 2).then(|| {
                next += 1;
                next - 1
            });
            replacement.extend(members.into_iter().map(|m| (m, id)));
        }
        for member in &entry.members {
            let new_id = replacement
                .iter()
                .find(|(m, _)| m == member)
                .and_then(|(_, id)| *id);
            let entity = resolve(world, *member)?;
            let Some(mut stack) = world.get::<SelectionGroup>(entity).cloned() else {
                continue;
            };
            match (stack.0.iter().position(|g| *g == self.group), new_id) {
                (Some(at), Some(id)) => stack.0[at] = id,
                (Some(at), None) => {
                    stack.0.remove(at);
                }
                (None, _) => {}
            }
            if stack.0.is_empty() {
                world.entity_mut(entity).remove::<SelectionGroup>();
            } else {
                world.entity_mut(entity).insert(stack);
            }
        }
        if let Some(mut names) = world.get_resource_mut::<GroupNames>()
            && names.0.contains_key(&self.group)
        {
            names.0.remove(&self.group);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::REGROUP
    }
}
