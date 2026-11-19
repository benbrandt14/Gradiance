//! The scene operations this layer owns, declared once for every doorway
//! (`gradiance-op`; plan `docs/unified-surface-plan.md` § 3, § 13).
//!
//! Each one is a parameter struct that lowers to the intent (or value, or
//! settings write) its seam already takes. Doorways reach them by id through
//! the [`OpRegistry`]; none names these types.
//!
//! The files split by family: [`spawn`] authors new geometry, [`body`] edits
//! one existing body, [`sets`] edits several at once (booleans, groups),
//! [`joint`] makes and removes relationships, [`scene`] holds the history,
//! the new scene, a count and the settings write, [`look`] the scene look
//! and its settings writes, [`query`] the read-only queries,
//! [`quantity`] the quantity catalog's reads, and [`units`] the unit
//! conversions a script writes its numbers with.
//!
//! # Targets and the selection
//!
//! A [`Target`] names scene objects. `"selection"` resolves against the
//! [`SelectionMirror`](gradiance_op::SelectionMirror) the interaction layer publishes, so menus and keys can
//! name what is selected; the Lisp doorway withdraws that mirror, so there it
//! is a visible error ([`OpContext::selection`]). Where the intent holds a set
//! (delete, the property edits, the booleans, the groups) the selection names
//! every selected body; where it holds one body (place, scale, the joint
//! ends) it names the primary (first-selected) body.

pub mod body;
pub mod group;
pub mod joint;
pub mod look;
pub mod quantity;
pub mod query;
pub mod scene;
pub mod sets;
pub mod spawn;
pub mod units;

use crate::CommandDispatchSet;
use crate::snapshot::{BodySnapshot, SceneSnapshot, snapshot_scene};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::props::BodyPhysics;
use gradiance_domain::settings::{LightingSettings, RenderSettings, ScenerySettings, SimSettings};
use gradiance_domain::shape::ShapeDef;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpInvokeSet, OpPlugin, OpRegistry, ResolveTarget, Target,
};
use gradiance_scene::BodyRecord;

/// A body of `shape` at `pos`, styled like a tool-drawn one: the record every
/// scripted or remote spawn authors. The colour derives from `id`, so a
/// scripted scene and a hand-built one look the same.
pub fn spawned_body_record(id: StableId, shape: ShapeDef, pos: Vec2) -> BodyRecord {
    BodyRecord {
        id,
        pose: PosRot { pos, rot: 0.0 },
        shape,
        physics: BodyPhysics::default(),
        appearance: Appearance::for_id(id),
        depth: DepthBand::default(),
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
    }
}

/// The bodies `target` names, first one first: one for an index, an id or a
/// point (none when it names nothing), every selected body for `"selection"`.
pub(crate) fn bodies_of(
    ctx: &OpContext,
    scene: &SceneSnapshot,
    target: &Target,
) -> Result<Vec<StableId>, OpError> {
    match target {
        Target::Selection => Ok(ctx
            .selection()?
            .objects
            .iter()
            .copied()
            .filter(|id| scene.get(*id).is_some())
            .collect()),
        other => Ok(scene.resolve(other).into_iter().collect()),
    }
}

/// The one body `target` names; `"selection"` names the primary body.
pub(crate) fn body_of<'s>(
    ctx: &OpContext,
    scene: &'s SceneSnapshot,
    target: &Target,
) -> Result<Option<&'s BodySnapshot>, OpError> {
    Ok(bodies_of(ctx, scene, target)?
        .first()
        .and_then(|id| scene.get(*id)))
}

