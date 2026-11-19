//! The **physical quantity catalog**: one table, keyed by a stable name, of
//! every quantity the app can read off a body, a pair of bodies or a joint
//! (units P3, `docs/units-decision.md`; brief 5-2).
//!
//! Each [`PhysicalQuantity`] row is a name, a short port label, a
//! [`Dimension`], a description and a [`Reader`]. The signal sources
//! ([`SignalSource`](crate::signal::SignalSource) variants name their row),
//! the inspector and node-graph sensor ports, the live plot's unit labels and
//! the script / remote `quantity` operations all read this table instead of
//! keeping their own lists: adding a quantity is one row.
//!
//! # Readers
//!
//! A reader is a plain `fn` over a read trait — [`BodyRead`] or
//! [`JointRead`] — so it is pure and testable against a known state
//! ([`BodySample`], [`JointSample`]). The physics layer implements the traits
//! over the live world (`gradiance_physics::queries::LiveBody`), which keeps
//! the frame-loop read one trait call per quantity: no allocation, no VM
//! (the two-tier rule). The trait is the shape a future **set aggregate**
//! (mean / sum over a group) wraps: it calls the same per-body reader over
//! each member and keeps the row's dimension.
//!
//! Values are `f32` in the base SI unit of the row's [`Dimension`].

use crate::joint::JointSensors;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_units::{
    AngularMomentum, AngularVelocity, Dimension, Energy, Force, Force2, Mass, Momentum, Velocity2,
};
use std::collections::HashMap;

/// What a [`PhysicalQuantity`] reads off a body. Every read is optional: a
/// body that does not simulate has no velocity or mass, one that has not been
/// placed yet has no position.
pub trait BodyRead {
    /// World position of the body's origin (metres).
    fn position(&self) -> Option<Vec2>;
    /// Linear and angular velocity.
    fn velocity(&self) -> Option<(Velocity2, AngularVelocity)>;
    /// The solver's mass.
    fn mass(&self) -> Option<Mass>;
    /// The solver's moment of inertia (kg·m²; no quantity type yet).
    fn angular_inertia(&self) -> Option<f32>;
    /// Net normal contact force over the last physics step.
    fn contact_force(&self) -> Option<Force2>;
    /// How many bodies it is touching.
    fn contact_count(&self) -> Option<usize>;
}

/// What a [`PhysicalQuantity`] reads off a joint.
pub trait JointRead {
    /// Length between the world anchors and the angle in the limit frame
    /// ([`JointDef::sensor_state`](crate::joint::JointDef::sensor_state)).
    fn sensors(&self) -> Option<JointSensors>;
    /// Magnitude of the linear reaction force after the last solve.
    fn reaction_force(&self) -> Option<Force>;
}

/// What a quantity is read off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    /// One body.
    Body,
    /// Two bodies.
    BodyPair,
    /// One joint.
    Joint,
}

impl Subject {
    /// A short noun for messages ("body", "pair of bodies", "joint").
    pub const fn noun(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::BodyPair => "pair of bodies",
            Self::Joint => "joint",
        }
    }
}

/// A row's reader, by subject. The value is in the row's base SI unit.
#[derive(Clone, Copy)]
pub enum Reader {
    /// Reads one body.
    Body(fn(&dyn BodyRead) -> Option<f32>),
    /// Reads two bodies.
    Pair(fn(&dyn BodyRead, &dyn BodyRead) -> Option<f32>),
    /// Reads one joint.
    Joint(fn(&dyn JointRead) -> Option<f32>),
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Body(_) => "Reader::Body",
            Self::Pair(_) => "Reader::Pair",
            Self::Joint(_) => "Reader::Joint",
        })
    }
}

/// One measurable quantity: a catalog row.
#[derive(Debug, Clone, Copy)]
pub struct PhysicalQuantity {
    /// The stable key (lower-case, `-`): what scripts and bindings name.
    pub name: &'static str,
    /// The short label a sensor port shows.
    pub label: &'static str,
    /// Its physical dimension (the unit label every surface shows).
    pub dimension: Dimension,
    /// One sentence on what it measures.
    pub doc: &'static str,
    /// How to read it.
    pub reader: Reader,
}

impl PhysicalQuantity {
    /// What it is read off.
    pub const fn subject(&self) -> Subject {
        match self.reader {
            Reader::Body(_) => Subject::Body,
            Reader::Pair(_) => Subject::BodyPair,
            Reader::Joint(_) => Subject::Joint,
        }
    }

    /// Reads it off one body (`None` for a pair or joint row).
    pub fn read_body(&self, body: &dyn BodyRead) -> Option<f32> {
        match self.reader {
            Reader::Body(read) => read(body),
            Reader::Pair(_) | Reader::Joint(_) => None,
        }
    }

