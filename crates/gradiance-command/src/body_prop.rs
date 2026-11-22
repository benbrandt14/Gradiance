//! The **body property table**: one row per scalar body property a group
//! edit, a distribute, a filter or a panel can name (brief 4b-1).
//!
//! Every surface that names a body property by string reads this one
//! table: the `group-set`/`group-distribute`/`group-scatter` verbs, the
//! `bodies-where` range clause, the remote surface (through the same
//! operations) and the 4b-2 Groups panel. A property is a name, a read off
//! the committed scene ([`BodySnapshot`]) and an edit that turns a number
//! into the [`PropertyValue`] pair a [`PropertyChange`](crate::property::PropertyChange)
//! carries. Adding one is one variant and one row.

use crate::property::PropertyValue;
use crate::snapshot::BodySnapshot;
use avian2d::prelude::{ColliderDensity, Friction, GravityScale, Restitution, RigidBody};
use gradiance_domain::appearance::{Appearance, palette_fill, palette_slot};
use gradiance_domain::depth::DepthBand;
use gradiance_units::Length;

/// A scalar body property, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyProp {
    /// Coulomb friction (both coefficients, as the inspector edits them).
    Friction,
    /// Bounciness.
    Restitution,
    /// Mass density.
    Density,
    /// Per-body gravity multiplier.
    GravityScale,
    /// 1 when static, 0 when dynamic.
    Static,
    /// 1 when an overlap sensor.
    Sensor,
    /// 1 when rotation is locked.
    RotationLock,
    /// The depth band's near face in metres; a write keeps the thickness.
    Depth,
    /// The fill's hue in degrees; a write keeps saturation and lightness.
    Hue,
    /// The fill's palette slot (NaN when the fill is not a palette colour);
    /// a write rounds and wraps into the palette.
    Palette,
    /// Emissive strength: 0 is matte, higher glows in the fill's hue.
    Emissive,
    /// 1 when the body shows its velocity arrow (brief S-2b).
    ArrowVelocity,
    /// 1 when the body shows its momentum arrow.
    ArrowMomentum,
    /// 1 when the body shows its net-force arrow.
    ArrowForce,
}

/// One row of [`BODY_PROPS`].
#[derive(Debug, Clone, Copy)]
pub struct BodyPropRow {
    /// The name every surface uses.
    pub name: &'static str,
    /// The property.
    pub prop: BodyProp,
    /// One line for the catalog and the panel's tooltip.
    pub doc: &'static str,
}

/// The table: the names, in the order a panel lists them.
pub const BODY_PROPS: &[BodyPropRow] = &[
    BodyPropRow {
        name: "friction",
        prop: BodyProp::Friction,
        doc: "Coulomb friction (static and dynamic together).",
    },
    BodyPropRow {
        name: "restitution",
        prop: BodyProp::Restitution,
        doc: "Bounciness: 0 is dead, 1 perfectly elastic.",
    },
    BodyPropRow {
        name: "density",
        prop: BodyProp::Density,
        doc: "Mass density (area x density = mass).",
    },
    BodyPropRow {
        name: "gravity-scale",
        prop: BodyProp::GravityScale,
        doc: "Per-body gravity multiplier.",
    },
    BodyPropRow {
        name: "static",
        prop: BodyProp::Static,
        doc: "1 = static, 0 = dynamic (a write rounds: >= 0.5 is static).",
    },
    BodyPropRow {
        name: "sensor",
        prop: BodyProp::Sensor,
        doc: "1 = overlap sensor (no contact response).",
    },
    BodyPropRow {
        name: "rotation-lock",
        prop: BodyProp::RotationLock,
        doc: "1 = rotation locked.",
    },
    BodyPropRow {
        name: "depth",
        prop: BodyProp::Depth,
        doc: "Near face of the depth band in metres; the thickness is kept.",
    },
    BodyPropRow {
        name: "hue",
        prop: BodyProp::Hue,
        doc: "Fill hue in degrees; saturation and lightness are kept.",
    },
    BodyPropRow {
        name: "palette",
        prop: BodyProp::Palette,
        doc: "Fill palette slot; a write rounds and wraps into the palette.",
    },
    BodyPropRow {
        name: "emissive",
        prop: BodyProp::Emissive,
        doc: "Glow strength in the fill's hue: 0 is matte (a write clamps at 0).",
    },
    BodyPropRow {
        name: "arrow-velocity",
        prop: BodyProp::ArrowVelocity,
        doc: "1 = draw the body's velocity arrow while it plays.",
    },
    BodyPropRow {
        name: "arrow-momentum",
        prop: BodyProp::ArrowMomentum,
        doc: "1 = draw the body's momentum arrow (mass times velocity).",
    },
    BodyPropRow {
        name: "arrow-force",
        prop: BodyProp::ArrowForce,
        doc: "1 = draw the net-force arrow (mass times acceleration).",
    },
];

