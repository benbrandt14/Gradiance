//! The simulation clock — the Gradiance-owned reading of simulated time.
//!
//! Tracers, plotters and signal sampling all need "how far has the simulation
//! got", and pausing must freeze them. That is a physics *fact*, but it is not
//! an engine *type*: exposing the engine's own clock resource forces every
//! reader to depend on the engine, and makes every engine swap a change to
//! `render` and `signal`.
//!
//! [`SimClock`] is that reading, owned here. Readers see plain typed
//! quantities and never name the engine; the one system that fills it is the
//! only place the engine's clock is touched.

use bevy::prelude::*;
use gradiance_units::Time as Seconds;

/// Simulated time — advances while playing, holds still while paused, and
/// scales with `SimSettings::speed`.
///
/// Distinct from bevy's wall-clock `Time`: a trail sampled against this one
/// stops growing the moment the sim pauses, which is what makes a tracer trace
/// *motion* rather than real time.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct SimClock {
    /// Total simulated time since startup.
    pub elapsed: Seconds,
    /// Simulated time added by the last update (zero while paused).
    pub delta: Seconds,
    /// Whether the simulation is currently frozen.
    pub paused: bool,
}

impl SimClock {
    /// Elapsed simulated time in seconds — the form gizmo and trail maths want.
    #[must_use]
    pub fn elapsed_secs(&self) -> f32 {
        self.elapsed.value()
    }

    /// Last update's simulated delta in seconds.
    #[must_use]
    pub fn delta_secs(&self) -> f32 {
        self.delta.value()
    }
}

/// Mirrors the engine's physics clock into [`SimClock`].
///
/// The single point where simulated time is read off the engine; swapping the
/// engine rewrites this function and nothing downstream.
pub fn sync_sim_clock(
    engine: Res<bevy::prelude::Time<avian2d::prelude::Physics>>,
    mut clock: ResMut<SimClock>,
) {
    use avian2d::prelude::PhysicsTime as _;

    let next = SimClock {
        elapsed: Seconds(engine.elapsed_secs()),
        delta: Seconds(engine.delta_secs()),
        paused: engine.is_paused(),
    };
    // Change detection on the clock would fire every frame and is meaningless;
    // write through so a reader's `Changed<>` means something if one appears.
    if *clock != next {
        *clock = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_reads_as_seconds() {
        let clock = SimClock {
            elapsed: Seconds(2.5),
            delta: Seconds(0.016),
            paused: false,
        };
        assert!((clock.elapsed_secs() - 2.5).abs() < 1e-6);
        assert!((clock.delta_secs() - 0.016).abs() < 1e-6);
    }

    #[test]
    fn a_default_clock_has_not_started() {
        let clock = SimClock::default();
        assert!((clock.elapsed_secs()).abs() < f32::EPSILON);
        assert!(!clock.paused);
    }
}