    /// Reads it off two bodies (`None` for a body or joint row).
    pub fn read_pair(&self, a: &dyn BodyRead, b: &dyn BodyRead) -> Option<f32> {
        match self.reader {
            Reader::Pair(read) => read(a, b),
            Reader::Body(_) | Reader::Joint(_) => None,
        }
    }

    /// Reads it off one joint (`None` for a body or pair row).
    pub fn read_joint(&self, joint: &dyn JointRead) -> Option<f32> {
        match self.reader {
            Reader::Joint(read) => read(joint),
            Reader::Body(_) | Reader::Pair(_) => None,
        }
    }
}

/// Total kinetic energy `½mv² + ½Iω²` (a missing inertia counts as no
/// rotational term). The one definition the catalog and the physics facade
/// share.
pub fn kinetic_energy(
    mass: Mass,
    inertia: Option<f32>,
    velocity: Velocity2,
    spin: AngularVelocity,
) -> Energy {
    let translational = 0.5 * mass.value() * velocity.value().length_squared();
    let rotational = inertia.map_or(0.0, |i| 0.5 * i * spin.value().powi(2));
    Energy(translational + rotational)
}

/// Linear momentum magnitude `m·|v|`.
pub fn momentum(mass: Mass, velocity: Velocity2) -> Momentum {
    Momentum(mass.value() * velocity.value().length())
}

/// Angular momentum `I·ω`, signed CCW-positive.
pub fn angular_momentum(inertia: f32, spin: AngularVelocity) -> AngularMomentum {
    AngularMomentum(inertia * spin.value())
}

fn speed(b: &dyn BodyRead) -> Option<f32> {
    b.velocity().map(|(v, _)| v.magnitude().value())
}

fn spin(b: &dyn BodyRead) -> Option<f32> {
    b.velocity().map(|(_, w)| w.value().abs())
}

fn height(b: &dyn BodyRead) -> Option<f32> {
    b.position().map(|p| p.y)
}

fn pos_x(b: &dyn BodyRead) -> Option<f32> {
    b.position().map(|p| p.x)
}

fn contact_force(b: &dyn BodyRead) -> Option<f32> {
    b.contact_force().map(|f| f.magnitude().value())
}

fn contact_count(b: &dyn BodyRead) -> Option<f32> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a contact count is far below f32's exact-integer range"
    )]
    b.contact_count().map(|n| n as f32)
}

fn energy(b: &dyn BodyRead) -> Option<f32> {
    let (v, w) = b.velocity()?;
    Some(kinetic_energy(b.mass()?, b.angular_inertia(), v, w).value())
}

fn linear_momentum(b: &dyn BodyRead) -> Option<f32> {
    let (v, _) = b.velocity()?;
    Some(momentum(b.mass()?, v).value())
}

fn spin_momentum(b: &dyn BodyRead) -> Option<f32> {
    let (_, w) = b.velocity()?;
    Some(angular_momentum(b.angular_inertia()?, w).value())
}

fn mass(b: &dyn BodyRead) -> Option<f32> {
    b.mass().map(Mass::value)
}

fn distance(a: &dyn BodyRead, b: &dyn BodyRead) -> Option<f32> {
    Some(a.position()?.distance(b.position()?))
}

fn joint_length(j: &dyn JointRead) -> Option<f32> {
    j.sensors().map(|s| s.length.value())
}

fn joint_angle(j: &dyn JointRead) -> Option<f32> {
    j.sensors().map(|s| s.angle.value())
}

fn joint_force(j: &dyn JointRead) -> Option<f32> {
    j.reaction_force().map(Force::value)
}

