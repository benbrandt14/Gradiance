//! Operations over **several bodies at once**: the body booleans and the
//! groups. Each intent holds a set; `"selection"` names every selected body.

use super::{bodies_of, body_set};
use crate::intent::name;
use crate::intent::{BooleanIntent, GroupIntent, MergeIntent, UngroupIntent};
use crate::snapshot::SceneSnapshot;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::shape::CsgOp;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_units::Length;
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Merge>();
    registry.register::<FilletMerge>();
    registry.register::<Subtract>();
    registry.register::<Intersect>();
    registry.register::<Xor>();
    registry.register::<Group>();
    registry.register::<Ungroup>();
}

/// The bodies of a boolean, `a`'s first: `a` hosts the result and the rest
/// are consumed. Fewer than two distinct bodies is no boolean (a body with
/// itself would ask the command to fold one body onto itself), so the caller
/// does nothing. A target that names nothing is the quiet no-op every indexed
/// verb uses.
fn operands(ctx: &OpContext, a: &Target, b: &Target) -> Result<Option<Vec<StableId>>, OpError> {
    let scene = ctx.get::<SceneSnapshot>()?;
    let first = bodies_of(ctx, scene, a)?;
    let second = bodies_of(ctx, scene, b)?;
    if first.is_empty() || second.is_empty() {
        return Ok(None);
    }
    let mut targets = first;
    for id in second {
        if !targets.contains(&id) {
            targets.push(id);
        }
    }
    Ok((targets.len() >= 2).then_some(targets))
}

/// A boolean of the bodies `a` and `b` as a [`BooleanIntent`].
fn boolean(ctx: &OpContext, a: &Target, b: &Target, op: CsgOp) -> Result<OpOutput, OpError> {
    Ok(operands(ctx, a, b)?.map_or_else(OpOutput::none, |targets| {
        OpOutput::none().message(BooleanIntent { op, targets })
    }))
}

/// Fuse the bodies `a` and `b` into one CSG union; `a` survives carrying both
/// shapes. This is what "weld" means in this engine (there is no weld joint).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Merge {
    /// The body that hosts the result.
    pub a: Target,
    /// The body it absorbs.
    pub b: Target,
}

impl Operation for Merge {
    const META: OpMeta = OpMeta {
        id: "body.merge",
        lisp: "merge",
        doc: "Fuse bodies a and b into one CSG union; a survives.",
        seam: Seam::Edit,
        menu: Some("Context/Merge into one body"),
        chord: None,
        undo: Some(name::MERGE),
        example: r#"{"a": 0, "b": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        // Merge keeps its own intent (the weld tool emits it too).
        Ok(
            operands(ctx, &self.a, &self.b)?.map_or_else(OpOutput::none, |targets| {
                OpOutput::none().message(MergeIntent { targets })
            }),
        )
    }
}

/// Merge with a fillet: the boolean as a smooth union of `radius`. A radius
/// that is not a positive finite length is a visible error; the command
/// refuses one too, for every other surface.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct FilletMerge {
    /// The body that hosts the result.
    pub a: Target,
    /// The body it absorbs.
    pub b: Target,
    /// The blend radius.
    pub radius: Length,
}

impl Operation for FilletMerge {
    const META: OpMeta = OpMeta {
        id: "body.fillet_merge",
        lisp: "fillet-merge",
        doc: "Fuse bodies a and b into one with a smooth fillet of radius metres where they meet (CSG smooth union); a survives. Bridges a gap under radius/2. Errors unless radius > 0.",
        seam: Seam::Edit,
        menu: Some("Context/Merge with fillet"),
        chord: None,
        undo: Some(name::FILLET_MERGE),
        example: r#"{"a": 0, "b": 1, "radius": 0.25}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let op = CsgOp::SmoothUnion {
            radius: self.radius.0,
        };
        if op.validate().is_err() {
            return Err(OpError::Invalid(format!(
                "the radius must be a finite number of metres > 0, got {}",
                self.radius.0
            )));
        }
        boolean(ctx, &self.a, &self.b, op)
    }
}

