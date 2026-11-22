//! Authored lifetime (brief S-7): a body that removes whatever it touches,
//! or removes itself after a time.
//!
//! Removal is **derived** and lasts for the run: `physics::lifetime` marks a
//! body `Removed` (no collider, no motion, hidden) without despawning it, so
//! the authored scene is unchanged and undoing the run brings it back. The
//! VM never runs per frame, so there is no script callback on a contact; a
//! script reads a contact through the `hit` signal instead.

use bevy::prelude::{Component, Message};
use gradiance_units::Time;
use serde::{Deserialize, Serialize};

/// What removes a body, or what a body removes, during a run.
#[derive(
    Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Default,
    Serialize,
    Deserialize,
    bevy::reflect::Reflect,
)]
pub struct Lifetime {
    /// Remove every other body this one starts touching.
    #[serde(default)]
    pub remove_on_contact: bool,
    /// Remove this body after this much simulated time (`None`: never).
    #[serde(default)]
    pub ttl: Option<Time>,
}

impl Lifetime {
    /// The lifetime with a time that is not positive and finite dropped,
    /// or `None` when nothing is left to do (the body keeps no component).
    #[must_use]
    pub fn sanitized(self) -> Option<Self> {
        let ttl = self.ttl.filter(|t| t.0.is_finite() && t.0 > 0.0);
        (self.remove_on_contact || ttl.is_some()).then_some(Self {
            remove_on_contact: self.remove_on_contact,
            ttl,
        })
    }
}

/// Bring every removed body back and restart every time to live: the run
/// they belonged to was undone. The command layer sends it; the physics
/// layer answers it. (A loaded scene respawns every body, so it needs none.)
#[derive(Message, Debug, Clone, Copy, Default)]
pub struct ClearRemovals;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_lifetime_is_none_and_a_bad_time_is_dropped() {
        assert_eq!(Lifetime::default().sanitized(), None);
        let bad = Lifetime {
            remove_on_contact: false,
            ttl: Some(Time(-1.0)),
        };
        assert_eq!(bad.sanitized(), None);
        let floor = Lifetime {
            remove_on_contact: true,
            ttl: Some(Time(f32::NAN)),
        };
        assert_eq!(
            floor.sanitized(),
            Some(Lifetime {
                remove_on_contact: true,
                ttl: None
            })
        );
    }
}
