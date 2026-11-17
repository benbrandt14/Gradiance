//! Operations on **groups as editing layers** (brief 4b-1): list and name
//! groups, read their members, set or spread one property over every
//! member in one undo step, filter bodies, and split a group by a key.
//!
//! A group is named by a [`Target`]: an index names the group at that
//! position in id order (`(groups)` lists them in that order); a body (its
//! id, a point on it, or `"selection"`, the primary selected body) names the
//! outermost group that body is in. Groups are authored scene state, not
//! selection, so group-indexed verbs keep scripting rule 2.
//!
//! Property names come from the one table every surface shares,
//! [`BODY_PROPS`](crate::body_prop::BODY_PROPS).

use super::body_of;
use crate::body_prop::BodyProp;
use crate::filter::{DecomposeKey, matching, parse_clause};
use crate::intent::name;
use crate::intent::{NameGroupIntent, PropertyEditIntent, RegroupIntent};
use crate::property::{PropertyChange, PropertyValue};
use crate::snapshot::{BodySnapshot, SceneSnapshot};
use bevy::prelude::*;
use gradiance_domain::appearance::{Appearance, Rgba};
use gradiance_domain::distribute::{Spread, distribute};
use gradiance_domain::group::GroupEntry;
use gradiance_geometry::sdf;
use gradiance_op::{
    OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target, Value,
};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Groups>();
    registry.register::<GroupName>();
    registry.register::<GroupMembers>();
    registry.register::<NameGroup>();
    registry.register::<GroupSet>();
    registry.register::<GroupSetColor>();
    registry.register::<GroupDistribute>();
    registry.register::<GroupScatter>();
    registry.register::<BodiesWhere>();
    registry.register::<Decompose>();
}

/// The group `target` names (see the module docs), if any.
pub(crate) fn group_of(
    ctx: &OpContext,
    scene: &SceneSnapshot,
    target: &Target,
) -> Result<Option<GroupEntry>, OpError> {
    let index = scene.group_index();
    Ok(match target {
        Target::Index(i) => usize::try_from(*i)
            .ok()
            .and_then(|i| index.groups.get(i).cloned()),
        body => body_of(ctx, scene, body)?
            .and_then(|b| b.groups.last().copied())
            .and_then(|g| index.get(g).cloned()),
    })
}

/// The table property called `name`, or an error naming the table.
fn prop_named(name: &str) -> Result<BodyProp, OpError> {
    BodyProp::named(name).ok_or_else(|| {
        OpError::Invalid(format!(
            "`{name}` is not a body property ({})",
            BodyProp::names()
        ))
    })
}

/// One property edit over a group's members: `value` gives each member's
/// new value. Members already at their value are left out, so a no-op is
/// no undo step.
fn group_edit(
    scene: &SceneSnapshot,
    group: Option<GroupEntry>,
    build: impl Fn(usize, &BodySnapshot) -> (PropertyValue, PropertyValue),
) -> OpOutput {
    let Some(group) = group else {
        return OpOutput::none();
    };
    let changes: Vec<PropertyChange> = group
        .members
        .iter()
        .filter_map(|id| scene.get(*id))
        .enumerate()
        .filter_map(|(k, body)| {
            let (old, new) = build(k, body);
            (old != new).then_some(PropertyChange {
                id: body.id,
                old,
                new,
            })
        })
        .collect();
    if changes.is_empty() {
        return OpOutput::none();
    }
    OpOutput::none().message(PropertyEditIntent { changes })
}

/// The metadata every group query shares: no menu, no chord, no undo step.
const fn query(
    id: &'static str,
    lisp: &'static str,
    doc: &'static str,
    example: &'static str,
) -> OpMeta {
    OpMeta {
        id,
        lisp,
        doc,
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example,
    }
}

/// The metadata every group edit shares.
const fn edit(
    id: &'static str,
    lisp: &'static str,
    doc: &'static str,
    undo: &'static str,
    example: &'static str,
) -> OpMeta {
    OpMeta {
        id,
        lisp,
        doc,
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(undo),
        example,
    }
}

/// Every group's name, in group order ("" for an unnamed group): the list
/// a group index counts into, so `(length (groups))` is the group count.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Groups {}

impl Operation for Groups {
    const META: OpMeta = query(
        "group.list",
        "groups",
        "Every group's name in group order (\"\" when unnamed); a group index counts into this list.",
        "{}",
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let names: Vec<Value> = scene
            .group_index()
            .groups
            .iter()
            .map(|g| Value::from(scene.group_names.get(g.id).unwrap_or_default()))
            .collect();
        Ok(OpOutput::none().returning(names))
    }
}

/// The name of the group `group` names: "" when unnamed, NaN when there is
/// no such group.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GroupName {
    /// The group: an index in group order, or a body in it.
    pub group: Target,
}

