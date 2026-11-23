//! Gamepad input names (brief S-20): the one table between a stick's or a
//! button's written name (`"LeftStickX"`, `"South"`) and the gamepad input
//! it reads.
//!
//! A node-graph **pad input** (`signal::BlockOp::Pad`) saves its input by
//! name, so a scene file reads `input: "LeftStickX"`, not an engine enum.
//! Its value reaches the kernel as the bus input [`pad_input_name`] names,
//! published each frame by the interaction layer: a stick reads −1..1, a
//! button 0..1 (its analog value: a trigger's travel, 1 for a pressed
//! digital button). The dead zone is part of that bus name, so two blocks
//! on one stick with two dead zones read two inputs.

use bevy::input::gamepad::{GamepadAxis, GamepadButton};

/// What a pad input reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadSource {
    /// A stick or analog axis: −1..1.
    Axis(GamepadAxis),
    /// A button: 0..1.
    Button(GamepadButton),
}

/// One named gamepad input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedPad {
    /// The canonical name (as saved and shown).
    pub name: &'static str,
    /// The input it reads.
    pub source: PadSource,
}

const fn axis(name: &'static str, a: GamepadAxis) -> NamedPad {
    NamedPad {
        name,
        source: PadSource::Axis(a),
    }
}

const fn button(name: &'static str, b: GamepadButton) -> NamedPad {
    NamedPad {
        name,
        source: PadSource::Button(b),
    }
}

/// Every gamepad input a pad block can read, sticks first.
pub const PADS: [NamedPad; 23] = [
    axis("LeftStickX", GamepadAxis::LeftStickX),
    axis("LeftStickY", GamepadAxis::LeftStickY),
    axis("RightStickX", GamepadAxis::RightStickX),
    axis("RightStickY", GamepadAxis::RightStickY),
    axis("LeftZ", GamepadAxis::LeftZ),
    axis("RightZ", GamepadAxis::RightZ),
    button("South", GamepadButton::South),
    button("East", GamepadButton::East),
    button("North", GamepadButton::North),
    button("West", GamepadButton::West),
    button("LeftTrigger", GamepadButton::LeftTrigger),
    button("RightTrigger", GamepadButton::RightTrigger),
    button("LeftTrigger2", GamepadButton::LeftTrigger2),
    button("RightTrigger2", GamepadButton::RightTrigger2),
    button("Select", GamepadButton::Select),
    button("Start", GamepadButton::Start),
    button("Mode", GamepadButton::Mode),
    button("LeftThumb", GamepadButton::LeftThumb),
    button("RightThumb", GamepadButton::RightThumb),
    button("DPadUp", GamepadButton::DPadUp),
    button("DPadDown", GamepadButton::DPadDown),
    button("DPadLeft", GamepadButton::DPadLeft),
    button("DPadRight", GamepadButton::DPadRight),
];

/// The table row of the input written `name` (case ignored), if any.
#[must_use]
pub fn named_pad(name: &str) -> Option<&'static NamedPad> {
    let name = name.trim();
    PADS.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

/// The bus-input prefix of a pad input's value (`pad@LeftStickX`).
pub const PAD_INPUT_TAG: &str = "pad@";

/// The largest dead zone a pad block takes (a stick that moves at all must
/// still reach 1).
pub const MAX_DEAD_ZONE: f32 = 0.9;

/// The dead zone a new pad block starts with: a worn stick rests within it.
pub const DEFAULT_DEAD_ZONE: f32 = 0.1;

/// `dead_zone` held to `0..=MAX_DEAD_ZONE` (a non-finite one is 0).
#[must_use]
pub fn sanitized_dead_zone(dead_zone: f32) -> f32 {
    if dead_zone.is_finite() {
        dead_zone.clamp(0.0, MAX_DEAD_ZONE)
    } else {
        0.0
    }
}

/// The bus input a pad block reads for the input written `name` with
/// `dead_zone`: `pad@<name>`, then `/<dead zone>` unless it is 0. An
/// unknown name keeps its spelling (it never reads anything but 0).
#[must_use]
pub fn pad_input_name(name: &str, dead_zone: f32) -> String {
    let canonical = named_pad(name).map_or(name.trim(), |p| p.name);
    let dead_zone = sanitized_dead_zone(dead_zone);
    if dead_zone > 0.0 {
        format!("{PAD_INPUT_TAG}{canonical}/{dead_zone}")
    } else {
        format!("{PAD_INPUT_TAG}{canonical}")
    }
}

/// The input and dead zone a bus input reads, if it is a known pad input
/// (`"pad@LeftStickX/0.1"` → the left stick's x, 0.1).
#[must_use]
pub fn parse_pad_input(input: &str) -> Option<(&'static NamedPad, f32)> {
    let rest = input.strip_prefix(PAD_INPUT_TAG)?;
    let (name, dead_zone) = match rest.split_once('/') {
        Some((name, dz)) => (name, dz.parse::<f32>().ok()?),
        None => (rest, 0.0),
    };
    Some((named_pad(name)?, sanitized_dead_zone(dead_zone)))
}

/// `value` past a dead zone: 0 within it, then rescaled so the rest of the
/// travel still spans 0..1 (sign kept). Pure.
#[must_use]
pub fn apply_dead_zone(value: f32, dead_zone: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let dead_zone = sanitized_dead_zone(dead_zone);
    let past = (value.abs() - dead_zone).max(0.0) / (1.0 - dead_zone);
    past.min(1.0).copysign(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_through_the_bus_input() {
        assert_eq!(pad_input_name("leftstickx", 0.0), "pad@LeftStickX");
        assert_eq!(pad_input_name("South", 0.25), "pad@South/0.25");
        let (pad, dz) = parse_pad_input("pad@LeftStickX/0.25").unwrap_or_else(|| panic!());
        assert_eq!(pad.name, "LeftStickX");
        assert!((dz - 0.25).abs() < 1e-6);
        assert_eq!(parse_pad_input("pad@Nope"), None);
        assert_eq!(parse_pad_input("key@E"), None);
    }

    #[test]
    fn the_dead_zone_rescales_the_rest_of_the_travel() {
        assert_eq!(apply_dead_zone(0.05, 0.1), 0.0);
        assert!((apply_dead_zone(0.55, 0.1) - 0.5).abs() < 1e-6);
        assert!((apply_dead_zone(-1.0, 0.1) + 1.0).abs() < 1e-6);
        assert_eq!(apply_dead_zone(0.3, 0.0), 0.3);
        assert_eq!(apply_dead_zone(f32::NAN, 0.1), 0.0);
    }

    #[test]
    fn every_name_is_unique() {
        for (i, a) in PADS.iter().enumerate() {
            for b in &PADS[i + 1..] {
                assert!(!a.name.eq_ignore_ascii_case(b.name), "{}", a.name);
            }
        }
    }
}
