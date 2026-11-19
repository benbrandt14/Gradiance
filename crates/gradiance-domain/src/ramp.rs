//! The toon **colour ramp** (brief 4b-6): how lighting becomes colour.
//!
//! The toon shader measures how lit a pixel is (the summed lights over the
//! body's albedo, `0..1` after the white point) and looks that level up in a
//! [`ColorRamp`]: an ordered list of colour stops, each with a position, and
//! constant (hard bands) or linear interpolation between them, as Blender's
//! `ColorRamp` node. The stop colour is a **tone**, not a fill: it tints the
//! body's own colour by a hard-light blend ([`shade`]), so every body goes
//! through the same falloff (the same shadow hue, the same lit patch) and
//! the scene reads as one family, whatever the fills.
//!
//! - Mid grey ([`NEUTRAL`], 0.5) shows the body's colour as authored.
//! - A darker tone multiplies: the shadow stops, and their hue shift (a
//!   warm piece falling into a cool mauve shadow).
//! - A lighter tone screens toward it: a lit patch in the light's hue.
//!
//! **Two tiers** (CLAUDE.md, the perf rule): the stops are an authoring
//! form. [`ColorRamp::table`] samples them once into a fixed table the
//! render layer uploads as a uniform; the frame loop does an index and a
//! lerp, never a search. `kernel::Lut` is the same idea for scalar curves.
//!
//! Colours in a ramp are data (the look presets live in
//! [`theme`](crate::theme), the one source of scene colour).

use crate::appearance::Rgba;
use serde::{Deserialize, Serialize};

/// The hard-light tone that leaves a body's colour as authored.
pub const NEUTRAL: f32 = 0.5;

/// Entries in the sampled table ([`ColorRamp::table`]); a constant ramp's
/// stop edges land on a `1 / RAMP_TABLE` grid of lighting levels.
pub const RAMP_TABLE: usize = 128;

/// The most stops a ramp keeps (the editor stops adding at this count).
pub const MAX_STOPS: usize = 16;

/// How a ramp blends between its stops.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub enum RampInterp {
    /// Each stop's tone holds until the next stop: hard toon bands.
    #[default]
    Constant,
    /// Tones blend linearly between stops: a smooth falloff.
    Linear,
}

impl RampInterp {
    /// Both modes, in menu order.
    pub const ALL: [Self; 2] = [Self::Constant, Self::Linear];

    /// The stable lower-case name (menus, scripts).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Constant => "constant",
            Self::Linear => "linear",
        }
    }
}

/// One stop of a [`ColorRamp`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct RampStop {
    /// The lighting level where this stop's tone starts, in `[0, 1]`.
    pub position: f32,
    /// The tone (sRGB; alpha unused). See the module docs for how it
    /// tints a body.
    pub color: Rgba,
}

/// A colour ramp over lighting levels `[0, 1]` (module docs).
///
/// Stops are kept sorted by position by every editing method here; a ramp
/// read from a file is sorted again by [`ColorRamp::table`], so an
/// out-of-order list still renders as its positions say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct ColorRamp {
    /// Constant (bands) or linear.
    pub interp: RampInterp,
    /// The stops, ascending by position. Never empty after editing.
    pub stops: Vec<RampStop>,
}

impl Default for ColorRamp {
    /// The default look's ramp.
    fn default() -> Self {
        crate::theme::Look::default().ramp()
    }
}

impl ColorRamp {
    /// A ramp from `(position, tone)` pairs, sorted.
    #[must_use]
    pub fn new(interp: RampInterp, stops: &[(f32, Rgba)]) -> Self {
        let mut ramp = Self {
            interp,
            stops: stops
                .iter()
                .map(|&(position, color)| RampStop { position, color })
                .collect(),
        };
        ramp.sort();
        ramp
    }

