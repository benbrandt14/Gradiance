//! Operations on **existing bodies**: their pose, their size, their removal
//! and the inspector's physics fields. Each names its body with a [`Target`]
//! resolved against the context's snapshot, which also holds the old value an
//! undoable intent needs.

use super::{bodies_of, body_of};
use crate::intent::TransformChange;
use crate::intent::name;
use crate::intent::{CommitTransformIntent, DeleteIntent, PropertyEditIntent, ScaleIntent};
use crate::property::{PropertyChange, PropertyValue};
use crate::snapshot::SceneSnapshot;
use avian2d::prelude::{ColliderDensity, Friction, Restitution, RigidBody};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::props::BodyPhysics;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_units::{Angle, Length};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Place>();
    registry.register::<Delete>();
    registry.register::<Scale>();
    registry.register::<SetFriction>();
    registry.register::<SetRestitution>();
    registry.register::<SetDensity>();
    registry.register::<SetStatic>();
}

/// Move the body `i` to (`x`, `y`) and rotate it to `angle`: one undo step.
/// A target that names no body, or a move to where the body already is, does
/// nothing (no empty undo step).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Place {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
    /// New centre x.
    pub x: Length,
    /// New centre y.
    pub y: Length,
    /// New rotation.
    pub angle: Angle,
}

impl Operation for Place {
    const META: OpMeta = OpMeta {
        id: "body.place",
        lisp: "place",
        doc: "Move the i-th body to (x, y) and rotate it to `angle` radians — one undo step.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::COMMIT_TRANSFORM),
        example: r#"{"i": 0, "x": 1.5, "y": 0.25, "angle": 0.5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(body) = body_of(ctx, scene, &self.i)? else {
            return Ok(OpOutput::none());
        };
        let old = body.pose();
        let new = PosRot {
            pos: Vec2::new(self.x.0, self.y.0),
            rot: self.angle.0,
        };
        if new == old {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(CommitTransformIntent {
            changes: vec![TransformChange {
                id: body.id,
                old,
                new,
            }],
        }))
    }
}

/// Delete the body `i` (`"selection"`: every selected body and node). An id
/// may also name a behaviour node. A target that names nothing does nothing.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    /// The body.
    pub i: Target,
}

impl Operation for Delete {
    const META: OpMeta = OpMeta {
        id: "body.delete",
        lisp: "delete",
        doc: "Delete the i-th body (id order, same index as body-x) — undoable.",
        seam: Seam::Edit,
        menu: Some("Edit/Delete selection"),
        chord: Some("Delete"),
        undo: Some(name::DELETE),
        example: r#"{"i": 0}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let known = |id: &StableId| scene.get(*id).is_some() || scene.nodes.contains(id);
        let targets = match self.i {
            Target::Selection => ctx
                .selection()?
                .objects
                .iter()
                .copied()
                .filter(known)
                .collect(),
            Target::Id(id) if scene.nodes.contains(&id) => vec![id],
            other => bodies_of(ctx, scene, &other)?,
        };
        if targets.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(DeleteIntent { targets }))
    }
}

/// Resize the body `i` by (`fx`, `fy`) about its own centre, along its own
/// axes. Local axes rather than global: "twice as wide" means along the
/// body's width, and a global-axis scale of a rotated box would shear it into
/// a polygon. A factor that is not positive does nothing: zero collapses the
/// body for good and a negative one mirrors it, and neither is a resize.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Scale {
    /// The body.
    pub i: Target,
    /// Factor along the body's own x axis.
    pub fx: f32,
    /// Factor along the body's own y axis.
    pub fy: f32,
}

impl Operation for Scale {
    const META: OpMeta = OpMeta {
        id: "body.scale",
        lisp: "scale",
        doc: "Resize the i-th body by (fx, fy) about its own centre, along its own axes.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SCALE),
        example: r#"{"i": 0, "fx": 2, "fy": 1.5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let positive = self.fx > 0.0 && self.fy > 0.0;
        let Some(body) = body_of(ctx, scene, &self.i)?.filter(|_| positive) else {
            return Ok(OpOutput::none());
        };
        Ok(OpOutput::none().message(ScaleIntent {
            targets: vec![body.id],
            pivot: body.pos,
            frame_rot: body.rot,
            factors: Vec2::new(self.fx, self.fy),
        }))
    }
}

