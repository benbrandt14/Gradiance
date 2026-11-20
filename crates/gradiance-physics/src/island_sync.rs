//! Paused-edit island reconciliation (the islands-on companion).
//!
//! avian links joints into [simulation islands] through
//! [`JointGraphChange`] messages, processed by a system that lives in the
//! `PhysicsSchedule` — which never runs while the simulation is paused (a
//! paused clock has a zero delta). Bevy messages expire after two frames
//! when no system reads them, so **every joint graph change made while the
//! editor is paused is lost**: deleting a jointed body leaves a stale
//! island node, restoring it reuses the freed graph slot, and the first
//! playing frame then tries to link a joint into an island node that still
//! exists — avian's `add_joint` panics on the double link. Editing while
//! paused is the sandbox's main mode, so this is not acceptable.
//!
//! [`reconcile_joint_islands`] closes the gap from the outside, through
//! avian's public API only: while paused it makes the islands' joint nodes
//! *exactly* match the live joint graph — adding the links avian missed,
//! unlinking the orphans it kept — and consumes the pending
//! [`JointGraphChange`] messages so a later resume cannot double-apply what
//! the reconciliation already did. While playing, avian processes its own
//! messages and this system stays out of the way.
//!
//! [simulation islands]: avian2d::dynamics::solver::islands

use avian2d::dynamics::joints::joint_graph::{JointGraph, JointGraphChange, JointId};
use avian2d::dynamics::solver::islands::{BodyIslandNode, PhysicsIslands, WakeIslands};
use bevy::ecs::entity_disabling::Disabled;
use bevy::prelude::*;
use bevy::state::condition::in_state;
use gradiance_core::states::GameState;

/// Installs the paused-edit island reconciliation.
pub fn install(app: &mut App) {
    app.add_systems(
        PreUpdate,
        reconcile_joint_islands.run_if(in_state(GameState::Paused)),
    );
}

/// Makes avian's island joint links match the joint graph while paused:
/// adds the links for graph changes avian never heard, unlinks the orphans
/// whose joints are gone, wakes an island that gained a joint, and drains
/// the pending [`JointGraphChange`] messages so resuming cannot double-link.
///
/// Cost: `O(J)` for `J` slots ever used by the joint graph — a probe per
/// slot while paused, none while playing.
pub fn reconcile_joint_islands(
    graph: Res<JointGraph>,
    mut islands: ResMut<PhysicsIslands>,
    mut body_islands: Query<&mut BodyIslandNode, Or<(With<Disabled>, Without<Disabled>)>>,
    mut commands: Commands,
    mut high_water: Local<u32>,
    mut messages: ResMut<Messages<JointGraphChange>>,
) {
    // Link the joints avian missed (authored or restored while paused).
    let mut max_slot = *high_water;
    for edge in graph.graph().all_edge_weights() {
        max_slot = max_slot.max(edge.id.0 + 1);
        if islands.joint_node(edge.id).is_some() {
            continue;
        }
        // A despawn while paused leaves a dead edge in the graph (nothing
        // observes it); linking one whose two ends are both gone would try
        // to merge islands that do not exist. Skip those — avian's own
        // `merge_islands` would panic on them.
        if body_islands.get(edge.body1).is_err() && body_islands.get(edge.body2).is_err() {
            continue;
        }
        if let Some(island) = islands.add_joint(edge.id, &mut body_islands, &graph)
            && island.is_sleeping()
        {
            commands.queue(WakeIslands(vec![island.id()]));
        }
    }
    // Unlink the orphans: a joint deleted while paused whose removal message
    // expired unread leaves its island node behind.
    for slot in 0..max_slot {
        let id = JointId(slot);
        if islands.joint_node(id).is_some() && graph.get_by_id(id).is_none() {
            islands.remove_joint(id, &mut body_islands);
        }
    }
    *high_water = max_slot;
    // The messages avian's paused schedule never reads: consumed here so
    // resuming cannot double-apply what the reconciliation already did.
    let _ = messages.drain();
}
