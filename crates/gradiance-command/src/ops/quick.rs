//! One-click **quick actions** on a body or a joint (brief S-6): loosen,
//! pin at the centre, thruster at the centre, to a circle, brake a motor.
//!
//! Each lowers to one existing intent, so each is one undo step: loosen is a
//! [`DeleteIntent`] naming joints, the pin a [`SpawnJointIntent`], the
//! thruster a [`SpawnNodeIntent`], the circle and the brake a
//! [`PropertyEditIntent`]. The context menu reaches them by id from one
//! table (`ui::context_menu::QUICK_ACTIONS`).

use super::joint::joint_record;
use super::{bodies_of, body_of, joint_of};
use crate::intent::name;
use crate::intent::{DeleteIntent, PropertyEditIntent, SpawnJointIntent, SpawnNodeIntent};
use crate::property::{PropertyChange, PropertyValue};
use crate::snapshot::SceneSnapshot;
use bevy::prelude::*;
use gradiance_core::units::PosRot;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::joint::{AngularMotorDef, JointKind};
use gradiance_domain::node::{NodeAttachment, NodeKind};
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::polygonize::polygonize;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_scene::NodeRecord;
use gradiance_units::AngularVelocity;
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Loosen>();
    registry.register::<PinCentre>();
    registry.register::<ThrusterCentre>();
    registry.register::<ToCircle>();
    registry.register::<Brake>();
}

/// Drop every joint of the body `i` (`"selection"`: of every selected body)
/// in one undo step. The bodies stay.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Loosen {
    /// The body.
    pub i: Target,
}

impl Operation for Loosen {
    const META: OpMeta = OpMeta {
        id: "body.loosen",
        lisp: "loosen",
        doc: "Drop every joint of the i-th body (the bodies stay) — one undo step.",
        seam: Seam::Edit,
        menu: Some("Context/Loosen"),
        chord: None,
        undo: Some(name::DELETE),
        example: r#"{"i": 3}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let bodies = bodies_of(ctx, scene, &self.i)?;
        let targets: Vec<_> = scene
            .joint_defs
            .iter()
            .filter(|(_, def)| {
                bodies.contains(&def.body_a) || def.body_b.is_some_and(|b| bodies.contains(&b))
            })
            .map(|(id, _)| *id)
            .collect();
        if targets.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(DeleteIntent { targets }))
    }
}

/// Pin the body `i` to the world with a hinge at its centre (its origin):
/// it can only spin.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PinCentre {
    /// The body.
    pub i: Target,
}

impl Operation for PinCentre {
    const META: OpMeta = OpMeta {
        id: "body.pin_centre",
        lisp: "pin-centre",
        doc: "Hinge the i-th body to the world at its centre, so it can only spin — undoable. Returns the joint's handle.",
        seam: Seam::Edit,
        menu: Some("Context/Pin at centre"),
        chord: None,
        undo: Some(name::SPAWN_JOINT),
        example: r#"{"i": 0}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(body) = body_of(ctx, scene, &self.i)? else {
            return Ok(OpOutput::none().returning(""));
        };
        let kind = JointKind::Hinge {
            limits: None,
            motor: None,
        };
        let record = joint_record(ctx, body, None, [body.pos, body.pos], kind, false);
        let handle = record.id.0.to_string();
        Ok(OpOutput::none()
            .message(SpawnJointIntent { record })
            .returning(handle))
    }
}

/// Attach a thruster to the body `i` at its centre, pushing along its local
/// +y with the default thrust (the thruster tool's click).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ThrusterCentre {
    /// The body.
    pub i: Target,
}

impl Operation for ThrusterCentre {
    const META: OpMeta = OpMeta {
        id: "body.thruster_centre",
        lisp: "thruster-centre",
        doc: "Attach a thruster at the i-th body's centre pushing along its local up with the default thrust — undoable. Returns its handle.",
        seam: Seam::Edit,
        menu: Some("Context/Thruster at centre"),
        chord: None,
        undo: Some(name::SPAWN_NODE),
        example: r#"{"i": 0}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(body) = body_of(ctx, scene, &self.i)? else {
            return Ok(OpOutput::none().returning(""));
        };
        let id = ctx.new_id();
        let record = NodeRecord {
            id,
            pose: PosRot {
                pos: body.pos,
                rot: 0.0,
            },
            attachment: NodeAttachment::to(body.id, Vec2::ZERO),
            kind: NodeKind::Thruster(gradiance_domain::thruster::Thruster::default()),
            appearance: Appearance::for_id(id),
        };
        Ok(OpOutput::none()
            .message(SpawnNodeIntent { record })
            .returning(id.0.to_string()))
    }
}

