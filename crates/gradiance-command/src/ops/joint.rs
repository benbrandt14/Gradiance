//! Joint operations: **relationships** between bodies.
//!
//! A joint's anchors are authored in each body's *local* space and its
//! `rest_rot_*` capture the bodies' orientations at creation (welds hold that
//! relative angle, sliders lock rotation to it, hinge limits measure from it).
//! Getting that wrong makes joints between rotated bodies snap violently at
//! spawn, so these operations do exactly what
//! `interaction::tools::connector_tool` does; `joint_record` is the one
//! place the conversion lives on this side.
//!
//! `b` that names no body (a negative index, say) pins `a` to the world: the
//! one-body case the tools produce by clicking where only one body sits. An
//! unresolved `b` degrades to a pin rather than dropping the joint, so a typo
//! is visible in the scene instead of silent. An `a` that names no body does
//! nothing and returns an empty handle.

use super::{body_of, joint_of};
use crate::intent::name;
use crate::intent::{DeleteJointIntent, SpawnJointIntent};
use crate::snapshot::{BodySnapshot, SceneSnapshot};
use bevy::prelude::*;
use gradiance_core::units::PosRot;
use gradiance_domain::joint::{JointCommon, JointDef, JointKind};
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_scene::JointRecord;
use gradiance_units::{Angle, Displacement, Frequency, Length};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Hinge>();
    registry.register::<Slider>();
    registry.register::<Spring>();
    registry.register::<DeleteJoint>();
}

/// The joint between `a` and (if it is another body) `b`, anchored at the
/// **world** points `[on_a, on_b]` (the same point for a hinge or a slider;
/// each body's centre for a spring), converted into each body's local frame.
/// A world pin's far anchor is `on_a`.
fn joint_record(
    ctx: &OpContext,
    a: &BodySnapshot,
    b: Option<&BodySnapshot>,
    [on_a, on_b]: [Vec2; 2],
    kind: JointKind,
    collide_connected: bool,
) -> JointRecord {
    let to_local =
        |pose: PosRot, anchor: Vec2| Vec2::from_angle(-pose.rot).rotate(anchor - pose.pos);
    let mut def = JointDef {
        kind,
        common: JointCommon {
            collide_connected,
            ..JointCommon::default()
        },
        body_a: a.id,
        body_b: None,
        // For a world pin the far anchor is the world point itself.
        anchor_b: Displacement(on_a),
        anchor_a: Displacement(to_local(a.pose(), on_a)),
        rest_rot_a: Angle(a.rot),
        rest_rot_b: Angle(0.0),
    };
    if let Some(b) = b.filter(|b| b.id != a.id) {
        def.body_b = Some(b.id);
        def.anchor_b = Displacement(to_local(b.pose(), on_b));
        def.rest_rot_b = Angle(b.rot);
    }
    JointRecord {
        id: ctx.new_id(),
        def,
    }
}

/// The output of every joint spawn: the intent plus the joint's handle.
fn spawn(record: JointRecord) -> OpOutput {
    let handle = record.id.0.to_string();
    OpOutput::none()
        .message(SpawnJointIntent { record })
        .returning(handle)
}

/// What a joint spawn returns when `a` names no body: no write, an empty
/// handle (what the hand-written verbs returned).
fn no_joint() -> OpOutput {
    OpOutput::none().returning("")
}

/// The two ends of a joint, from the context's snapshot.
fn ends<'s>(
    ctx: &OpContext,
    scene: &'s SceneSnapshot,
    a: &Target,
    b: &Target,
) -> Result<Option<(&'s BodySnapshot, Option<&'s BodySnapshot>)>, OpError> {
    let Some(first) = body_of(ctx, scene, a)? else {
        return Ok(None);
    };
    Ok(Some((first, body_of(ctx, scene, b)?)))
}

/// Hinge (revolute joint) the bodies `a` and `b` at the world point
/// (`x`, `y`).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Hinge {
    /// The first body.
    pub a: Target,
    /// The second body, or none for a world pin.
    pub b: Target,
    /// Anchor x (world).
    pub x: Length,
    /// Anchor y (world).
    pub y: Length,
}

impl Operation for Hinge {
    const META: OpMeta = OpMeta {
        id: "joint.hinge",
        lisp: "hinge",
        doc: "Hinge bodies a and b at world point (x, y); b < 0 pins a to the world.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_JOINT),
        example: r#"{"a": 0, "b": 1, "x": -0.2, "y": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some((a, b)) = ends(ctx, scene, &self.a, &self.b)? else {
            return Ok(no_joint());
        };
        let kind = JointKind::Hinge {
            limits: None,
            motor: None,
        };
        let anchor = Vec2::new(self.x.0, self.y.0);
        Ok(spawn(joint_record(
            ctx,
            a,
            b,
            [anchor, anchor],
            kind,
            false,
        )))
    }
}