/// The catalog. Body rows come first, in sensor-port order (the inspector
/// rows and the node-graph output pins list them in this order).
pub const QUANTITIES: &[PhysicalQuantity] = &[
    PhysicalQuantity {
        name: "speed",
        label: "speed",
        dimension: Dimension::Velocity,
        doc: "Linear speed of a body.",
        reader: Reader::Body(speed),
    },
    PhysicalQuantity {
        name: "spin",
        label: "spin",
        dimension: Dimension::AngularVelocity,
        doc: "Angular speed of a body (absolute).",
        reader: Reader::Body(spin),
    },
    PhysicalQuantity {
        name: "height",
        label: "height",
        dimension: Dimension::Length,
        doc: "A body's height (world y).",
        reader: Reader::Body(height),
    },
    PhysicalQuantity {
        name: "pos-x",
        label: "pos x",
        dimension: Dimension::Length,
        doc: "A body's horizontal position (world x).",
        reader: Reader::Body(pos_x),
    },
    PhysicalQuantity {
        name: "contact-force",
        label: "force",
        dimension: Dimension::Force,
        doc: "Net normal contact force on a body over the last step.",
        reader: Reader::Body(contact_force),
    },
    PhysicalQuantity {
        name: "contact-count",
        label: "contacts",
        dimension: Dimension::Dimensionless,
        doc: "How many bodies a body is touching.",
        reader: Reader::Body(contact_count),
    },
    PhysicalQuantity {
        name: "kinetic-energy",
        label: "energy",
        dimension: Dimension::Energy,
        doc: "Total kinetic energy of a body (translational + rotational).",
        reader: Reader::Body(energy),
    },
    PhysicalQuantity {
        name: "momentum",
        label: "momentum",
        dimension: Dimension::Momentum,
        doc: "Linear momentum magnitude of a body (m·|v|).",
        reader: Reader::Body(linear_momentum),
    },
    PhysicalQuantity {
        name: "angular-momentum",
        label: "ang mom",
        dimension: Dimension::AngularMomentum,
        doc: "Angular momentum of a body (I·ω, counter-clockwise positive).",
        reader: Reader::Body(spin_momentum),
    },
    PhysicalQuantity {
        name: "mass",
        label: "mass",
        dimension: Dimension::Mass,
        doc: "The solver's mass of a body (density × area).",
        reader: Reader::Body(mass),
    },
    PhysicalQuantity {
        name: "distance",
        label: "distance",
        dimension: Dimension::Length,
        doc: "Centre-to-centre distance between two bodies.",
        reader: Reader::Pair(distance),
    },
    PhysicalQuantity {
        name: "joint-length",
        label: "length",
        dimension: Dimension::Length,
        doc: "Distance between a joint's two world anchors.",
        reader: Reader::Joint(joint_length),
    },
    PhysicalQuantity {
        name: "joint-angle",
        label: "angle",
        dimension: Dimension::Angle,
        doc: "A joint's relative rotation minus its rest angle (the limit frame).",
        reader: Reader::Joint(joint_angle),
    },
    PhysicalQuantity {
        name: "joint-force",
        label: "force",
        dimension: Dimension::Force,
        doc: "Magnitude of a joint's linear reaction force after the last solve.",
        reader: Reader::Joint(joint_force),
    },
];

/// The row named `name`.
pub fn quantity(name: &str) -> Option<&'static PhysicalQuantity> {
    QUANTITIES.iter().find(|q| q.name == name)
}

/// Every row read off `subject`, in catalog order.
pub fn quantities_of(subject: Subject) -> impl Iterator<Item = &'static PhysicalQuantity> {
    QUANTITIES.iter().filter(move |q| q.subject() == subject)
}

/// Every name, in catalog order, comma-separated: for an error that says what
/// the caller could have named.
pub fn known_names() -> String {
    QUANTITIES
        .iter()
        .map(|q| q.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A body's state captured once: what a [`BodyRead`] answered at capture
/// time. The operation context holds these so a script run reads one
/// snapshot (scripting rule 3); tests use them as a known state.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BodySample {
    /// [`BodyRead::position`].
    pub position: Option<Vec2>,
    /// [`BodyRead::velocity`].
    pub velocity: Option<(Velocity2, AngularVelocity)>,
    /// [`BodyRead::mass`].
    pub mass: Option<Mass>,
    /// [`BodyRead::angular_inertia`].
    pub angular_inertia: Option<f32>,
    /// [`BodyRead::contact_force`].
    pub contact_force: Option<Force2>,
    /// [`BodyRead::contact_count`].
    pub contact_count: Option<usize>,
}

impl BodySample {
    /// Asks every [`BodyRead`] question once.
    pub fn capture(body: &dyn BodyRead) -> Self {
        Self {
            position: body.position(),
            velocity: body.velocity(),
            mass: body.mass(),
            angular_inertia: body.angular_inertia(),
            contact_force: body.contact_force(),
            contact_count: body.contact_count(),
        }
    }
}

impl BodyRead for BodySample {
    fn position(&self) -> Option<Vec2> {
        self.position
    }
    fn velocity(&self) -> Option<(Velocity2, AngularVelocity)> {
        self.velocity
    }
    fn mass(&self) -> Option<Mass> {
        self.mass
    }
    fn angular_inertia(&self) -> Option<f32> {
        self.angular_inertia
    }
    fn contact_force(&self) -> Option<Force2> {
        self.contact_force
    }
    fn contact_count(&self) -> Option<usize> {
        self.contact_count
    }
}

/// A joint's state captured once (see [`BodySample`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JointSample {
    /// [`JointRead::sensors`].
    pub sensors: Option<JointSensors>,
    /// [`JointRead::reaction_force`].
    pub reaction_force: Option<Force>,
}

