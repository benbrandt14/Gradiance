//! [`SceneSnapshot`]: the read-only mirror of the committed scene that the
//! operation context carries (`docs/unified-surface-plan.md` § 13.2).
//!
//! It lives here, not in `gradiance-op`, because it names `ShapeDef` and
//! `BodyPhysics`. One builder ([`snapshot_scene`]) serves every doorway, so
//! "one snapshot per run" is one value, not one per surface.

use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::Body;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::group::{GroupIndex, GroupNames, SelectionGroup};
use gradiance_domain::joint::JointDef;
use gradiance_domain::props::BodyPhysics;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::sdf;
use gradiance_op::{ResolveTarget, Target};
use std::collections::HashMap;

/// One body in a [`SceneSnapshot`] snapshot (world pose + authored shape).
#[derive(Debug, Clone)]
pub struct BodySnapshot {
    /// Durable identity — what an edit verb names as its target. Raw `Entity`
    /// is never cross-referenced (invariant 3), and the snapshot outlives the
    /// frame it was taken in, so the id is the only safe handle here.
    pub id: StableId,
    /// Authored physics — the *old* half of a property edit.
    ///
    /// `PropertyChange` carries both ends so it can reverse itself, and the
    /// script layer has no live World access at verb time (the VM runs inside
    /// one exclusive system, reading this snapshot). So the values a
    /// `set-*` verb might change have to be *in* the snapshot; they double as
    /// the `body-friction`-style reads, which is the read-total half.
    pub physics: BodyPhysics,
    /// World position (metres).
    pub pos: Vec2,
    /// World rotation (radians).
    pub rot: f32,
    /// Authored shape, for point containment.
    pub shape: ShapeDef,
    /// How many bodies it touches (from avian's contact graph).
    pub touching: usize,
    /// Authored appearance: the old half of a colour edit, and what the
    /// colour filters and decompose read.
    pub appearance: Appearance,
    /// Authored depth band.
    pub depth: DepthBand,
    /// The group stack, innermost first (empty when ungrouped).
    pub groups: Vec<u32>,
    /// The fluid region the body holds, if any: the old half of a
    /// `set-fluid` edit (brief N-7).
    pub fluid: Option<gradiance_domain::fluid::Fluid>,
    /// The initial motion, if any: the old half of a `set-velocity` /
    /// `set-spin` edit (brief S-1).
    pub motion: Option<gradiance_domain::motion::InitialMotion>,
    /// The refractive index, if any: the old half of a `set-index` edit
    /// (brief S-3).
    pub optics: Option<gradiance_domain::optics::Optics>,
    /// Removal during a run (brief S-7).
    pub lifetime: Option<gradiance_domain::lifetime::Lifetime>,
}

impl BodySnapshot {
    /// The body's pose.
    pub fn pose(&self) -> PosRot {
        PosRot {
            pos: self.pos,
            rot: self.rot,
        }
    }

    /// Whether the world point `p` lies inside this body's shape (exact SDF
    /// containment, transforming the point into the body's local frame).
    pub fn contains(&self, p: Vec2) -> bool {
        let local = Vec2::from_angle(-self.rot).rotate(p - self.pos);
        sdf::eval(&self.shape, local) <= 0.0
    }
}

/// A read-only snapshot of the committed scene, built once per operation context
/// ([`OpContext`](gradiance_op::OpContext)): once per script run pass, once
/// per remote or menu invocation. Operations and the script query verbs read
/// it (they cannot hold `&World`), giving every doorway the read-total half
/// of the governance model with no mutation path. Bodies are ordered by id so an index is stable across a
/// run.
#[derive(Debug, Default, Clone)]
pub struct SceneSnapshot {
    /// The committed scene's bodies, in `StableId` order.
    pub bodies: Vec<BodySnapshot>,
    /// The committed scene's joints, in `StableId` order. Ids rather than a
    /// count so `delete-joint` can name one, using the same index vocabulary
    /// the body verbs use.
    pub joints: Vec<StableId>,
    /// The committed scene's behaviour nodes, in `StableId` order: what a
    /// delete may name besides a body.
    pub nodes: Vec<StableId>,
    /// The bodies each joint connects (`None`: pinned to the world), in
    /// joint id order: what connectivity decompose walks.
    pub joint_links: Vec<(StableId, Option<StableId>)>,
    /// Every joint's definition with its id, in joint id order: the old
    /// value a joint edit (`joint.brake`) needs, and the joints a body's
    /// `body.loosen` drops (brief S-6).
    pub joint_defs: Vec<(StableId, JointDef)>,
    /// The group names (scene content).
    pub group_names: GroupNames,
}

impl SceneSnapshot {
    /// Number of bodies.
    pub fn count(&self) -> f64 {
        self.bodies.len() as f64
    }

    /// The body with this id.
    pub fn get(&self, id: StableId) -> Option<&BodySnapshot> {
        self.bodies.iter().find(|b| b.id == id)
    }

    /// Identity of the `i`-th body — the target an edit verb names.
    pub fn id(&self, i: usize) -> Option<StableId> {
        self.bodies.get(i).map(|b| b.id)
    }

    /// Number of authored joints.
    pub fn joint_count(&self) -> f64 {
        self.joints.len() as f64
    }

    /// Identity of the `i`-th joint.
    pub fn joint_id(&self, i: usize) -> Option<StableId> {
        self.joints.get(i).copied()
    }

    /// How many bodies contain the world point `p`.
    pub fn count_at(&self, p: Vec2) -> f64 {
        self.bodies.iter().filter(|b| b.contains(p)).count() as f64
    }