/// The bodies a set-shaped param names, distinct, first occurrence first.
///
/// Every target must name something: a set edit that quietly dropped one
/// member would author a *different* set than the caller wrote, and nothing
/// in the scene would show it. `"selection"` is the exception: an empty
/// selection is a set with no members.
pub(crate) fn body_set(
    ctx: &OpContext,
    scene: &SceneSnapshot,
    targets: &[Target],
) -> Result<Vec<StableId>, OpError> {
    let mut ids = Vec::new();
    for target in targets {
        let named = bodies_of(ctx, scene, target)?;
        if named.is_empty() && *target != Target::Selection {
            return Err(OpError::Invalid(match target {
                Target::Index(i) => format!("no body {i} (the scene has {})", scene.len()),
                other => format!("{other:?} names no body"),
            }));
        }
        for id in named {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    Ok(ids)
}

/// The joint `target` names: an index in id order, an id that exists, or the
/// selected joint. A point names no joint (the snapshot has no joint
/// geometry).
pub(crate) fn joint_of(
    ctx: &OpContext,
    scene: &SceneSnapshot,
    target: &Target,
) -> Result<Option<StableId>, OpError> {
    Ok(match *target {
        Target::Index(i) => usize::try_from(i).ok().and_then(|i| scene.joint_id(i)),
        Target::Id(id) => scene.joints.contains(&id).then_some(id),
        Target::Selection => ctx.selection()?.joint,
        Target::At(_) => None,
    })
}

/// Publishes this layer's mirrors into a new operation context: the scene
/// snapshot, the quantity samples and the simulation, render, lighting and
/// scenery settings.
fn fill_context(world: &mut World, ctx: &mut OpContext) {
    ctx.insert(snapshot_scene(world));
    // The last operation, while the history is still at its step.
    let revision = world
        .get_resource::<crate::CommandStack>()
        .map(crate::CommandStack::revision);
    let last = world
        .get_resource::<gradiance_op::LastOp>()
        .and_then(|last| last.0.clone())
        .filter(|last| Some(last.step) == revision);
    ctx.insert(gradiance_op::LastOp(last));
    ctx.insert(gradiance_signal::sample_quantities(world));
    if let Some(settings) = world.get_resource::<SimSettings>() {
        ctx.insert(settings.clone());
    }
    if let Some(settings) = world.get_resource::<RenderSettings>() {
        ctx.insert(settings.clone());
    }
    if let Some(settings) = world.get_resource::<LightingSettings>() {
        ctx.insert(settings.clone());
    }
    if let Some(settings) = world.get_resource::<ScenerySettings>() {
        ctx.insert(settings.clone());
    }
}

/// Registers this layer's operations and its context mirrors.
#[derive(Default)]
pub struct SceneOpsPlugin;

impl Plugin for SceneOpsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OpPlugin>() {
            app.add_plugins(OpPlugin);
        }
        {
            let mut registry = app.world_mut().resource_mut::<OpRegistry>();
            spawn::register(&mut registry);
            body::register(&mut registry);
            group::register(&mut registry);
            sets::register(&mut registry);
            joint::register(&mut registry);
            scene::register(&mut registry);
            look::register(&mut registry);
            query::register(&mut registry);
            quantity::register(&mut registry);
            units::register(&mut registry);
        }
        app.world_mut()
            .resource_mut::<OpContextFillers>()
            .add(fill_context);
        // An invoked edit becomes a command in the same frame.
        app.configure_sets(Update, OpInvokeSet.before(CommandDispatchSet));
    }
}

/// Adds [`SceneOpsPlugin`] once. The command and script layers both call it,
/// so an app with either one has the scene operations.
pub fn install(app: &mut App) {
    if !app.is_plugin_added::<SceneOpsPlugin>() {
        app.add_plugins(SceneOpsPlugin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_op::{OpIdSource, SelectionMirror};

    fn scene(n: u64) -> SceneSnapshot {
        SceneSnapshot {
            bodies: (1..=n)
                .map(|i| BodySnapshot {
                    id: id(i),
                    physics: BodyPhysics::default(),
                    pos: Vec2::ZERO,
                    rot: 0.0,
                    shape: ShapeDef::Circle { radius: 0.5 },
                    touching: 0,
                    appearance: Appearance::default(),
                    depth: DepthBand::default(),
                    groups: Vec::new(),
                })
                .collect(),
            joints: vec![id(99)],
            nodes: Vec::new(),
            joint_links: Vec::new(),
            group_names: gradiance_domain::group::GroupNames::default(),
        }
    }

    /// The id numbered `n` (a fresh context's fixed sequence).
    fn id(n: u64) -> StableId {
        OpContext::new(OpIdSource::Sequence(n)).new_id()
    }

    #[test]
    fn the_selection_names_every_selected_body_or_the_primary_one() {
        let scene = scene(3);
        let mut ctx = OpContext::new(OpIdSource::Random);
        ctx.insert(SelectionMirror {
            objects: vec![id(3), id(1)],
            joint: None,
        });
        let all = bodies_of(&ctx, &scene, &Target::Selection).ok();
        assert_eq!(all, Some(vec![id(3), id(1)]));
        let primary = body_of(&ctx, &scene, &Target::Selection)
            .ok()
            .flatten()
            .map(|b| b.id);
        assert_eq!(primary, Some(id(3)));
    }

    #[test]
    fn without_a_selection_mirror_the_selection_is_an_error() {
        let scene = scene(2);
        let ctx = OpContext::new(OpIdSource::Random);
        assert!(matches!(
            bodies_of(&ctx, &scene, &Target::Selection),
            Err(OpError::NoSelection)
        ));
        assert!(matches!(
            joint_of(&ctx, &scene, &Target::Selection),
            Err(OpError::NoSelection)
        ));
        // Other targets do not need the mirror.
        assert_eq!(
            bodies_of(&ctx, &scene, &Target::Index(1)).ok(),
            Some(vec![id(2)])
        );
    }

    #[test]
    fn a_set_member_that_names_nothing_is_an_error() {
        let scene = scene(2);
        let ctx = OpContext::new(OpIdSource::Random);
        let set = body_set(&ctx, &scene, &[Target::Index(0), Target::Index(0)]);
        assert_eq!(set.ok(), Some(vec![id(1)]), "distinct members");
        let err = body_set(&ctx, &scene, &[Target::Index(0), Target::Index(5)])
            .err()
            .map(|e| e.to_string());
        assert_eq!(err.as_deref(), Some("no body 5 (the scene has 2)"));
    }
}
