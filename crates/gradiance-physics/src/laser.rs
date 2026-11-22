//! Laser beams: ray casts that reflect off bodies (brief N-8).
//!
//! A laser node ([`NodeKind::Laser`]) casts a ray from its point along its
//! axis with avian's [`SpatialQuery`] (through the read facade's
//! [`cast_beam`]), reflects off every body it hits
//! (`geometry::beam` does the reflection), and ends at its range, after its
//! last bounce, or on its target body. The result is the derived
//! [`LaserBeam`] on the node: the path the overlay draws, whether the beam
//! reaches the target, and its length. The signal layer publishes the last
//! two on the bus. None of it is saved or undone (invariant 5).
//!
//! [`trace_lasers`] runs in `FixedPostUpdate`, after avian's step, so each
//! step's beam sees that step's bodies. It runs while paused too (a placed or
//! edited laser shows its beam at once), against the colliders as the last
//! step left them.
//!
//! What a beam hits: every collider except sensors (fluid regions) and the
//! body the laser rides. A laser on a body hits only the bodies that share a
//! collision layer with it (collision layer ≡ visual depth); a free laser
//! hits every depth.

use crate::queries::cast_beam;
use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_core::ids::IdIndex;
use gradiance_domain::Body;
use gradiance_domain::laser::Laser;
use gradiance_domain::node::{BehaviorNode, NodeAttachment, NodeKind};

/// The derived beam of a laser node: its path, whether it reaches the
/// target, its length, and the bus names it publishes under. Rebuilt every
/// physics step by [`trace_lasers`]; never saved, never undone.
#[derive(Component, Debug, Clone, PartialEq, Default)]
pub struct LaserBeam {
    /// The beam's corners: the origin, each hit, and its end. Reused: its
    /// capacity is reserved once for the bounce cap, so a step never grows
    /// it.
    pub path: Vec<Vec2>,
    /// Whether the beam ends on the laser's target body.
    pub hit: bool,
    /// The path length from the laser to the end of the beam (m).
    pub distance: f32,
    /// The bus name of the hit signal (the laser's name).
    pub hit_name: String,
    /// The bus name of the distance signal (`<name>.distance`).
    pub distance_name: String,
}

impl LaserBeam {
    /// An empty beam for `laser`, with room for its longest path.
    #[must_use]
    pub fn for_laser(laser: &Laser) -> Self {
        Self {
            path: Vec::with_capacity(gradiance_domain::laser::MAX_BOUNCES as usize + 2),
            hit: false,
            distance: 0.0,
            hit_name: laser.name.clone(),
            distance_name: laser.distance_name(),
        }
    }

    /// Re-derives the bus names when the laser was renamed (allocates only
    /// then).
    fn rename(&mut self, laser: &Laser) {
        if self.hit_name != laser.name {
            self.hit_name.clone_from(&laser.name);
            self.distance_name = laser.distance_name();
        }
    }
}

/// The world origin and unit direction of a laser: on a body at `body`
/// (position, rotation in radians) the node's offset and axis turn with it;
/// a free laser sits at `free_at` and aims along its world angle. Pure.
#[must_use]
pub fn laser_ray(
    laser: &Laser,
    attachment: &NodeAttachment,
    body: Option<(Vec2, f32)>,
    free_at: Vec2,
) -> (Vec2, Vec2) {
    match body {
        Some((pos, rot)) => (
            pos + Vec2::from_angle(rot).rotate(attachment.offset.value()),
            Vec2::from_angle(rot + laser.angle.0),
        ),
        None => (free_at, Vec2::from_angle(laser.angle.0)),
    }
}