    /// Index of the body whose centre is nearest `p`, or -1 if the scene is
    /// empty.
    pub fn nearest_at(&self, p: Vec2) -> f64 {
        self.bodies
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.pos
                    .distance_squared(p)
                    .total_cmp(&b.pos.distance_squared(p))
            })
            .map_or(-1.0, |(i, _)| i as f64)
    }

    /// Distance from `p` to the nearest body centre, or -1 if the scene is empty.
    pub fn nearest_dist(&self, p: Vec2) -> f64 {
        self.bodies
            .iter()
            .map(|b| b.pos.distance(p))
            .min_by(f32::total_cmp)
            .map_or(-1.0, f64::from)
    }

    /// Index of the first body (id order) whose shape contains `p`, or -1.
    pub fn index_at(&self, p: Vec2) -> f64 {
        self.bodies
            .iter()
            .position(|b| b.contains(p))
            .map_or(-1.0, |i| i as f64)
    }
}

/// Snapshots the committed scene (bodies ordered by id) for the read builtins.
pub fn snapshot_scene(world: &mut World) -> SceneSnapshot {
    // Touching counts per body entity (one pass over the contact graph).
    let mut touch: HashMap<Entity, usize> = HashMap::new();
    if let Some(contacts) = world.get_resource::<avian2d::prelude::ContactGraph>() {
        for pair in contacts
            .iter_active_touching()
            .chain(contacts.iter_sleeping_touching())
        {
            for entity in [pair.collider1, pair.collider2] {
                *touch.entry(entity).or_default() += 1;
            }
        }
    }
    // Authored physics is spread across avian components (that is the
    // de-adapter collapse — `docs/physics-deadapter-decision.md`), so the
    // snapshot re-gathers them into the `BodyPhysics` bundle the records use.
    let mut rows: Vec<BodySnapshot> = world
        .query_filtered::<(EntityRef, &StableId, &Transform, &ShapeDef), With<Body>>()
        .iter(world)
        .map(|(entity, id, transform, shape)| {
            let pose = PosRot::from_transform(transform);
            // Every authored part reads as its default when absent, so a body
            // missing one is still *in* the snapshot: requiring them would
            // silently drop it from `body-count` and every index after it,
            // which is a far worse failure than reading a default.
            BodySnapshot {
                id: *id,
                physics: BodyPhysics::capture(&entity),
                pos: pose.pos,
                rot: pose.rot,
                shape: shape.clone(),
                touching: touch.get(&entity.id()).copied().unwrap_or(0),
                appearance: entity.get::<Appearance>().copied().unwrap_or_default(),
                depth: entity.get::<DepthBand>().copied().unwrap_or_default(),
                groups: entity
                    .get::<SelectionGroup>()
                    .map_or_else(Vec::new, |g| g.0.clone()),
                fluid: entity.get::<gradiance_domain::fluid::Fluid>().copied(),
                motion: entity
                    .get::<gradiance_domain::motion::InitialMotion>()
                    .copied(),
                optics: entity.get::<gradiance_domain::optics::Optics>().copied(),
                lifetime: entity
                    .get::<gradiance_domain::lifetime::Lifetime>()
                    .copied(),
            }
        })
        .collect();
    rows.sort_by_key(|b| b.id.0);
    let mut joint_defs: Vec<(StableId, JointDef)> = world
        .query_filtered::<(&StableId, Option<&JointDef>), With<gradiance_domain::Joint>>()
        .iter(world)
        .filter_map(|(id, def)| def.map(|d| (*id, d.clone())))
        .collect();
    joint_defs.sort_by_key(|(id, _)| id.0);
    let links: Vec<(StableId, Option<StableId>)> = joint_defs
        .iter()
        .map(|(_, d)| (d.body_a, d.body_b))
        .collect();
    let mut joints: Vec<StableId> = world
        .query_filtered::<&StableId, With<gradiance_domain::Joint>>()
        .iter(world)
        .copied()
        .collect();
    joints.sort_by_key(|id| id.0);
    let group_names = world
        .get_resource::<GroupNames>()
        .cloned()
        .unwrap_or_default();
    let mut nodes: Vec<StableId> = world
        .query_filtered::<&StableId, With<gradiance_domain::node::BehaviorNode>>()
        .iter(world)
        .copied()
        .collect();
    nodes.sort_by_key(|id| id.0);
    SceneSnapshot {
        joints,
        nodes,
        bodies: rows,
        joint_links: links,
        joint_defs,
        group_names,
    }
}

impl SceneSnapshot {
    /// Every group, in id order, members in body (id) order.
    pub fn group_index(&self) -> GroupIndex {
        GroupIndex::build(self.bodies.iter().map(|b| (b.id, b.groups.as_slice())))
    }

    /// The index of the body with this id.
    pub fn index_of(&self, id: StableId) -> Option<usize> {
        self.bodies.iter().position(|b| b.id == id)
    }

    /// Number of bodies, as a count.
    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    /// Whether the scene has no body.
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }
}

impl ResolveTarget for SceneSnapshot {
    /// Bodies: an index in id order, an id that exists, or the first body
    /// whose shape holds the point. The snapshot holds no selection, so
    /// [`Target::Selection`] names nothing here.
    fn resolve(&self, target: &Target) -> Option<StableId> {
        match *target {
            Target::Index(i) => usize::try_from(i).ok().and_then(|i| self.id(i)),
            Target::Id(id) => self.bodies.iter().any(|b| b.id == id).then_some(id),
            Target::At(p) => self.bodies.iter().find(|b| b.contains(p)).map(|b| b.id),
            Target::Selection => None,
        }
    }
}