impl JointSample {
    /// Asks every [`JointRead`] question once.
    pub fn capture(joint: &dyn JointRead) -> Self {
        Self {
            sensors: joint.sensors(),
            reaction_force: joint.reaction_force(),
        }
    }
}

impl JointRead for JointSample {
    fn sensors(&self) -> Option<JointSensors> {
        self.sensors
    }
    fn reaction_force(&self) -> Option<Force> {
        self.reaction_force
    }
}

/// Every body's and joint's captured state, by id: the read-only mirror the
/// `quantity` operations read.
#[derive(Debug, Clone, Default)]
pub struct QuantitySamples {
    /// Bodies by id.
    pub bodies: HashMap<StableId, BodySample>,
    /// Joints by id.
    pub joints: HashMap<StableId, JointSample>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_units::{Angle, Length};

    /// A body with every read set to a distinctive value.
    fn known_body() -> BodySample {
        BodySample {
            position: Some(Vec2::new(3.0, 4.0)),
            velocity: Some((Velocity2::new(Vec2::new(3.0, -4.0)), AngularVelocity(-2.0))),
            mass: Some(Mass(2.0)),
            angular_inertia: Some(0.5),
            contact_force: Some(Force2::new(Vec2::new(0.0, 19.6))),
            contact_count: Some(2),
        }
    }

    fn known_joint() -> JointSample {
        JointSample {
            sensors: Some(JointSensors {
                length: Length(0.75),
                angle: Angle(0.25),
            }),
            reaction_force: Some(Force(12.0)),
        }
    }

    /// The value every row must read off [`known_body`] / [`known_joint`]
    /// (and the origin body, for the pair). A row missing here fails the
    /// test, so every reader is checked.
    const EXPECTED: &[(&str, f32)] = &[
        ("speed", 5.0),
        ("spin", 2.0),
        ("height", 4.0),
        ("pos-x", 3.0),
        ("contact-force", 19.6),
        ("contact-count", 2.0),
        // ½·2·25 + ½·0.5·4
        ("kinetic-energy", 26.0),
        ("momentum", 10.0),
        ("angular-momentum", -1.0),
        ("mass", 2.0),
        ("distance", 5.0),
        ("joint-length", 0.75),
        ("joint-angle", 0.25),
        ("joint-force", 12.0),
    ];

    fn expected(name: &str) -> Option<f32> {
        EXPECTED.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
    }

    #[test]
    fn every_reader_reads_a_known_state() {
        let body = known_body();
        let origin = BodySample {
            position: Some(Vec2::ZERO),
            ..BodySample::default()
        };
        let joint = known_joint();
        for q in QUANTITIES {
            let want = expected(q.name)
                .unwrap_or_else(|| panic!("no expected value for catalog row {:?}", q.name));
            let got = match q.subject() {
                Subject::Body => q.read_body(&body),
                Subject::BodyPair => q.read_pair(&origin, &body),
                Subject::Joint => q.read_joint(&joint),
            };
            let got = got.unwrap_or_else(|| panic!("{} read nothing", q.name));
            assert!((got - want).abs() < 1e-5, "{}: {got} != {want}", q.name);
        }
    }

    #[test]
    fn an_empty_state_reads_nothing() {
        let empty = BodySample::default();
        for q in quantities_of(Subject::Body) {
            assert_eq!(q.read_body(&empty), None, "{}", q.name);
        }
        assert_eq!(JointSample::default().sensors(), None);
    }

    #[test]
    fn a_reader_only_answers_its_own_subject() {
        let body = known_body();
        let joint = known_joint();
        for q in QUANTITIES {
            let s = q.subject();
            assert_eq!(
                s == Subject::Body,
                q.read_body(&body).is_some(),
                "{}",
                q.name
            );
            assert_eq!(
                s == Subject::BodyPair,
                q.read_pair(&body, &body).is_some(),
                "{}",
                q.name
            );
            assert_eq!(
                s == Subject::Joint,
                q.read_joint(&joint).is_some(),
                "{}",
                q.name
            );
        }
    }

    #[test]
    fn names_are_unique_stable_keys() {
        let mut names: Vec<&str> = QUANTITIES.iter().map(|q| q.name).collect();
        for name in &names {
            assert!(
                !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{name:?} is not a lower-case, dash-separated key"
            );
            assert_eq!(quantity(name).map(|q| q.name), Some(*name));
        }
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate catalog names");
        assert!(quantity("nonsense").is_none());
        assert!(known_names().starts_with("speed, spin"));
    }

    #[test]
    fn a_capture_answers_what_was_read() {
        let body = known_body();
        assert_eq!(BodySample::capture(&body), body);
        let joint = known_joint();
        assert_eq!(JointSample::capture(&joint), joint);
    }
}
