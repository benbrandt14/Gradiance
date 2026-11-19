//! Insert a scene fragment into the current scene (brief R-3): the command
//! behind `scene.insert` (a `.ron` file as a part) and `edit.paste` (the
//! same RON text from the clipboard).
//!
//! # One remap, three users
//!
//! A fragment arrives with the ids it was saved with. [`fresh_copy`] gives
//! every body, joint and node a new [`StableId`] (minted by the caller: an
//! operation takes them from its context) and moves the set by an offset,
//! through the same helpers duplicate and array use
//! ([`remap_joint_def`](crate::spawn::remap_joint_def),
//! [`remap_node`](crate::spawn::remap_node)): a joint keeps both ends
//! inside the set, a world pin's anchor moves with it. [`InsertCommand`]
//! then gives the groups fresh ids against the live scene
//! ([`remap_clone_groups`](crate::spawn::remap_clone_groups), as a
//! duplicate does) and carries their names over. Nothing here clones a
//! live entity: the records are the fragment's.
//!
//! [`fragment`] is the other direction: the part of a scene record a copy
//! puts on the clipboard (the objects, the joints inside the set, the
//! nodes on its bodies, the names of its groups).

use crate::{CommandError, GameCommand};
use bevy::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::group::GroupNames;
use gradiance_scene::{EnvironmentRecord, JointRecord, SceneRecord};

/// The point a fragment is placed by: the centre of the box around its body
/// positions (its node positions when it has no body). `None` when empty.
pub fn fragment_anchor(scene: &SceneRecord) -> Option<Vec2> {
    let points: Vec<Vec2> = if scene.bodies.is_empty() {
        scene.nodes.iter().map(|n| n.pose.pos).collect()
    } else {
        scene.bodies.iter().map(|b| b.pose.pos).collect()
    };
    let first = *points.first()?;
    let (lo, hi) = points
        .iter()
        .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    Some((lo + hi) * 0.5)
}

/// `scene` with a new id for every body, joint and node (from `mint`, in
/// body, joint, node order) and every pose moved by `offset`. Joints whose
/// ends are not all in the fragment and nodes attached to a body outside it
/// are left out. The environment keeps only the group names (the one
/// setting an insert carries); group ids are made fresh by
/// [`InsertCommand`] against the live scene.
pub fn fresh_copy(
    scene: &SceneRecord,
    offset: Vec2,
    mut mint: impl FnMut() -> StableId,
) -> SceneRecord {
    let mut id_map = Vec::with_capacity(scene.bodies.len());
    let bodies = scene
        .bodies
        .iter()
        .map(|body| {
            let mut copy = body.clone();
            copy.id = mint();
            copy.pose.pos += offset;
            id_map.push((body.id, copy.id));
            copy
        })
        .collect();
    let joints = scene
        .joints
        .iter()
        .filter_map(|joint| {
            let def = crate::spawn::remap_joint_def(joint.def.clone(), &id_map, |p| p + offset, 0.0)?;
            Some(JointRecord { id: mint(), def })
        })
        .collect();
    let nodes = scene
        .nodes
        .iter()
        .filter_map(|node| crate::spawn::remap_node(node.clone(), &id_map, offset, mint()))
        .collect();
    SceneRecord {
        version: scene.version,
        app_version: scene.app_version.clone(),
        bodies,
        joints,
        nodes,
        environment: EnvironmentRecord {
            group_names: scene.environment.group_names.clone(),
            ..EnvironmentRecord::default()
        },
    }
}

/// The part of `scene` that `objects` names, as a copy writes it: those
/// bodies, the joints whose ends are all among them (a world pin goes with
/// its body), every node attached to one of them, every named node that is
/// free, and the names of the copied bodies' groups. The other settings
/// stay at their defaults: a paste never applies them. `None` when
/// `objects` names no body or node of `scene`.
pub fn fragment(scene: &SceneRecord, objects: &[StableId]) -> Option<SceneRecord> {
    let bodies: Vec<_> = scene
        .bodies
        .iter()
        .filter(|b| objects.contains(&b.id))
        .cloned()
        .collect();
    let body_ids: Vec<StableId> = bodies.iter().map(|b| b.id).collect();
    let joints = scene
        .joints
        .iter()
        .filter(|j| j.def.referenced_bodies().all(|id| body_ids.contains(&id)))
        .cloned()
        .collect();
    let nodes: Vec<_> = scene
        .nodes
        .iter()
        .filter(|n| match n.attachment.target {
            Some(target) => body_ids.contains(&target),
            None => objects.contains(&n.id),
        })
        .cloned()
        .collect();
    if bodies.is_empty() && nodes.is_empty() {
        return None;
    }
    let groups: Vec<u32> = bodies.iter().flat_map(|b| b.groups.clone()).collect();
    let names = scene
        .environment
        .group_names
        .0
        .iter()
        .filter(|(id, _)| groups.contains(id))
        .map(|(id, name)| (*id, name.clone()))
        .collect();
    Some(SceneRecord {
        version: scene.version,
        app_version: scene.app_version.clone(),
        bodies,
        joints,
        nodes,
        environment: EnvironmentRecord {
            group_names: GroupNames(names),
            ..EnvironmentRecord::default()
        },
    })
}

