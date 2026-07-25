//! Grouping commands (hierarchical — see [`SelectionGroup`]).
//!
//! Grouping pushes a fresh id onto every target's stack; ungrouping pops
//! each target's **outermost** id, so `ungroup(group(group(A,B,C), D))`
//! leaves `group(A,B,C)` intact.

use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::group::SelectionGroup;

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

/// The next unused group id across every stack in the world.
fn fresh_group_id(world: &mut World) -> u32 {
    let mut query = world.query::<&SelectionGroup>();
    query
        .iter(world)
        .flat_map(|g| g.0.iter().copied())
        .max()
        .map_or(1, |m| m + 1)
}

/// Wraps the targets in one fresh group (pushed *around* any groups they
/// already form — hierarchical).
#[derive(Debug)]
pub struct GroupCommand {
    /// Bodies to group.
    pub targets: Vec<StableId>,
    prior: Vec<(StableId, Option<SelectionGroup>)>,
}

impl GroupCommand {
    /// Builds a group command.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self {
            targets,
            prior: Vec::new(),
        }
    }
}

impl GameCommand for GroupCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.targets.len() < 2 {
            return Err(CommandError::NoEffect);
        }
        let prior = prior_groups(world, &self.targets)?;
        let next = fresh_group_id(world);
        for &id in &self.targets {
            if let Some(entity) = world.resource::<IdIndex>().entity(id) {
                let mut stack = world
                    .get::<SelectionGroup>(entity)
                    .cloned()
                    .unwrap_or(SelectionGroup(Vec::new()));
                stack.0.push(next);
                world.entity_mut(entity).insert(stack);
            }
        }
        if self.prior.is_empty() {
            self.prior = prior;
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
    prior: Vec<(StableId, Option<SelectionGroup>)>,
}

impl UngroupCommand {
    /// Builds an ungroup command.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self {
            targets,
            prior: Vec::new(),
        }
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
        if self.prior.is_empty() {
            self.prior = prior;
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::UNGROUP
    }
}