/// The circle of the same area as `shape`, centred on the body's origin;
/// `None` for a circle already, a ground, or an empty shape.
fn circle_of(shape: &ShapeDef) -> Option<ShapeDef> {
    if matches!(shape, ShapeDef::Circle { .. }) || shape.contains_half_plane() {
        return None;
    }
    let area: f32 = polygonize(shape).area();
    (area > 0.0).then(|| ShapeDef::Circle {
        radius: (area / std::f32::consts::PI).sqrt(),
    })
}

/// Turn the body `i` (`"selection"`: every selected body) into a circle of
/// the same area, centred on its origin: one undo step.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ToCircle {
    /// The body.
    pub i: Target,
}

impl Operation for ToCircle {
    const META: OpMeta = OpMeta {
        id: "body.to_circle",
        lisp: "to-circle",
        doc: "Turn the i-th body into a circle of the same area, centred on its origin (circles and grounds stay) — undoable.",
        seam: Seam::Edit,
        menu: Some("Context/To circle"),
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let changes: Vec<PropertyChange> = bodies_of(ctx, scene, &self.i)?
            .into_iter()
            .filter_map(|id| scene.get(id))
            .filter_map(|body| {
                Some(PropertyChange {
                    id: body.id,
                    old: PropertyValue::Shape(body.shape.clone()),
                    new: PropertyValue::Shape(circle_of(&body.shape)?),
                })
            })
            .collect();
        if changes.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(PropertyEditIntent { changes }))
    }
}

/// Brake the hinge `i`: its motor on, driving at 0 rad/s, so it holds the
/// joint still with the motor's torque cap (a hinge with no motor gets the
/// default one, auto cap). A slider or any other joint is left alone.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Brake {
    /// The joint: an index in id order, an id, or `"selection"`.
    pub i: Target,
}

impl Operation for Brake {
    const META: OpMeta = OpMeta {
        id: "joint.brake",
        lisp: "brake",
        doc: "Brake the i-th joint, a hinge: motor on at 0 rad/s with its torque cap kept — undoable.",
        seam: Seam::Edit,
        menu: Some("Context/Brake motor"),
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(id) = joint_of(ctx, scene, &self.i)? else {
            return Ok(OpOutput::none());
        };
        let Some((_, def)) = scene.joint_defs.iter().find(|(j, _)| *j == id) else {
            return Ok(OpOutput::none());
        };
        let mut new = def.clone();
        let JointKind::Hinge { motor, .. } = &mut new.kind else {
            return Ok(OpOutput::none());
        };
        let braked = AngularMotorDef {
            target_velocity: AngularVelocity(0.0),
            oscillate: false,
            enabled: true,
            ..motor.unwrap_or_default()
        };
        if *motor == Some(braked) {
            return Ok(OpOutput::none());
        }
        *motor = Some(braked);
        Ok(OpOutput::none().message(PropertyEditIntent {
            changes: vec![PropertyChange {
                id,
                old: PropertyValue::Joint(def.clone()),
                new: PropertyValue::Joint(new),
            }],
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_circle_keeps_the_area() {
        let shapes = [
            ShapeDef::Box {
                width: 2.0,
                height: 0.5,
            },
            ShapeDef::Polygon {
                outline: gradiance_geometry::gear::gear_outline(0.6, 12),
                holes: Vec::new(),
            },
        ];
        for shape in shapes {
            let before = polygonize(&shape).area();
            let circle = circle_of(&shape).unwrap();
            let ShapeDef::Circle { radius } = circle else {
                panic!("a circle");
            };
            let after = std::f32::consts::PI * radius * radius;
            assert!(
                (after - before).abs() <= 0.005 * before,
                "{before} vs {after}"
            );
        }
        assert_eq!(circle_of(&ShapeDef::Circle { radius: 1.0 }), None);
        assert_eq!(circle_of(&ShapeDef::HalfPlane), None);
    }
}