/// Whether a scalar is "on" for a flag property.
fn flag(v: f64) -> bool {
    v >= 0.5
}

impl BodyProp {
    /// The property called `name`, if the table has it.
    pub fn named(name: &str) -> Option<Self> {
        BODY_PROPS.iter().find(|r| r.name == name).map(|r| r.prop)
    }

    /// The table's names, comma-separated (for error messages).
    pub fn names() -> String {
        BODY_PROPS
            .iter()
            .map(|r| r.name)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The property's name.
    pub fn name(self) -> &'static str {
        BODY_PROPS
            .iter()
            .find(|r| r.prop == self)
            .map_or("", |r| r.name)
    }

    /// Whether the property is an on/off flag (read as 0 or 1, written
    /// rounded), so a panel offers a checkbox rather than a number.
    pub fn is_flag(self) -> bool {
        matches!(self, Self::Static | Self::Sensor | Self::RotationLock)
    }

    /// The property's value on `body`.
    pub fn read(self, body: &BodySnapshot) -> f64 {
        let p = &body.physics;
        let bit = |on: bool| if on { 1.0 } else { 0.0 };
        match self {
            Self::Friction => f64::from(p.friction.dynamic_coefficient),
            Self::Restitution => f64::from(p.restitution.coefficient),
            Self::Density => f64::from(p.density.0),
            Self::GravityScale => f64::from(p.gravity_scale.0),
            Self::Static => bit(p.rigid_body == RigidBody::Static),
            Self::Sensor => bit(p.sensor),
            Self::RotationLock => bit(p.rotation_locked),
            Self::Depth => f64::from(body.depth.near.0),
            Self::Hue => f64::from(body.appearance.fill.to_hsl().0),
            Self::Palette => palette_slot(body.appearance.fill).map_or(f64::NAN, |s| s as f64),
            Self::Emissive => f64::from(body.appearance.emissive),
            Self::ArrowVelocity => bit(body.appearance.arrows.velocity),
            Self::ArrowMomentum => bit(body.appearance.arrows.momentum),
            Self::ArrowForce => bit(body.appearance.arrows.force),
        }
    }

