//! The chain operation (brief N-5): a run of capsule links hinged end to
//! end along a path, one undo step.
//!
//! `body.chain` is what the chain tool commits: the tool invokes it with
//! the stroke it recorded and the bodies it found under the ends, so a
//! drawn chain and a scripted one are the same records, and Adjust Last
//! Operation can change a drawn chain's link length afterwards.
//!
//! The links are laid by `geometry::chain::chain_points` (equal arc
//! length, ends exactly on the path's ends). Link `i` is a capsule from
//! point `i` to point `i + 1`; a hinge joins consecutive links at their
//! shared point. The ends are pinned by `pins`: `"free"` leaves both
//! free; `"bodies"` hinges an end to the body `a` (start) or `b` (end)
//! names, if any; `"world"` does the same and pins an end that names no
//! body to the world where it lies.

use super::body_of;
use crate::intent::SpawnChainIntent;
use crate::intent::name;
use crate::snapshot::{BodySnapshot, SceneSnapshot};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::joint::{JointCommon, JointDef, JointKind};
use gradiance_domain::settings::ChainPins;
use gradiance_domain::shape::ShapeDef;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_scene::{BodyRecord, JointRecord};
use gradiance_units::{Angle, Displacement, Length};
use serde::Deserialize;

/// Link density (kg/m²), the rod's: a thin capsule at unit density is the
/// light side of a bad solver mass ratio against a body it hangs from.
pub const CHAIN_DENSITY: f32 = 5.0;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Chain>();
}

/// Lay a chain of capsule links about `link` long and `width` wide along
/// `path` (flat world coordinates `[x0, y0, x1, y1, …]`), its ends pinned
/// by `pins` to `a` (start) and `b` (end).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Chain {
    /// The path, world coordinates in pairs: `[x0, y0, x1, y1, …]`.
    pub path: Vec<f32>,
    /// Link length, hinge to hinge (the path takes the nearest whole number
    /// of links).
    pub link: Length,
    /// Link width (the capsule's diameter).
    pub width: Length,
    /// The body the start is pinned to (a negative index: none).
    pub a: Target,
    /// The body the end is pinned to (a negative index: none).
    pub b: Target,
    /// `"free"`, `"bodies"` or `"world"` (see the module docs).
    pub pins: String,
}

impl Operation for Chain {
    const META: OpMeta = OpMeta {
        id: "body.chain",
        lisp: "chain",
        doc: "Lay a chain of capsule links (about link long, width wide) hinged end to end along path [x0 y0 x1 y1 …]; pins \"free\", \"bodies\" (start to body a, end to body b, when they name one) or \"world\" (else to the world). Returns the links' ids.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_CHAIN),
        example: r#"{"path": [-1, 1, 0, 0, 1, 1], "link": 0.25, "width": 0.06, "a": -1, "b": -1, "pins": "world"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let pins = ChainPins::named(&self.pins).ok_or_else(|| {
            OpError::Invalid(format!(
                "pins {:?}: expected \"free\", \"bodies\" or \"world\"",
                self.pins
            ))
        })?;
        if !self.path.len().is_multiple_of(2) {
            return Err(OpError::Invalid(
                "path: an even count of numbers, x y pairs".to_owned(),
            ));
        }
        let width = self.width.0;
        if !(width.is_finite() && width > 0.0) {
            return Err(OpError::Invalid(format!(
                "width {width}: a positive length"
            )));
        }
        let path: Vec<Vec2> = self
            .path
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[x, y]| Vec2::new(*x, *y))
            .collect();
        let points = gradiance_geometry::chain::chain_points(&path, self.link.0).ok_or_else(|| {
            OpError::Invalid(format!(
                "no chain: the path needs two finite points at least {} m apart along it, and link a positive length",
                gradiance_geometry::chain::MIN_CHAIN_LENGTH
            ))
        })?;
        let ends = match pins {
            ChainPins::Free => [EndPin::Free, EndPin::Free],
            ChainPins::Bodies | ChainPins::World => {
                let world = pins == ChainPins::World;
                let pin = |target: &Target| -> Result<EndPin, OpError> {
                    Ok(match body_of(ctx, scene, target)? {
                        Some(body) => EndPin::Body(body),
                        None if world => EndPin::World,
                        None => EndPin::Free,
                    })
                };
                [pin(&self.a)?, pin(&self.b)?]
            }
        };
        let (bodies, joints) = chain_records(&points, width / 2.0, &ends, &mut || ctx.new_id());
        let ids: Vec<gradiance_op::Value> = bodies
            .iter()
            .map(|b| gradiance_op::Value::String(b.id.0.to_string()))
            .collect();
        Ok(OpOutput::none()
            .message(SpawnChainIntent { bodies, joints })
            .returning(gradiance_op::Value::Array(ids)))
    }
}

