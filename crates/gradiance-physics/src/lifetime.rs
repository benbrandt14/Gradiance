//! Contact hits and run-time removal (brief S-7).
//!
//! - [`ContactHits`]: the bodies that started a contact during the last
//!   physics step, from avian's `CollisionStart` messages. The `hit` signal
//!   reads it (1 on that step, else 0): the reference sandbox runs a script
//!   on collide, but the VM never runs per frame, so the hook is a signal
//!   edge instead.
//! - [`Removed`]: a **derived** marker. A body with an authored
//!   [`Lifetime`] removes every body it starts touching, or itself after its
//!   time. A removed body keeps its entity and its authored state; it loses
//!   its collider and its motion (`ColliderDisabled`, `RigidBodyDisabled`)
//!   and its `Visibility` goes to hidden. Nothing is despawned, so no world mutation
//!   happens outside commands (invariant 1), and [`ClearRemovals`] (sent when
//!   a run is undone or a scene loads) brings every body back.

use avian2d::prelude::*;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::fluid::Fluid;
use gradiance_domain::lifetime::{ClearRemovals, Lifetime};
use gradiance_domain::settings::ScenerySettings;
use gradiance_domain::shape::ShapeDef;

/// The bodies that started touching something during the last physics step.
#[derive(Resource, Default, Debug)]
pub struct ContactHits(pub HashSet<Entity>);

/// Derived: a body taken out of the run (brief S-7). Never saved, never in
/// undo records.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Removed;

/// Derived: simulated seconds a body with a time to live has run.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct RunAge(pub f32);

/// Installs the hit set, the removal marker and their systems.
pub(crate) fn install(app: &mut App) {
    app.init_resource::<ContactHits>();
    app.add_message::<ClearRemovals>();
    // Every body reports its contact starts (avian sends `CollisionStart`
    // only for colliders that opt in).
    app.register_required_components::<Body, CollisionEventsEnabled>();
    app.add_systems(FixedFirst, clear_hits);
    app.add_systems(
        FixedPostUpdate,
        (collect_hits, remove_by_lifetime)
            .chain()
            .after(PhysicsSystems::Last)
            .run_if(crate::sim_running),
    );
    app.add_systems(Update, clear_removals);
}

fn clear_hits(mut hits: ResMut<ContactHits>) {
    if !hits.0.is_empty() {
        hits.0.clear();
    }
}

/// Records the bodies (and colliders) that started a contact this step.
fn collect_hits(mut starts: MessageReader<CollisionStart>, mut hits: ResMut<ContactHits>) {
    for start in starts.read() {
        hits.0.extend(
            [start.collider1, start.collider2]
                .into_iter()
                .chain(start.body1)
                .chain(start.body2),
        );
    }
}

/// Marks what this step removed: every body a remove-on-contact body
/// started touching (not a fluid region), and every body past its time.
#[allow(clippy::type_complexity)]
fn remove_by_lifetime(
    mut commands: Commands,
    mut starts: MessageReader<CollisionStart>,
    removers: Query<&Lifetime, Without<Removed>>,
    targets: Query<(), (With<Body>, Without<Removed>, Without<Fluid>)>,
    mut aging: Query<(Entity, &Lifetime, Option<&mut RunAge>), (With<Body>, Without<Removed>)>,
    fixed: Res<Time<Fixed>>,
    physics_time: Res<Time<Physics>>,
) {
    let mut removed: Vec<Entity> = Vec::new();
    for start in starts.read() {
        let a = start.body1.unwrap_or(start.collider1);
        let b = start.body2.unwrap_or(start.collider2);
        for (remover, other) in [(a, b), (b, a)] {
            if removers.get(remover).is_ok_and(|l| l.remove_on_contact) && targets.contains(other) {
                removed.push(other);
            }
        }
    }
    let dt = fixed.timestep().as_secs_f32() * physics_time.relative_speed();
    for (entity, lifetime, age) in &mut aging {
        let Some(ttl) = lifetime.ttl else {
            continue;
        };
        let now = age.as_ref().map_or(0.0, |a| a.0) + dt;
        match age {
            Some(mut age) => age.0 = now,
            None => {
                commands.entity(entity).try_insert(RunAge(now));
            }
        }
        // Half a step of slack: the step that reaches the time removes it.
        if now + dt * 0.5 >= ttl.0 {
            removed.push(entity);
        }
    }
    for entity in removed {
        commands.entity(entity).try_insert((
            Removed,
            ColliderDisabled,
            RigidBodyDisabled,
            Visibility::Hidden,
        ));
    }
}

/// Answers [`ClearRemovals`]: every removed body back, every age reset.
fn clear_removals(
    mut commands: Commands,
    mut requests: MessageReader<ClearRemovals>,
    removed: Query<(Entity, &ShapeDef), With<Removed>>,
    aged: Query<Entity, With<RunAge>>,
    scenery: Option<Res<ScenerySettings>>,
) {
    if requests.read().count() == 0 {
        return;
    }
    // A ground stays hidden when the scenery hides grounds.
    let grounds_visible = scenery.is_none_or(|s| s.ground_visible);
    for (entity, shape) in &removed {
        let visible = grounds_visible || !matches!(shape, ShapeDef::HalfPlane);
        commands
            .entity(entity)
            .remove::<(Removed, ColliderDisabled, RigidBodyDisabled)>()
            .insert(if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
    }
    for entity in &aged {
        commands.entity(entity).remove::<RunAge>();
    }
}
