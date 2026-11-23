//! Lasers: an authored beam that reflects off bodies (brief N-8).
//!
//! A [`Laser`] is the payload of a placeable behavior node
//! ([`NodeKind::Laser`](crate::node::NodeKind::Laser)). It casts a ray from
//! the node's point along its axis, reflects off every body it hits up to
//! [`bounces`](Laser::bounces) times, and ends at its
//! [`range`](Laser::range), at its last bounce, or on its
//! [`target`](Laser::target) body. The axis is stored in the **body's
//! frame** when the laser rides a body (it turns with the body), and is a
//! world angle for a free laser.
//!
//! It publishes two bus values under its [`name`](Laser::name): the name
//! itself reads `1` while the beam reaches the target and `0` otherwise, and
//! `<name>.distance` the beam's path length to where it ends (metres). A
//! signal reads them like any bus value, so a light gate drives an actuator
//! through the N-1 sinks.
//!
//! Only the laser is authored (saved, undone through
//! `PropertyValue::NodeKind`); the beam path and the bus values are derived
//! (`physics::laser`), never saved and never undone.

use bevy::reflect::Reflect;
use gradiance_core::ids::StableId;
use gradiance_units::{Angle, Length};
use serde::{Deserialize, Serialize};

/// The most reflections any laser traces: the hard cap that bounds the ray
/// casts per laser per step.
pub const MAX_BOUNCES: u32 = 32;

/// The reflections a new laser traces.
pub const DEFAULT_BOUNCES: u32 = 8;

/// How far a new laser reaches, along the whole path (m).
pub const DEFAULT_RANGE: f32 = 50.0;

/// The longest range a laser takes (m).
pub const MAX_RANGE: f32 = 10_000.0;

/// The bus name a new laser publishes under.
pub const DEFAULT_NAME: &str = "laser";

/// The suffix of a laser's distance signal (`<name>.distance`).
pub const DISTANCE_SUFFIX: &str = ".distance";

/// The first of `laser`, `laser2`, `laser3`, … that no name in `taken` is
/// (brief Q-4): a new laser drawn with the tool publishes its own values.
pub fn free_name<'a>(taken: impl IntoIterator<Item = &'a str>) -> String {
    let taken: Vec<&str> = taken.into_iter().collect();
    if !taken.contains(&DEFAULT_NAME) {
        return DEFAULT_NAME.to_owned();
    }
    // `taken` has n names, so one of the first n + 1 candidates is free.
    (2..=taken.len() + 2)
        .map(|n| format!("{DEFAULT_NAME}{n}"))
        .find(|name| !taken.contains(&name.as_str()))
        .unwrap_or_else(|| DEFAULT_NAME.to_owned())
}

/// An authored laser: its axis, reach, bounce limit, target and bus name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Laser {
    /// The bus name of the hit signal; the distance is `<name>.distance`.
    pub name: String,
    /// The body whose hit reads `1`, by id. `None`: the hit reads `0`.
    pub target: Option<StableId>,
    /// The beam's axis, as an angle from the body's local +x (radians,
    /// counter-clockwise). For a free laser it is a world angle.
    pub angle: Angle,
    /// How far the beam reaches, along the whole path (m).
    pub range: Length,
    /// How many times the beam reflects or refracts, at most (capped at
    /// [`MAX_BOUNCES`]).
    pub bounces: u32,
    /// The beam's colour (brief S-3). `None`: the look's laser ink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<crate::appearance::Rgba>,
    /// White light (brief S-15): the beam splits into the spectrum's
    /// colours where a dispersive body refracts it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub white: bool,
}

impl Default for Laser {
    /// Along the body's local +x, 50 m, 8 bounces, no target, named
    /// `"laser"`.
    fn default() -> Self {
        Self {
            name: DEFAULT_NAME.to_owned(),
            target: None,
            angle: Angle(0.0),
            range: Length(DEFAULT_RANGE),
            bounces: DEFAULT_BOUNCES,
            color: None,
            white: false,
        }
    }
}

impl Laser {
    /// The bus name of the distance signal: `<name>.distance`.
    #[must_use]
    pub fn distance_name(&self) -> String {
        format!("{}{DISTANCE_SUFFIX}", self.name)
    }

    /// The bounce limit actually traced: the authored one, capped.
    #[must_use]
    pub fn bounce_limit(&self) -> u32 {
        self.bounces.min(MAX_BOUNCES)
    }

    /// The range actually traced: the authored one, clamped to
    /// `[0, MAX_RANGE]` (a non-finite range traces nothing).
    #[must_use]
    pub fn reach(&self) -> f32 {
        if self.range.0.is_finite() {
            self.range.0.clamp(0.0, MAX_RANGE)
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_laser_takes_the_first_free_name() {
        assert_eq!(free_name([]), "laser");
        assert_eq!(free_name(["gate"]), "laser");
        assert_eq!(free_name(["laser"]), "laser2");
        assert_eq!(free_name(["laser", "laser2", "laser4"]), "laser3");
    }

    #[test]
    fn the_bus_names_follow_the_name() {
        let laser = Laser {
            name: "gate".into(),
            ..Laser::default()
        };
        assert_eq!(laser.distance_name(), "gate.distance");
        assert_eq!(Laser::default().name, "laser");
    }

    #[test]
    fn bounces_and_range_are_capped() {
        let laser = Laser {
            bounces: 1000,
            range: Length(f32::INFINITY),
            ..Laser::default()
        };
        assert_eq!(laser.bounce_limit(), MAX_BOUNCES);
        assert_eq!(laser.reach(), 0.0);
        let far = Laser {
            range: Length(1e9),
            ..Laser::default()
        };
        assert_eq!(far.reach(), MAX_RANGE);
    }
}