/// What one end of the chain is pinned to.
#[derive(Debug, Clone, Copy)]
pub enum EndPin<'s> {
    /// Nothing: the end hangs free.
    Free,
    /// The world, where the end lies.
    World,
    /// A body in the scene.
    Body(&'s BodySnapshot),
}

/// The links along `points` (capsules of `radius`, link `i` from point `i`
/// to point `i + 1`), the hinges joining consecutive links at their shared
/// point, then a hinge for each pinned end (`ends`: start, end). Ids come
/// from `new_id`, links first, in order.
pub fn chain_records(
    points: &[Vec2],
    radius: f32,
    ends: &[EndPin; 2],
    new_id: &mut dyn FnMut() -> StableId,
) -> (Vec<BodyRecord>, Vec<JointRecord>) {
    let bodies: Vec<BodyRecord> = points
        .windows(2)
        .map(|w| link_record(new_id(), w[0], w[1], radius))
        .collect();
    let mut joints = Vec::with_capacity(bodies.len() + 1);
    for (i, pair) in bodies.windows(2).enumerate() {
        let at = points[i + 1];
        joints.push(hinge(
            new_id(),
            &pair[0],
            Some((pair[1].id, pair[1].pose)),
            at,
        ));
    }
    if let (Some(first), Some(last)) = (bodies.first(), bodies.last()) {
        for (link, end, at) in [
            (first, ends[0], points[0]),
            (last, ends[1], points[points.len() - 1]),
        ] {
            match end {
                EndPin::Free => {}
                EndPin::World => joints.push(hinge(new_id(), link, None, at)),
                EndPin::Body(body) => {
                    joints.push(hinge(new_id(), link, Some((body.id, body.pose())), at));
                }
            }
        }
    }
    (bodies, joints)
}

/// One link: a capsule spanning `a`→`b` (its segment ends are the hinge
/// points), styled like the rod.
fn link_record(id: StableId, a: Vec2, b: Vec2, radius: f32) -> BodyRecord {
    let mut body = super::spawned_body_record(
        id,
        ShapeDef::Capsule {
            half_length: a.distance(b) / 2.0,
            radius,
        },
        (a + b) / 2.0,
    );
    body.pose.rot = (b - a).to_angle();
    body.physics.density = avian2d::prelude::ColliderDensity(CHAIN_DENSITY);
    body.appearance = Appearance {
        // The look's rod tone (canonical; the render layer tones it).
        fill: gradiance_domain::theme::STUDIO.rod,
        ..Appearance::default()
    };
    body
}