    /// The `(old, new)` values that set this property to `v` on `body`.
    /// Equal when the body already holds `v` (the caller drops the change).
    pub fn edit(self, body: &BodySnapshot, v: f64) -> (PropertyValue, PropertyValue) {
        let p = &body.physics;
        let f = v as f32;
        match self {
            Self::Friction => (
                PropertyValue::Friction(p.friction),
                PropertyValue::Friction(Friction {
                    dynamic_coefficient: f,
                    static_coefficient: f,
                    ..p.friction
                }),
            ),
            Self::Restitution => (
                PropertyValue::Restitution(p.restitution),
                PropertyValue::Restitution(Restitution {
                    coefficient: f,
                    ..p.restitution
                }),
            ),
            Self::Density => (
                PropertyValue::Density(p.density),
                PropertyValue::Density(ColliderDensity(f)),
            ),
            Self::GravityScale => (
                PropertyValue::GravityScale(p.gravity_scale),
                PropertyValue::GravityScale(GravityScale(f)),
            ),
            Self::Static => (
                PropertyValue::RigidBody(p.rigid_body),
                PropertyValue::RigidBody(if flag(v) {
                    RigidBody::Static
                } else {
                    RigidBody::Dynamic
                }),
            ),
            Self::Sensor => (
                PropertyValue::Sensor(p.sensor),
                PropertyValue::Sensor(flag(v)),
            ),
            Self::RotationLock => (
                PropertyValue::RotationLock(p.rotation_locked),
                PropertyValue::RotationLock(flag(v)),
            ),
            Self::Depth => {
                let thickness = body.depth.thickness();
                (
                    PropertyValue::Depth(body.depth),
                    PropertyValue::Depth(
                        DepthBand {
                            near: Length(f),
                            far: Length(f + thickness),
                        }
                        .sanitized(),
                    ),
                )
            }
            Self::Hue => colour(body, |fill| fill.with_hue(f)),
            Self::Palette => colour(body, |fill| palette_fill(v, fill)),
            Self::Emissive => {
                let old = body.appearance;
                (
                    PropertyValue::Appearance(old),
                    PropertyValue::Appearance(Appearance {
                        emissive: f.max(0.0),
                        ..old
                    }),
                )
            }
            Self::ArrowVelocity | Self::ArrowMomentum | Self::ArrowForce => {
                let old = body.appearance;
                let mut arrows = old.arrows;
                let slot = match self {
                    Self::ArrowVelocity => &mut arrows.velocity,
                    Self::ArrowMomentum => &mut arrows.momentum,
                    _ => &mut arrows.force,
                };
                *slot = flag(v);
                (
                    PropertyValue::Appearance(old),
                    PropertyValue::Appearance(Appearance { arrows, ..old }),
                )
            }
        }
    }
}

/// A colour edit: `fill` maps the old fill to the new one.
fn colour(
    body: &BodySnapshot,
    fill: impl FnOnce(gradiance_domain::appearance::Rgba) -> gradiance_domain::appearance::Rgba,
) -> (PropertyValue, PropertyValue) {
    let old = body.appearance;
    let new = Appearance {
        fill: fill(old.fill),
        ..old
    };
    (
        PropertyValue::Appearance(old),
        PropertyValue::Appearance(new),
    )
}

#[cfg(test)]
mod tests {
    // float_cmp: values are written and read back unchanged.
    #![allow(clippy::float_cmp)]
    use super::*;
    use bevy::prelude::Vec2;
    use gradiance_core::ids::StableId;
    use gradiance_domain::props::BodyPhysics;
    use gradiance_domain::shape::ShapeDef;

    fn body() -> BodySnapshot {
        BodySnapshot {
            id: StableId::new(),
            physics: BodyPhysics::default(),
            pos: Vec2::ZERO,
            rot: 0.0,
            shape: ShapeDef::Circle { radius: 0.5 },
            touching: 0,
            appearance: Appearance::default(),
            depth: DepthBand::default(),
            groups: Vec::new(),
            fluid: None,
            motion: None,
        }
    }

    #[test]
    fn every_row_is_named_once_and_an_edit_reads_back() {
        for row in BODY_PROPS {
            assert_eq!(BodyProp::named(row.name), Some(row.prop));
            assert_eq!(row.prop.name(), row.name);
        }
        assert_eq!(BodyProp::named("colour"), None);
        let mut b = body();
        let (_, new) = BodyProp::Density.edit(&b, 3.0);
        assert_eq!(new, PropertyValue::Density(ColliderDensity(3.0)));
        let (_, new) = BodyProp::Palette.edit(&b, 4.0);
        if let PropertyValue::Appearance(a) = new {
            b.appearance = a;
        }
        assert_eq!(BodyProp::Palette.read(&b), 4.0);
        let (old, new) = BodyProp::Palette.edit(&b, 4.0);
        assert_eq!(old, new, "a no-op edit is equal");
    }
}
