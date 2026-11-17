//! Selection grouping — **hierarchical**.
//!
//! A body carries a *stack* of group ids, innermost first:
//!
//! ```text
//! group(A, B, C)            A[1]      B[1]      C[1]
//! group(that, D)            A[1,2]    B[1,2]    C[1,2]    D[2]
//! ungroup(selection)        A[1]      B[1]      C[1]      D
//!                           └── the inner group survives ──┘
//! ```
//!
//! Selecting a grouped body selects everything sharing its **outermost**
//! id; ungrouping pops only the outermost id, so nested groups peel like
//! an onion instead of dissolving.
//!
//! ```
//! use gradiance_domain::group::SelectionGroup;
//!
//! let nested = SelectionGroup(vec![1, 2]); // in group 1, inside group 2
//! assert_eq!(nested.outermost(), Some(2));
//! ```
//!
//! # Groups as editing layers (brief 4b-1)
//!
//! A group is the set of bodies whose stack holds its id, at any depth, so
//! an inner group stays addressable after it is wrapped. Its id is stable
//! across save, load and undo (the stacks are authored and saved), and its
//! optional **name** lives in [`GroupNames`], a scene-content resource keyed
//! by that id. [`GroupIndex`] is the read view the operations and the panel
//! share: groups in id order, their members, their nesting.

use bevy::prelude::{Component, Reflect, Resource};
use gradiance_core::ids::StableId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The group stack a body belongs to (innermost first, outermost last).
///
/// Never empty while attached — commands remove the component instead of
/// leaving an empty stack.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SelectionGroup(pub Vec<u32>);

impl SelectionGroup {
    /// The outermost group id — the one selection expands by and
    /// ungroup pops.
    pub fn outermost(&self) -> Option<u32> {
        self.0.last().copied()
    }
}

/// The authored names of groups, keyed by group id.
///
/// Scene content (`gradiance_scene::records`' `scene_content!` list): saved
/// with the scene and restored by undo. Written only by the group-naming
/// command; an unnamed group has no entry. An entry whose group has no
/// members any more is inert, and fresh group ids skip it
/// ([`GroupIndex::fresh_id`]) so a new group never inherits an old name.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct GroupNames(pub BTreeMap<u32, String>);

impl GroupNames {
    /// The name of group `id`, if it has one.
    pub fn get(&self, id: u32) -> Option<&str> {
        self.0.get(&id).map(String::as_str)
    }
}

/// One group in a [`GroupIndex`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupEntry {
    /// The group's id (the value in the members' stacks).
    pub id: u32,
    /// Its members, in the order the index was built from.
    pub members: Vec<StableId>,
    /// The group directly around it, if it is nested.
    pub parent: Option<u32>,
}

/// A read view of every group in a scene, computed from the members'
/// [`SelectionGroup`] stacks. Groups are in id order, so "group `i`" means
/// the same group to every reader of the same scene.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupIndex {
    /// The groups, in id order.
    pub groups: Vec<GroupEntry>,
}

impl GroupIndex {
    /// Builds the index from `(body, stack)` pairs; members keep the order
    /// the pairs come in. A group's parent is the id after it in any
    /// member's stack (stacks nest, so every member agrees).
    pub fn build<'a>(bodies: impl IntoIterator<Item = (StableId, &'a [u32])>) -> Self {
        let mut map: BTreeMap<u32, GroupEntry> = BTreeMap::new();
        for (body, stack) in bodies {
            for (depth, &id) in stack.iter().enumerate() {
                let entry = map.entry(id).or_insert_with(|| GroupEntry {
                    id,
                    members: Vec::new(),
                    parent: None,
                });
                entry.members.push(body);
                if entry.parent.is_none() {
                    entry.parent = stack.get(depth + 1).copied();
                }
            }
        }
        Self {
            groups: map.into_values().collect(),
        }
    }

    /// Number of groups.
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Whether the scene has no group.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// The group with id `id`.
    pub fn get(&self, id: u32) -> Option<&GroupEntry> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// The position of group `id` in id order.
    pub fn position(&self, id: u32) -> Option<usize> {
        self.groups.iter().position(|g| g.id == id)
    }

    /// The id a new group gets: past every live group and every named one,
    /// so it never inherits a stale name.
    pub fn fresh_id(&self, names: &GroupNames) -> u32 {
        let live = self.groups.last().map_or(0, |g| g.id);
        let named = names.0.keys().next_back().copied().unwrap_or(0);
        live.max(named) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn id(n: u128) -> StableId {
        StableId(Uuid::from_u128(n))
    }

    #[test]
    fn the_index_lists_nested_groups_with_their_members_and_parents() {
        let a = [1, 2];
        let b = [1, 2];
        let c = [2];
        let d: [u32; 0] = [];
        let index = GroupIndex::build([
            (id(1), &a[..]),
            (id(2), &b[..]),
            (id(3), &c[..]),
            (id(4), &d[..]),
        ]);
        assert_eq!(index.len(), 2);
        assert_eq!(index.groups[0].id, 1);
        assert_eq!(index.groups[0].members, vec![id(1), id(2)]);
        assert_eq!(index.groups[0].parent, Some(2));
        assert_eq!(index.groups[1].members, vec![id(1), id(2), id(3)]);
        assert_eq!(index.groups[1].parent, None);
        assert_eq!(index.position(2), Some(1));
    }

    #[test]
    fn a_fresh_id_skips_live_and_named_groups() {
        let stack = [3];
        let index = GroupIndex::build([(id(1), &stack[..])]);
        assert_eq!(index.fresh_id(&GroupNames::default()), 4);
        let names = GroupNames(BTreeMap::from([(9, "old".to_owned())]));
        assert_eq!(index.fresh_id(&names), 10);
        assert_eq!(GroupIndex::default().fresh_id(&GroupNames::default()), 1);
    }
}