impl Operation for GroupName {
    const META: OpMeta = query(
        "group.name",
        "group-name",
        "The group's name (\"\" when unnamed; NaN when no such group).",
        r#"{"group": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let value = group_of(ctx, scene, &self.group)?.map_or(Value::Null, |g| {
            Value::from(scene.group_names.get(g.id).unwrap_or_default())
        });
        Ok(OpOutput::none().returning(value))
    }
}

/// The body indices (id order) of every member of the group, at any
/// nesting depth; empty when there is no such group.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GroupMembers {
    /// The group.
    pub group: Target,
}

impl Operation for GroupMembers {
    const META: OpMeta = query(
        "group.members",
        "group-members",
        "The body indices of the group's members (any nesting depth), as a list.",
        r#"{"group": 0}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let members: Vec<Value> = group_of(ctx, scene, &self.group)?.map_or_else(Vec::new, |g| {
            g.members
                .iter()
                .filter_map(|id| scene.index_of(*id))
                .map(Value::from)
                .collect()
        });
        Ok(OpOutput::none().returning(members))
    }
}

/// Name the group (an empty name removes it): one undo step, saved with
/// the scene.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct NameGroup {
    /// The group.
    pub group: Target,
    /// The new name.
    pub name: String,
}

impl Operation for NameGroup {
    const META: OpMeta = edit(
        "group.rename",
        "name-group",
        "Name the group (\"\" removes the name) — undoable, saved with the scene.",
        name::NAME_GROUP,
        r#"{"group": 0, "name": "wheels"}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(group) = group_of(ctx, scene, &self.group)? else {
            return Ok(OpOutput::none());
        };
        if scene.group_names.get(group.id).unwrap_or_default() == self.name {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(NameGroupIntent {
            group: group.id,
            name: self.name,
        }))
    }
}

/// Set one table property on every member of the group: one undo step.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GroupSet {
    /// The group.
    pub group: Target,
    /// A property name from the body property table.
    pub prop: String,
    /// The value.
    pub v: f64,
}

impl Operation for GroupSet {
    const META: OpMeta = edit(
        "group.set",
        "group-set",
        "Set one body property (friction, density, hue, palette, depth, …) on every member — one undo step.",
        name::PROPERTY_EDIT,
        r#"{"group": 0, "prop": "friction", "v": 0.2}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let prop = prop_named(&self.prop)?;
        let scene = ctx.get::<SceneSnapshot>()?;
        let group = group_of(ctx, scene, &self.group)?;
        Ok(group_edit(scene, group, |_, body| prop.edit(body, self.v)))
    }
}

/// An opaque fill from RGB in `[0, 1]`, keeping the old alpha.
fn fill_of(r: f32, g: f32, b: f32, old: Appearance) -> Appearance {
    Appearance {
        fill: Rgba {
            a: old.fill.a,
            ..Rgba::rgb(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0))
        },
        ..old
    }
}

/// Set every member's fill colour: one undo step.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GroupSetColor {
    /// The group.
    pub group: Target,
    /// Red in `[0, 1]`.
    pub r: f32,
    /// Green in `[0, 1]`.
    pub g: f32,
    /// Blue in `[0, 1]`.
    pub b: f32,
}

impl Operation for GroupSetColor {
    const META: OpMeta = edit(
        "group.set_color",
        "group-set-color",
        "Set every member's fill to (r, g, b), each in 0..1 — one undo step.",
        name::PROPERTY_EDIT,
        r#"{"group": 0, "r": 0.9, "g": 0.3, "b": 0.1}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let group = group_of(ctx, scene, &self.group)?;
        Ok(group_edit(scene, group, |_, body| {
            let old = body.appearance;
            (
                PropertyValue::Appearance(old),
                PropertyValue::Appearance(fill_of(self.r, self.g, self.b, old)),
            )
        }))
    }
}

/// The sort key a linear distribute orders members by.
fn sort_key(by: &str, prop: BodyProp, body: &BodySnapshot, k: usize) -> Result<f64, OpError> {
    Ok(match by {
        "order" => k as f64,
        "x" => f64::from(body.pos.x),
        "y" => f64::from(body.pos.y),
        "size" => {
            let (lo, hi) = sdf::aabb(&body.shape);
            f64::from((hi - lo).length())
        }
        "value" => prop.read(body),
        other => {
            return Err(OpError::Invalid(format!(
                "`{other}` is not a sort key (order, x, y, size, value)"
            )));
        }
    })
}