/// A hinge from `link` at the world point `at` to `other` (a body id and
/// pose), or to the world when `other` is `None`.
fn hinge(
    id: StableId,
    link: &BodyRecord,
    other: Option<(StableId, PosRot)>,
    at: Vec2,
) -> JointRecord {
    let to_local = |pose: PosRot| Vec2::from_angle(-pose.rot).rotate(at - pose.pos);
    let (body_b, anchor_b, rest_rot_b) = match other {
        Some((other, pose)) => (Some(other), to_local(pose), pose.rot),
        // A world pin's far anchor is the world point itself.
        None => (None, at, 0.0),
    };
    JointRecord {
        id,
        def: JointDef {
            kind: JointKind::Hinge {
                limits: None,
                motor: None,
            },
            common: JointCommon::default(),
            body_a: link.id,
            body_b,
            anchor_a: Displacement(to_local(link.pose)),
            anchor_b: Displacement(anchor_b),
            rest_rot_a: Angle(link.pose.rot),
            rest_rot_b: Angle(rest_rot_b),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> impl FnMut() -> StableId {
        StableId::new
    }

    /// Where a body-local anchor sits in the world.
    fn world_of(pose: PosRot, local: Vec2) -> Vec2 {
        pose.pos + Vec2::from_angle(pose.rot).rotate(local)
    }

    #[test]
    fn links_span_consecutive_points_and_hinges_join_them_there() {
        let points = [Vec2::ZERO, Vec2::new(0.25, 0.0), Vec2::new(0.25, -0.25)];
        let (bodies, joints) =
            chain_records(&points, 0.03, &[EndPin::Free, EndPin::Free], &mut ids());
        assert_eq!(bodies.len(), 2);
        assert_eq!(joints.len(), 1, "one hinge between two free links");
        for (i, body) in bodies.iter().enumerate() {
            let ShapeDef::Capsule {
                half_length,
                radius,
            } = body.shape
            else {
                panic!("a capsule link");
            };
            assert!((half_length - 0.125).abs() < 1e-6 && (radius - 0.03).abs() < 1e-6);
            let a = world_of(body.pose, Vec2::new(-half_length, 0.0));
            let b = world_of(body.pose, Vec2::new(half_length, 0.0));
            assert!(a.distance(points[i]) < 1e-5 && b.distance(points[i + 1]) < 1e-5);
            assert!((body.physics.density.0 - CHAIN_DENSITY).abs() < f32::EPSILON);
        }
        let j = &joints[0].def;
        assert_eq!((j.body_a, j.body_b), (bodies[0].id, Some(bodies[1].id)));
        assert!(world_of(bodies[0].pose, j.anchor_a.value()).distance(points[1]) < 1e-5);
        assert!(world_of(bodies[1].pose, j.anchor_b.value()).distance(points[1]) < 1e-5);
        assert!(matches!(j.kind, JointKind::Hinge { .. }));
    }

    #[test]
    fn pinned_ends_hinge_to_the_world_or_a_body_at_the_end_points() {
        let points = [Vec2::new(-1.0, 0.0), Vec2::ZERO, Vec2::new(1.0, 0.0)];
        let body = BodySnapshot {
            id: StableId::new(),
            physics: gradiance_domain::props::BodyPhysics::default(),
            pos: Vec2::new(1.5, 0.5),
            rot: 0.4,
            shape: ShapeDef::Box {
                width: 2.0,
                height: 2.0,
            },
            touching: 0,
            appearance: Appearance::default(),
            depth: gradiance_domain::depth::DepthBand::default(),
            groups: Vec::new(),
            fluid: None,
            motion: None,
            optics: None,
            lifetime: None,
            surface: None,
            texture: None,
        };
        let (bodies, joints) = chain_records(
            &points,
            0.03,
            &[EndPin::World, EndPin::Body(&body)],
            &mut ids(),
        );
        assert_eq!(joints.len(), 3, "one between, two pins");
        let start = &joints[1].def;
        assert_eq!((start.body_a, start.body_b), (bodies[0].id, None));
        assert_eq!(
            start.anchor_b.value(),
            points[0],
            "a world pin's anchor is the point"
        );
        let end = &joints[2].def;
        assert_eq!((end.body_a, end.body_b), (bodies[1].id, Some(body.id)));
        assert!(world_of(body.pose(), end.anchor_b.value()).distance(points[2]) < 1e-5);
        assert!((end.rest_rot_b.0 - 0.4).abs() < 1e-6);
    }
}