/// Cut body `b`'s shape out of body `a`; `b` is consumed.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Subtract {
    /// The body that hosts the result.
    pub a: Target,
    /// The body cut away.
    pub b: Target,
}

impl Operation for Subtract {
    const META: OpMeta = OpMeta {
        id: "body.subtract",
        lisp: "subtract",
        doc: "Cut body b's shape out of body a (CSG subtract); b is consumed, a split result becomes pieces.",
        seam: Seam::Edit,
        menu: Some("Context/Subtract from first selected"),
        chord: None,
        undo: Some(name::SUBTRACT),
        example: r#"{"a": 0, "b": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        boolean(ctx, &self.a, &self.b, CsgOp::Subtract)
    }
}

/// Keep only where bodies `a` and `b` overlap, on `a`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Intersect {
    /// The body that hosts the result.
    pub a: Target,
    /// The other body.
    pub b: Target,
}

impl Operation for Intersect {
    const META: OpMeta = OpMeta {
        id: "body.intersect",
        lisp: "intersect",
        doc: "Keep only where bodies a and b overlap (CSG intersect) on a; b is consumed.",
        seam: Seam::Edit,
        menu: Some("Context/Intersect"),
        chord: None,
        undo: Some(name::INTERSECT),
        example: r#"{"a": 0, "b": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        boolean(ctx, &self.a, &self.b, CsgOp::Intersect)
    }
}

/// Keep what lies in exactly one of bodies `a` and `b`, on `a`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Xor {
    /// The body that hosts the result.
    pub a: Target,
    /// The other body.
    pub b: Target,
}

impl Operation for Xor {
    const META: OpMeta = OpMeta {
        id: "body.xor",
        lisp: "xor",
        doc: "Keep what lies in exactly one of bodies a and b (CSG xor) on a; b is consumed, a split result becomes pieces.",
        seam: Seam::Edit,
        menu: Some("Context/Xor"),
        chord: None,
        undo: Some(name::XOR),
        example: r#"{"a": 0, "b": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        boolean(ctx, &self.a, &self.b, CsgOp::Xor)
    }
}

/// Wrap the listed bodies in one fresh group, like Ctrl+G. Fewer than two
/// distinct bodies is a quiet no-op (Ctrl+G with one body selected emits
/// nothing, and the command would reject it as no effect). A listed target
/// that names no body is an error.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Group {
    /// The bodies: a list of targets (`(list i …)` in Lisp).
    pub targets: Vec<Target>,
}

impl Operation for Group {
    const META: OpMeta = OpMeta {
        id: "group.create",
        lisp: "group",
        doc: "Group the listed bodies (id order) like Ctrl+G; fewer than two distinct bodies is a no-op.",
        seam: Seam::Edit,
        menu: Some("Edit/Group"),
        chord: Some("Ctrl+G"),
        undo: Some(name::GROUP),
        example: r#"{"targets": [0, 1]}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let targets = body_set(ctx, scene, &self.targets)?;
        if targets.len() < 2 {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(GroupIntent { targets }))
    }
}

/// Peel the listed bodies' outermost group, like Ctrl+Shift+G; inner groups
/// survive. A listed target that names no body is an error.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Ungroup {
    /// The bodies: a list of targets (`(list i …)` in Lisp).
    pub targets: Vec<Target>,
}

impl Operation for Ungroup {
    const META: OpMeta = OpMeta {
        id: "group.ungroup",
        lisp: "ungroup",
        doc: "Peel the listed bodies' outermost group (id order) like Ctrl+Shift+G; inner groups survive.",
        seam: Seam::Edit,
        menu: Some("Edit/Ungroup"),
        chord: Some("Ctrl+Shift+G"),
        undo: Some(name::UNGROUP),
        example: r#"{"targets": [3]}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let targets = body_set(ctx, scene, &self.targets)?;
        if targets.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(UngroupIntent { targets }))
    }
}