/// A prismatic joint at (`x`, `y`) sliding along the world axis
/// (`ax`, `ay`). The axis is stored body-`a` local, so it rotates with the
/// body. A zero-length axis has no direction; it falls back to body `a`'s
/// local x, which is what a too-short tool drag does.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Slider {
    /// The first body.
    pub a: Target,
    /// The second body, or none for a world pin.
    pub b: Target,
    /// Anchor x (world).
    pub x: Length,
    /// Anchor y (world).
    pub y: Length,
    /// Axis x (world, any length).
    pub ax: f32,
    /// Axis y (world, any length).
    pub ay: f32,
}

impl Operation for Slider {
    const META: OpMeta = OpMeta {
        id: "joint.slider",
        lisp: "slider",
        doc: "Prismatic joint at (x, y) sliding along world axis (ax, ay); b < 0 pins to the world.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_JOINT),
        example: r#"{"a": 0, "b": 1, "x": -0.2, "y": 1, "ax": 1, "ay": 0}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some((a, b)) = ends(ctx, scene, &self.a, &self.b)? else {
            return Ok(no_joint());
        };
        let world_axis = Vec2::new(self.ax, self.ay);
        let world_axis = if world_axis.length() < 1e-6 {
            Vec2::from_angle(a.rot)
        } else {
            world_axis.normalize()
        };
        let kind = JointKind::Slider {
            axis: Vec2::from_angle(-a.rot).rotate(world_axis),
            limits: None,
            motor: None,
        };
        let anchor = Vec2::new(self.x.0, self.y.0);
        Ok(spawn(joint_record(
            ctx,
            a,
            b,
            [anchor, anchor],
            kind,
            false,
        )))
    }
}

/// A spring-damper strut between the centres of `a` and `b`, authored like
/// the tool's: a natural frequency and a damping ratio. The stiffness is
/// derived from the connected mass at run time, so the same call bounces
/// with the same period whatever the bodies weigh (brief 4b-4). The rest
/// length is the current separation, so the strut starts relaxed.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Spring {
    /// The first body.
    pub a: Target,
    /// The second body, or none for a world pin.
    pub b: Target,
    /// Natural frequency of the bounce (Hz; the tool's default is 1.5).
    pub frequency: Frequency,
    /// Damping ratio: 0 = undamped, 1 = critical (the tool's default is 0.1).
    pub damping_ratio: f32,
}

impl Operation for Spring {
    const META: OpMeta = OpMeta {
        id: "joint.spring",
        lisp: "spring",
        doc: "Spring-damper strut between the centres of a and b; rest length is their current distance.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_JOINT),
        example: r#"{"a": 0, "b": 2, "frequency": 1.5, "damping_ratio": 0.1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some((a, b)) = ends(ctx, scene, &self.a, &self.b)? else {
            return Ok(no_joint());
        };
        let rest_length = b.map_or(0.0, |b| a.pos.distance(b.pos));
        let kind = JointKind::Spring {
            rest_length: Length(rest_length),
            frequency: self.frequency,
            damping_ratio: self.damping_ratio,
            range: None,
        };
        // From a's centre to b's centre (a world pin: to a's centre). Struts
        // collide by default (the tool's convention), so a spring between
        // neighbours does not make them pass through each other.
        let on_b = b.map_or(a.pos, |b| b.pos);
        Ok(spawn(joint_record(ctx, a, b, [a.pos, on_b], kind, true)))
    }
}

/// Remove the joint `i` (`"selection"`: the selected joint).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct DeleteJoint {
    /// The joint: an index in id order, an id, or `"selection"`.
    pub i: Target,
}

impl Operation for DeleteJoint {
    const META: OpMeta = OpMeta {
        id: "joint.delete",
        lisp: "delete-joint",
        doc: "Remove the i-th joint (id order, as joint-count) — undoable.",
        seam: Seam::Edit,
        menu: Some("Context/Delete joint"),
        chord: Some("Delete"),
        undo: Some(name::DELETE_JOINT),
        example: r#"{"i": 0}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        Ok(
            joint_of(ctx, scene, &self.i)?.map_or_else(OpOutput::none, |id| {
                OpOutput::none().message(DeleteJointIntent { id })
            }),
        )
    }
}