    /// Clamps every position into `[0, 1]` and sorts the stops by it.
    pub fn sort(&mut self) {
        for stop in &mut self.stops {
            stop.position = if stop.position.is_finite() {
                stop.position.clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        self.stops.sort_by(|a, b| a.position.total_cmp(&b.position));
    }

    /// The tone at lighting level `level` (clamped to `[0, 1]`).
    ///
    /// The authoring form (a search over the stops); the frame loop reads
    /// [`table`](Self::table) instead. Below the first stop the first tone
    /// holds, above the last the last; an empty ramp is [`NEUTRAL`].
    #[must_use]
    pub fn eval(&self, level: f32) -> Rgba {
        let level = if level.is_finite() {
            level.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut sorted = self.stops.clone();
        sorted.sort_by(|a, b| a.position.total_cmp(&b.position));
        let Some(first) = sorted.first() else {
            return neutral();
        };
        let after = sorted.partition_point(|s| s.position <= level);
        if after == 0 {
            return first.color;
        }
        let lo = sorted[after - 1];
        let Some(hi) = sorted.get(after) else {
            return lo.color;
        };
        match self.interp {
            RampInterp::Constant => lo.color,
            RampInterp::Linear => {
                let span = hi.position - lo.position;
                let t = if span > f32::EPSILON {
                    (level - lo.position) / span
                } else {
                    1.0
                };
                lerp(lo.color, hi.color, t)
            }
        }
    }

    /// The ramp sampled into [`RAMP_TABLE`] tones (`[r, g, b, 1]`, sRGB),
    /// which the shader indexes by lighting level.
    ///
    /// Constant: entry `i` is the tone over levels `[i, i + 1) / N`
    /// (sampled at the bin's centre), read with a floor. Linear: entry `i`
    /// is the tone at `i / (N - 1)`, read with a lerp.
    #[must_use]
    pub fn table(&self) -> Vec<[f32; 4]> {
        let n = RAMP_TABLE;
        (0..n)
            .map(|i| {
                let level = match self.interp {
                    RampInterp::Constant => (i as f32 + 0.5) / n as f32,
                    RampInterp::Linear => i as f32 / (n - 1) as f32,
                };
                let c = self.eval(level);
                [c.r, c.g, c.b, 1.0]
            })
            .collect()
    }

    /// Inserts a stop at `position` with the tone the ramp has there (so
    /// the ramp looks the same until the stop is edited). Returns its
    /// index, or `None` at [`MAX_STOPS`].
    pub fn insert(&mut self, position: f32) -> Option<usize> {
        if self.stops.len() >= MAX_STOPS {
            return None;
        }
        let position = position.clamp(0.0, 1.0);
        let stop = RampStop {
            position,
            color: self.eval(position),
        };
        self.stops.push(stop);
        self.sort();
        self.stops.iter().position(|s| *s == stop)
    }

    /// Removes stop `index`, never the last one left. Returns whether a
    /// stop was removed.
    pub fn remove(&mut self, index: usize) -> bool {
        if self.stops.len() <= 1 || index >= self.stops.len() {
            return false;
        }
        self.stops.remove(index);
        true
    }

    /// Moves stop `index` to `position` (clamped) and re-sorts. Returns the
    /// stop's new index (a stop dragged past its neighbour changes place).
    pub fn move_stop(&mut self, index: usize, position: f32) -> usize {
        let Some(stop) = self.stops.get_mut(index) else {
            return index;
        };
        stop.position = position.clamp(0.0, 1.0);
        let moved = *stop;
        self.sort();
        self.stops.iter().position(|s| *s == moved).unwrap_or(index)
    }
}

/// [`NEUTRAL`] as a colour.
#[must_use]
pub const fn neutral() -> Rgba {
    Rgba {
        r: NEUTRAL,
        g: NEUTRAL,
        b: NEUTRAL,
        a: 1.0,
    }
}

fn lerp(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: f32, y: f32| x + (y - x) * t;
    Rgba {
        r: mix(a.r, b.r),
        g: mix(a.g, b.g),
        b: mix(a.b, b.b),
        a: mix(a.a, b.a),
    }
}

/// The hard-light blend of one channel: `tone` below [`NEUTRAL`] multiplies
/// `base` (`2 base tone`), above it screens toward white
/// (`1 - 2 (1 - base)(1 - tone)`); exactly neutral returns `base`.
#[must_use]
pub fn hard_light(base: f32, tone: f32) -> f32 {
    if tone < NEUTRAL {
        2.0 * base * tone
    } else {
        1.0 - 2.0 * (1.0 - base) * (1.0 - tone)
    }
}

/// A body of fill `base` (sRGB) shown at a ramp `tone`: the hard-light
/// blend per channel, alpha from `base`. `toon.wesl` does the same sum in
/// the shader (on the linear colour taken to gamma 2.2), so the ramp
/// editor's preview strip and a lit body agree.
#[must_use]
pub fn shade(base: Rgba, tone: Rgba) -> Rgba {
    Rgba {
        r: hard_light(base.r, tone.r),
        g: hard_light(base.g, tone.g),
        b: hard_light(base.b, tone.b),
        a: base.a,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact sentinels: stop tones are copied, not computed
mod tests {
    use super::*;

    fn grey(v: f32) -> Rgba {
        Rgba::rgb(v, v, v)
    }

    fn three() -> ColorRamp {
        ColorRamp::new(
            RampInterp::Constant,
            &[(0.6, grey(0.5)), (0.0, grey(0.2)), (0.3, grey(0.35))],
        )
    }

    #[test]
    fn new_sorts_and_constant_holds_each_tone_until_the_next_stop() {
        let ramp = three();
        let positions: Vec<f32> = ramp.stops.iter().map(|s| s.position).collect();
        assert_eq!(positions, [0.0, 0.3, 0.6]);
        assert_eq!(ramp.eval(0.1), grey(0.2));
        assert_eq!(ramp.eval(0.3), grey(0.35), "a stop starts at its position");
        assert_eq!(ramp.eval(0.59), grey(0.35));
        assert_eq!(ramp.eval(1.0), grey(0.5));
        assert_eq!(ramp.eval(f32::NAN), grey(0.2), "not finite reads as 0");
    }

    #[test]
    fn linear_blends_between_stops_and_holds_past_the_ends() {
        let mut ramp = ColorRamp::new(RampInterp::Linear, &[(0.2, grey(0.0)), (0.6, grey(0.8))]);
        assert_eq!(ramp.eval(0.0), grey(0.0));
        assert!((ramp.eval(0.4).r - 0.4).abs() < 1e-6);
        assert_eq!(ramp.eval(0.9), grey(0.8));
        ramp.stops.clear();
        assert_eq!(ramp.eval(0.5), neutral(), "an empty ramp is neutral");
    }

    #[test]
    fn the_table_matches_eval_at_its_sample_points() {
        let ramp = three();
        let table = ramp.table();
        assert_eq!(table.len(), RAMP_TABLE);
        // Constant: bin i covers [i, i+1)/N; the stop at 0.3 starts in the
        // bin holding 0.3.
        let bin = (0.3 * RAMP_TABLE as f32) as usize;
        assert_eq!(table[bin - 1][0], 0.2);
        assert_eq!(table[bin + 1][0], 0.35);
        assert_eq!(table[RAMP_TABLE - 1][0], 0.5);
        let linear = ColorRamp {
            interp: RampInterp::Linear,
            ..ramp
        };
        let table = linear.table();
        assert_eq!(table[0][0], 0.2);
        assert_eq!(table[RAMP_TABLE - 1][0], 0.5);
    }

    #[test]
    fn insert_keeps_the_look_and_remove_keeps_one_stop() {
        let mut ramp = three();
        let before = ramp.table();
        let i = ramp.insert(0.45).expect("room");
        assert_eq!(ramp.stops[i].position, 0.45);
        assert_eq!(
            ramp.table(),
            before,
            "a new stop takes the tone it lands on"
        );
        assert!(ramp.remove(i));
        while ramp.stops.len() > 1 {
            assert!(ramp.remove(0));
        }
        assert!(!ramp.remove(0), "the last stop stays");
        for _ in 0..MAX_STOPS {
            ramp.insert(0.5);
        }
        assert_eq!(ramp.stops.len(), MAX_STOPS);
        assert_eq!(ramp.insert(0.5), None);
    }

    #[test]
    fn moving_a_stop_past_its_neighbour_reorders() {
        let mut ramp = three();
        let to = ramp.move_stop(0, 0.45);
        assert_eq!(to, 1);
        assert_eq!(ramp.stops[1].color, grey(0.2));
        assert_eq!(ramp.move_stop(2, 7.0), 2);
        assert_eq!(ramp.stops[2].position, 1.0, "clamped");
    }

    #[test]
    fn hard_light_is_identity_at_neutral_and_darkens_or_lightens_around_it() {
        for base in [0.0, 0.3, 0.7, 1.0] {
            assert!((hard_light(base, NEUTRAL) - base).abs() < 1e-6);
            assert!(hard_light(base, 0.25) <= base);
            assert!(hard_light(base, 0.75) >= base);
        }
        assert_eq!(hard_light(0.4, 0.0), 0.0);
        assert_eq!(hard_light(0.4, 1.0), 1.0);
        let fill = Rgba::rgb(0.7, 0.39, 0.13).with_alpha(0.5);
        assert_eq!(shade(fill, neutral()).a, 0.5, "alpha from the fill");
    }

    #[test]
    fn every_look_ramp_is_sorted_and_lights_a_fill_as_authored_somewhere() {
        for look in crate::theme::Look::ALL {
            let ramp = look.ramp();
            assert!(ramp.stops.len() >= 2, "{look:?} has bands");
            assert!(
                ramp.stops
                    .windows(2)
                    .all(|w| w[0].position <= w[1].position),
                "{look:?} sorted"
            );
            assert!(
                ramp.stops.iter().any(|s| s.color == neutral()),
                "{look:?}: one band shows the fill as authored"
            );
            assert!(
                ramp.stops[0].color.r < NEUTRAL,
                "{look:?}: the darkest band darkens"
            );
        }
    }
}