/// Adds a fragment's bodies, joints and nodes to the scene (one undo step).
///
/// The records carry their final ids and poses ([`fresh_copy`]); an id that
/// already names a live entity is refused, so an insert can never replace
/// or alias an object. Groups get fresh ids, one per distinct source group,
/// so the nesting is kept and nothing joins an existing group; their names
/// move to the new ids.
#[derive(Debug)]
pub struct InsertCommand {
    /// The fragment, with fresh ids.
    pub scene: SceneRecord,
}

impl InsertCommand {
    /// Builds an insert command.
    pub fn new(scene: SceneRecord) -> Self {
        Self { scene }
    }
}

impl GameCommand for InsertCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let scene = &self.scene;
        if scene.bodies.is_empty() && scene.nodes.is_empty() {
            return Err(CommandError::NoEffect);
        }
        for body in &scene.bodies {
            body.shape.validate()?;
        }
        let ids = scene
            .bodies
            .iter()
            .map(|b| b.id)
            .chain(scene.joints.iter().map(|j| j.id))
            .chain(scene.nodes.iter().map(|n| n.id));
        let index = world.resource::<IdIndex>();
        let mut seen = Vec::new();
        for id in ids {
            if index.entity(id).is_some() || seen.contains(&id) {
                return Err(CommandError::IdTaken(id));
            }
            seen.push(id);
        }
        // A joint or node whose body is not in the fragment would dangle.
        let body_ids: Vec<StableId> = scene.bodies.iter().map(|b| b.id).collect();
        let ends = scene
            .joints
            .iter()
            .flat_map(|j| j.def.referenced_bodies())
            .chain(scene.nodes.iter().filter_map(|n| n.attachment.target));
        for end in ends {
            if !body_ids.contains(&end) {
                return Err(CommandError::MissingEntity(end));
            }
        }

        let mut bodies = scene.bodies.clone();
        let mut next_group = crate::group_cmd::fresh_group_id(world);
        let moved = crate::spawn::remap_clone_groups(&mut bodies, &mut next_group);
        let carried: Vec<(u32, String)> = moved
            .iter()
            .filter_map(|(old, new)| {
                let name = scene.environment.group_names.get(*old)?;
                Some((*new, name.to_owned()))
            })
            .collect();
        if !carried.is_empty() {
            let mut names = world
                .get_resource::<GroupNames>()
                .cloned()
                .unwrap_or_default();
            names.0.extend(carried);
            world.insert_resource(names);
        }
        for record in &bodies {
            record.spawn(world);
        }
        for record in &scene.joints {
            record.spawn(world);
        }
        for record in &scene.nodes {
            record.spawn(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::INSERT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_core::units::PosRot;
    use gradiance_domain::joint::JointDef;
    use gradiance_scene::BodyRecord;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// The test's `n`-th id: one random id per number, per thread.
    fn id(n: u128) -> StableId {
        thread_local! {
            static IDS: RefCell<HashMap<u128, StableId>> = RefCell::new(HashMap::new());
        }
        IDS.with(|ids| *ids.borrow_mut().entry(n).or_insert_with(StableId::new))
    }

    fn body(n: u128, x: f32, groups: Vec<u32>) -> BodyRecord {
        let mut record = crate::ops::spawned_body_record(
            id(n),
            gradiance_domain::shape::ShapeDef::Circle { radius: 0.5 },
            Vec2::new(x, 1.0),
        );
        record.groups = groups;
        record
    }

    fn scene(bodies: Vec<BodyRecord>, joints: Vec<JointRecord>) -> SceneRecord {
        SceneRecord {
            version: gradiance_scene::FORMAT_VERSION,
            app_version: String::new(),
            bodies,
            joints,
            nodes: Vec::new(),
            environment: EnvironmentRecord::default(),
        }
    }

    /// A hinge from body `a` to body `b`, or to the world at (4, 1).
    fn pin(n: u128, a: u128, b: Option<u128>) -> JointRecord {
        let def = JointDef {
            kind: gradiance_domain::joint::JointKind::Hinge {
                limits: None,
                motor: None,
            },
            common: gradiance_domain::joint::JointCommon::default(),
            body_a: id(a),
            body_b: b.map(id),
            anchor_a: gradiance_units::Displacement(Vec2::ZERO),
            anchor_b: gradiance_units::Displacement(Vec2::new(4.0, 1.0)),
            rest_rot_a: gradiance_units::Angle(0.0),
            rest_rot_b: gradiance_units::Angle(0.0),
        };
        JointRecord { id: id(n), def }
    }

    #[test]
    fn a_fresh_copy_renames_everything_and_keeps_the_inside_joints() {
        let source = scene(
            vec![body(1, 0.0, vec![7]), body(2, 2.0, vec![7, 9])],
            vec![pin(10, 1, Some(2)), pin(11, 1, None), pin(12, 1, Some(99))],
        );
        let mut next = 100;
        let copy = fresh_copy(&source, Vec2::new(1.0, 0.0), || {
            next += 1;
            id(next)
        });
        assert_eq!(
            copy.bodies.iter().map(|b| b.id).collect::<Vec<_>>(),
            [id(101), id(102)]
        );
        assert_eq!(copy.bodies[1].pose.pos, Vec2::new(3.0, 1.0));
        assert_eq!(copy.bodies[1].groups, [7, 9], "group ids move in the command");
        // The pair joint and the world pin; the joint to body 99 is outside.
        assert_eq!(copy.joints.len(), 2);
        assert_eq!(copy.joints[0].def.body_a, id(101));
        assert_eq!(copy.joints[0].def.body_b, Some(id(102)));
        assert_eq!(copy.joints[1].def.anchor_b.value(), Vec2::new(5.0, 1.0));
        assert_eq!(
            copy.joints.iter().map(|j| j.id).collect::<Vec<_>>(),
            [id(103), id(104)],
            "joints take the ids after the bodies'"
        );
    }

    #[test]
    fn a_fragment_takes_the_named_bodies_and_their_inside_joints() {
        let mut source = scene(
            vec![body(1, 0.0, vec![7]), body(2, 4.0, vec![7]), body(3, 8.0, vec![])],
            vec![pin(10, 1, Some(2)), pin(11, 1, Some(3)), pin(12, 2, None)],
        );
        source.environment.group_names.0.insert(7, "pair".to_owned());
        source.environment.group_names.0.insert(8, "other".to_owned());
        let part = fragment(&source, &[id(1), id(2)]);
        let part = part.as_ref();
        assert_eq!(part.map(|p| p.bodies.len()), Some(2));
        assert_eq!(
            part.map(|p| p.joints.iter().map(|j| j.id).collect::<Vec<_>>()),
            Some(vec![id(10), id(12)])
        );
        assert_eq!(
            part.map(|p| p.environment.group_names.0.len()),
            Some(1),
            "only the copied groups' names"
        );
        assert_eq!(part.and_then(fragment_anchor), Some(Vec2::new(2.0, 1.0)));
        assert!(fragment(&source, &[id(42)]).is_none());
    }

    #[test]
    fn the_anchor_is_the_centre_of_the_body_positions() {
        let source = scene(vec![body(1, -1.0, vec![]), body(2, 5.0, vec![])], vec![]);
        assert_eq!(fragment_anchor(&source), Some(Vec2::new(2.0, 1.0)));
        assert_eq!(fragment_anchor(&scene(vec![], vec![])), None);
    }

    #[test]
    fn a_pose_is_moved_not_rotated() {
        let mut source = scene(vec![body(1, 0.0, vec![])], vec![]);
        source.bodies[0].pose = PosRot {
            pos: Vec2::ZERO,
            rot: 0.5,
        };
        let copy = fresh_copy(&source, Vec2::ONE, StableId::new);
        assert_eq!(copy.bodies[0].pose.rot, 0.5);
        assert_eq!(copy.bodies[0].pose.pos, Vec2::ONE);
    }
}