/// One property edit over the bodies `target` names: `build` turns a body's
/// physics into the old and new value. A body already at the new value is
/// left out, so a no-op is no undo step (the guard `place` has).
fn property_edit(
    ctx: &OpContext,
    target: &Target,
    build: impl Fn(&BodyPhysics) -> (PropertyValue, PropertyValue),
) -> Result<OpOutput, OpError> {
    let scene = ctx.get::<SceneSnapshot>()?;
    let changes: Vec<PropertyChange> = bodies_of(ctx, scene, target)?
        .into_iter()
        .filter_map(|id| {
            let (old, new) = build(&scene.get(id)?.physics);
            (old != new).then_some(PropertyChange { id, old, new })
        })
        .collect();
    if changes.is_empty() {
        return Ok(OpOutput::none());
    }
    Ok(OpOutput::none().message(PropertyEditIntent { changes }))
}

/// Set the body's Coulomb friction. Both coefficients move together, as the
/// inspector does: authoring two numbers that are almost always equal is
/// friction the UI deliberately does not expose.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetFriction {
    /// The body.
    pub i: Target,
    /// The coefficient.
    pub v: f32,
}

impl Operation for SetFriction {
    const META: OpMeta = OpMeta {
        id: "body.set_friction",
        lisp: "set-friction",
        doc: "Set the i-th body's Coulomb friction (static and dynamic) — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "v": 0.9}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        property_edit(ctx, &self.i, |physics| {
            let old = PropertyValue::Friction(physics.friction);
            let new = PropertyValue::Friction(Friction {
                dynamic_coefficient: self.v,
                static_coefficient: self.v,
                ..physics.friction
            });
            (old, new)
        })
    }
}

/// Set the body's bounciness: 0 is dead, 1 perfectly elastic.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetRestitution {
    /// The body.
    pub i: Target,
    /// The coefficient.
    pub v: f32,
}

impl Operation for SetRestitution {
    const META: OpMeta = OpMeta {
        id: "body.set_restitution",
        lisp: "set-restitution",
        doc: "Set the i-th body's bounciness, 0 = dead, 1 = perfectly elastic — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "v": 0.8}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        property_edit(ctx, &self.i, |physics| {
            let old = PropertyValue::Restitution(physics.restitution);
            let new = PropertyValue::Restitution(Restitution {
                coefficient: self.v,
                ..physics.restitution
            });
            (old, new)
        })
    }
}

/// Set the body's mass density (area x density = mass).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetDensity {
    /// The body.
    pub i: Target,
    /// The density.
    pub v: f32,
}

impl Operation for SetDensity {
    const META: OpMeta = OpMeta {
        id: "body.set_density",
        lisp: "set-density",
        doc: "Set the i-th body's mass density (area x density = mass) — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "v": 3}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        property_edit(ctx, &self.i, |physics| {
            let old = PropertyValue::Density(physics.density);
            let new = PropertyValue::Density(ColliderDensity(self.v));
            (old, new)
        })
    }
}

/// Make the body static when `on` is non-zero, dynamic otherwise: a toggle
/// with an explicit argument rather than two verbs that can disagree.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetStatic {
    /// The body.
    pub i: Target,
    /// Non-zero for static.
    pub on: f64,
}

impl Operation for SetStatic {
    const META: OpMeta = OpMeta {
        id: "body.set_static",
        lisp: "set-static",
        doc: "Make the i-th body static when `on` is non-zero, dynamic otherwise — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "on": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let kind = if self.on == 0.0 {
            RigidBody::Dynamic
        } else {
            RigidBody::Static
        };
        property_edit(ctx, &self.i, |physics| {
            (
                PropertyValue::RigidBody(physics.rigid_body),
                PropertyValue::RigidBody(kind),
            )
        })
    }
}