/// Spread values over a group's members and emit the one edit.
fn spread(
    ctx: &OpContext,
    target: &Target,
    prop: &str,
    lo: f64,
    hi: f64,
    how: Spread,
    by: &str,
) -> Result<OpOutput, OpError> {
    let prop = prop_named(prop)?;
    let scene = ctx.get::<SceneSnapshot>()?;
    let group = group_of(ctx, scene, target)?;
    let bodies: Vec<&BodySnapshot> = group
        .iter()
        .flat_map(|g| g.members.iter())
        .filter_map(|id| scene.get(*id))
        .collect();
    let keys = bodies
        .iter()
        .enumerate()
        .map(|(k, body)| sort_key(by, prop, body, k))
        .collect::<Result<Vec<f64>, OpError>>()?;
    let values = distribute(&keys, lo, hi, how);
    Ok(group_edit(scene, group, |k, body| {
        prop.edit(body, values[k])
    }))
}

/// Spread one property over the group's members from `lo` to `hi`, in the
/// order of a sort key: one undo step. The first member in key order gets
/// `lo`, the last `hi`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GroupDistribute {
    /// The group.
    pub group: Target,
    /// A property name from the body property table.
    pub prop: String,
    /// The first member's value.
    pub lo: f64,
    /// The last member's value.
    pub hi: f64,
    /// The sort key: `order` (member order), `x`, `y`, `size`, or `value`
    /// (the property's current value).
    pub by: String,
}

impl Operation for GroupDistribute {
    const META: OpMeta = edit(
        "group.distribute",
        "group-distribute",
        "Spread a body property over the members from lo to hi, ordered by `by` (order, x, y, size, value) — one undo step.",
        name::PROPERTY_EDIT,
        r#"{"group": 0, "prop": "density", "lo": 0.5, "hi": 5, "by": "x"}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        spread(
            ctx,
            &self.group,
            &self.prop,
            self.lo,
            self.hi,
            Spread::Linear,
            &self.by,
        )
    }
}

/// Set one property on every member to a seeded random value in
/// `[lo, hi]`: one undo step, and the same seed gives the same values.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GroupScatter {
    /// The group.
    pub group: Target,
    /// A property name from the body property table.
    pub prop: String,
    /// The low bound.
    pub lo: f64,
    /// The high bound.
    pub hi: f64,
    /// The seed (a whole number).
    pub seed: f64,
}

impl Operation for GroupScatter {
    const META: OpMeta = edit(
        "group.scatter",
        "group-scatter",
        "Set a body property on every member to a seeded random value in lo..hi — one undo step.",
        name::PROPERTY_EDIT,
        r#"{"group": 0, "prop": "hue", "lo": 0, "hi": 300, "seed": 7}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let how = Spread::Random {
            seed: self.seed as u64,
        };
        spread(ctx, &self.group, &self.prop, self.lo, self.hi, how, "order")
    }
}

/// The indices (id order) of the bodies matching every clause: the filter
/// a selection, a new group or a group edit starts from.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BodiesWhere {
    /// The clauses (all must hold): `kind:circle`, `group:0`, `grouped`,
    /// `ungrouped`, `palette:3`, `friction:0.2..0.5`, `not:<clause>`.
    pub clauses: Vec<String>,
}

impl Operation for BodiesWhere {
    const META: OpMeta = query(
        "body.where",
        "bodies-where",
        "Indices of the bodies matching every clause (kind:circle, group:0, ungrouped, palette:3, friction:0.2..0.5, not:…).",
        r#"{"clauses": ["kind:circle", "grouped"]}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let groups = scene.group_index();
        let clauses = self
            .clauses
            .iter()
            .map(|c| parse_clause(c, &groups))
            .collect::<Result<Vec<_>, String>>()
            .map_err(OpError::Invalid)?;
        let found: Vec<Value> = matching(scene, &clauses)
            .into_iter()
            .map(Value::from)
            .collect();
        Ok(OpOutput::none().returning(found))
    }
}

/// Split the group by a key, in one undo step: `color` and `kind` make one
/// group per distinct value, `islands` one per joint-connected island, and
/// `dissolve` removes the group. A part of one body just leaves the group;
/// nesting around the group is kept.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Decompose {
    /// The group.
    pub group: Target,
    /// `color`, `kind`, `islands` or `dissolve`.
    pub key: String,
}

impl Operation for Decompose {
    const META: OpMeta = edit(
        "group.decompose",
        "decompose",
        "Split the group into one group per colour, kind or joint island, or dissolve it — one undo step.",
        name::REGROUP,
        r#"{"group": 0, "key": "dissolve"}"#,
    );

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let key = DecomposeKey::named(&self.key).ok_or_else(|| {
            OpError::Invalid(format!(
                "`{}` is not a decompose key (color, kind, islands, dissolve)",
                self.key
            ))
        })?;
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(group) = group_of(ctx, scene, &self.group)? else {
            return Ok(OpOutput::none());
        };
        let parts = crate::filter::decompose(scene, &group.members, key);
        if parts.len() == 1 && parts[0].len() == group.members.len() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(RegroupIntent {
            group: group.id,
            parts,
        }))
    }
}