/// **The laser pass:** one system over the behavior nodes. Each laser casts
/// its beam through avian's [`SpatialQuery`] (one ray per segment) and
/// writes its [`LaserBeam`]; a new laser gets one (a command, once).
///
/// Cost: `O(L × (B + 1))` ray casts per step for `L` lasers with at most `B`
/// bounces each (`B ≤ MAX_BOUNCES`), each cast a BVH traversal of avian's
/// collider trees; per laser one id lookup for its body and one for its
/// target. No allocation in the steady state: the path buffer is reused and
/// the bus names are rebuilt only on a rename.
#[expect(clippy::type_complexity, reason = "two filtered queries")]
pub fn trace_lasers(
    mut commands: Commands,
    index: Res<IdIndex>,
    spatial: SpatialQuery,
    mut lasers: Query<
        (
            Entity,
            &NodeKind,
            &NodeAttachment,
            &Transform,
            Option<&mut LaserBeam>,
        ),
        With<BehaviorNode>,
    >,
    bodies: Query<(&Position, &Rotation, Option<&CollisionLayers>), With<Body>>,
    sensors: Query<(), With<Sensor>>,
    optics: Query<&gradiance_domain::optics::Optics, With<Body>>,
) {
    for (entity, kind, attachment, transform, beam) in &mut lasers {
        let NodeKind::Laser(laser) = kind else {
            if beam.is_some() {
                commands.entity(entity).try_remove::<LaserBeam>();
            }
            continue;
        };
        let Some(mut beam) = beam else {
            commands
                .entity(entity)
                .try_insert(LaserBeam::for_laser(laser));
            continue;
        };
        beam.rename(laser);
        let rides = attachment.target.and_then(|id| index.entity(id));
        let body = rides.and_then(|e| bodies.get(e).ok());
        let (origin, dir) = laser_ray(
            laser,
            attachment,
            body.map(|(p, r, _)| (p.0, r.as_radians())),
            transform.translation.truncate(),
        );
        let filter = body
            .and_then(|(_, _, layers)| layers)
            .map_or_else(SpatialQueryFilter::default, |l| {
                SpatialQueryFilter::from_mask(l.memberships)
            });
        let target = laser.target.and_then(|id| index.entity(id));
        let skip = |e: Entity| Some(e) != rides && !sensors.contains(e);
        let LaserBeam { path, .. } = &mut *beam;
        let end = cast_beam(
            &spatial,
            origin,
            dir,
            laser.reach(),
            laser.bounce_limit(),
            &filter,
            &skip,
            target,
            &|e| optics.get(e).ok().map(|o| o.sanitized().index),
            path,
        );
        beam.hit = end.absorbed;
        beam.distance = end.length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_core::ids::StableId;
    use gradiance_units::Angle;

    #[test]
    fn a_laser_on_a_body_turns_with_it() {
        let laser = Laser {
            angle: Angle(0.0),
            ..Laser::default()
        };
        let attachment = NodeAttachment::to(StableId::new(), Vec2::new(1.0, 0.0));
        let (o, d) = laser_ray(
            &laser,
            &attachment,
            Some((Vec2::new(2.0, 0.0), core::f32::consts::FRAC_PI_2)),
            Vec2::ZERO,
        );
        assert!((o - Vec2::new(2.0, 1.0)).length() < 1e-5, "{o}");
        assert!((d - Vec2::Y).length() < 1e-5, "{d}");
    }

    #[test]
    fn a_free_laser_aims_along_its_world_angle() {
        let laser = Laser {
            angle: Angle(core::f32::consts::PI),
            ..Laser::default()
        };
        let (o, d) = laser_ray(&laser, &NodeAttachment::free(), None, Vec2::ONE);
        assert_eq!(o, Vec2::ONE);
        assert!((d - Vec2::NEG_X).length() < 1e-5, "{d}");
    }

    #[test]
    fn a_rename_rebuilds_the_bus_names() {
        let mut beam = LaserBeam::for_laser(&Laser::default());
        assert_eq!(beam.distance_name, "laser.distance");
        beam.rename(&Laser {
            name: "gate".into(),
            ..Laser::default()
        });
        assert_eq!(beam.hit_name, "gate");
        assert_eq!(beam.distance_name, "gate.distance");
    }
}
