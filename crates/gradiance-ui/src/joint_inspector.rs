//! The joint inspector: configure the selected joint's limits, motor, and
//! collision flag.
//!
//! Shown when a joint is selected (body selection is cleared then). Every
//! edit builds a new [`JointDef`] and emits a `PropertyEditIntent`
//! carrying [`PropertyValue::Joint`] — the same undoable path body edits
//! use, so joint configuration composes with undo/redo for free.

use crate::labels::body_label;
use crate::widgets;
use crate::widgets::{Commit, precise_drag, precise_drag_unit};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_command::intent::PropertyEditIntent;
use gradiance_command::property::{PropertyChange, PropertyValue};
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::Body;
use gradiance_domain::joint::{
    AngularMotorDef, DEFAULT_BREAK_FORCE, JointCommon, JointDef, JointKind, LinearMotorDef,
};
use gradiance_domain::settings::SimSettings;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::signal::SignalBindings;
use gradiance_domain::tuning;
use gradiance_interaction::selection::SelectedJoint;
use gradiance_op::{InvokeOp, json};
use gradiance_physics::queries::PhysicsQueries;
use gradiance_units::{Angle, Length};
use gradiance_units::{Dimension, Force};

/// Hinge motor speeds are authored in rad/s (SI) but shown in **rpm**, the
/// familiar motor unit: `rpm = rad/s · 60 / 2π`.
const RAD_PER_S_TO_RPM: f32 = 60.0 / std::f32::consts::TAU;

/// Renders the joint inspector for the currently selected joint.
#[expect(clippy::too_many_arguments, reason = "one system param per read")]
pub fn joint_inspector(
    mut contexts: EguiContexts,
    selected: Res<SelectedJoint>,
    joints: Query<(&StableId, &JointDef)>,
    transforms: Query<&Transform, With<Body>>,
    shapes: Query<&ShapeDef, With<Body>>,
    index: Res<IdIndex>,
    physics: PhysicsQueries,
    fixed: Res<Time<Fixed>>,
    sim: Res<SimSettings>,
    kinds: Query<&avian2d::prelude::RigidBody>,
    mut bindings: ResMut<SignalBindings>,
    mut edits: MessageWriter<PropertyEditIntent>,
    mut invoke: MessageWriter<InvokeOp>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let Some(entity) = selected.0 else {
        return Ok(());
    };
    let Ok((&id, def)) = joints.get(entity) else {
        return Ok(());
    };

    // Edits accumulate into `next`; a single change emits one intent.
    let old = def.clone();
    let mut next = old.clone();
    let mut changed = false;

    egui::Window::new("Joint")
        .default_width(240.0)
        .anchor(egui::Align2::RIGHT_TOP, [-8.0, 8.0])
        .show(ctx, |ui| {
            widgets::section_header(ui, kind_name(&def.kind));
            let body = |b: StableId| {
                let shape = index.entity(b).and_then(|e| shapes.get(e).ok());
                body_label(shape, &b)
            };
            let target = def.body_b.map_or_else(|| "world pin".to_owned(), body);
            ui.label(egui::RichText::new(format!("{} to {target}", body(def.body_a))).weak());
            ui.separator();

            // Sensor ports: the joint's live length/angle (read through the
            // physics facade), each with the same plot toggle bodies have.
            widgets::section_header(ui, "Sensors")
                .on_hover_text("live read-only quantities — the joint's sensor ports");
            crate::ports::sensor_rows(
                ui,
                id,
                crate::ports::joint_sensors(id),
                &index,
                &physics,
                &transforms,
                fixed.timestep().as_secs_f32().max(1e-6),
                &mut bindings,
            );
            ui.separator();

            // Struts collide by default and are governed by the collision-layer
            // menu, so they don't carry a per-joint collide toggle.
            if !matches!(def.kind, JointKind::Spring { .. }) {
                let mut collide = def.common.collide_connected;
                if ui
                    .checkbox(&mut collide, "connected bodies collide")
                    .changed()
                {
                    next.common.collide_connected = collide;
                    changed = true;
                }
            }

            // A fresh break force scales with the weight the joint carries:
            // the masses of its moving bodies (brief 4b-4).
            let moving_mass: f32 = def
                .referenced_bodies()
                .filter_map(|b| index.entity(b))
                .filter(|&e| {
                    kinds
                        .get(e)
                        .is_ok_and(avian2d::prelude::RigidBody::is_dynamic)
                })
                .filter_map(|e| physics.mass_of(e))
                .map(gradiance_units::Mass::value)
                .sum();
            let fresh_break = if moving_mass > 0.0 {
                tuning::default_break_force(moving_mass, sim.gravity.value().length())
            } else {
                DEFAULT_BREAK_FORCE
            };
            if let Some(common) = break_section(ui, def.common, fresh_break) {
                next.common = common;
                changed = true;
            }

            // An auto motor shows the ceiling the physics seam derived.
            let ceiling = physics.motor_ceiling(entity);
            changed |= configure_kind(ui, &def.kind, &mut next, ceiling);

            ui.separator();
            if ui.button("Delete joint").clicked() {
                invoke.write(InvokeOp::with(
                    "joint.delete",
                    json!({ "i": id.0.to_string() }),
                ));
            }
        });

    if changed && next != old {
        edits.write(PropertyEditIntent {
            changes: vec![PropertyChange {
                id,
                old: PropertyValue::Joint(old),
                new: PropertyValue::Joint(next),
            }],
        });
    }
    Ok(())
}

/// Renders the kind-specific config controls, writing any edit into `next.kind`.
/// `ceiling` is the live motor's derived torque/force cap (shown while the
/// motor is auto). Returns whether something changed.
fn configure_kind(
    ui: &mut egui::Ui,
    kind: &JointKind,
    next: &mut JointDef,
    ceiling: Option<f32>,
) -> bool {
    let mut changed = false;
    match kind {
        JointKind::Hinge { limits, motor } => {
            if let Some(k) =
                limit_section(ui, raw(*limits, Angle::value), "angle limits (deg)", true)
            {
                next.kind = JointKind::Hinge {
                    limits: k.map(|[lo, hi]| [Angle(lo), Angle(hi)]),
                    motor: *motor,
                };
                changed = true;
            }
            if let Some(m) =
                motor_section(ui, motor.map(MotorView::from), &MotorUnits::HINGE, ceiling)
            {
                // Re-read limits from `next` in case both changed.
                let cur_limits = current_limits(&next.kind);
                next.kind = JointKind::Hinge {
                    limits: cur_limits.map(|[lo, hi]| [Angle(lo), Angle(hi)]),
                    motor: m.map(MotorView::into_angular),
                };
                changed = true;
            }
        }
        JointKind::Slider {
            axis,
            limits,
            motor,
        } => {
            if let Some(k) =
                limit_section(ui, raw(*limits, Length::value), "travel limits (m)", false)
            {
                next.kind = JointKind::Slider {
                    axis: *axis,
                    limits: k.map(|[lo, hi]| [Length(lo), Length(hi)]),
                    motor: *motor,
                };
                changed = true;
            }
            if let Some(m) =
                motor_section(ui, motor.map(MotorView::from), &MotorUnits::SLIDER, ceiling)
            {
                let cur_limits = current_limits(&next.kind);
                next.kind = JointKind::Slider {
                    axis: *axis,
                    limits: cur_limits.map(|[lo, hi]| [Length(lo), Length(hi)]),
                    motor: m.map(MotorView::into_linear),
                };
                changed = true;
            }
        }
        JointKind::Spring {
            rest_length,
            frequency,
            damping_ratio,
            range,
        } => {
            if let Some(new_kind) = spring_section(
                ui,
                rest_length.value(),
                frequency.value(),
                *damping_ratio,
                raw(*range, Length::value),
            ) {
                next.kind = new_kind;
                changed = true;
            }
        }
    }
    changed
}

/// A human label for a joint kind, shared with the context menu.
pub fn kind_name(kind: &JointKind) -> &'static str {
    match kind {
        JointKind::Hinge { .. } => "Hinge (revolute)",
        JointKind::Slider { .. } => "Prismatic (slider)",
        JointKind::Spring { .. } => "Spring (spring-damper)",
    }
}

/// The current limits' base-SI values (radians for a hinge, metres for a
/// slider) — the units the limit widgets edit in.
fn current_limits(kind: &JointKind) -> Option<[f32; 2]> {
    match kind {
        JointKind::Hinge { limits, .. } => raw(*limits, Angle::value),
        JointKind::Slider { limits, .. } => raw(*limits, Length::value),
        JointKind::Spring { .. } => None,
    }
}

/// A typed `[min, max]` pair's base-SI values (what the drag widgets edit).
fn raw<T: Copy>(pair: Option<[T; 2]>, si: fn(T) -> f32) -> Option<[f32; 2]> {
    pair.map(|[lo, hi]| [si(lo), si(hi)])
}
/// Limits UI: a toggle plus min/max drags. `degrees` shows/edits in
/// degrees but stores radians.
///
/// Returns `Some(new)` only on the frame something changed; the inner
/// option is the new limits (`None` = limits disabled).
#[allow(clippy::option_option)]
fn limit_section(
    ui: &mut egui::Ui,
    current: Option<[f32; 2]>,
    label: &str,
    degrees: bool,
) -> Option<Option<[f32; 2]>> {
    let mut enabled = current.is_some();
    let mut result = None;
    if ui.checkbox(&mut enabled, label).changed() {
        result = Some(if enabled {
            Some(if degrees {
                [-90_f32.to_radians(), 90_f32.to_radians()]
            } else {
                // Travel limits are metres now (was a ±100 px default).
                [-1.0, 1.0]
            })
        } else {
            None
        });
    }
    if let Some([min, max]) = current {
        let scale = if degrees { 1_f32.to_degrees() } else { 1.0 };
        // Angle limits display in degrees (fine at 1°/px); travel limits are
        // metres and need the metre-scale sensitivity + unit.
        let (speed, unit) = if degrees {
            (1.0, "°")
        } else {
            (0.01, Dimension::Length.symbol())
        };
        let (mut lo, mut hi) = (min * scale, max * scale);
        ui.horizontal(|ui| {
            ui.label("min");
            let clo = precise_drag_unit(ui, egui::Id::new("jlim-lo"), &mut lo, 0.0, speed, unit);
            ui.label("max");
            let chi = precise_drag_unit(ui, egui::Id::new("jlim-hi"), &mut hi, 0.0, speed, unit);
            if matches!(clo, Commit::Done(..)) || matches!(chi, Commit::Done(..)) {
                result = Some(Some([lo / scale, hi / scale]));
            }
        });
    }
    result
}

/// Breaking-limit UI: a "breakable" toggle plus the break-force field (edit
/// seam: one undo step per gesture). A broken joint says so; undo is the
/// repair. Returns the new common parameters only on the frame one changed.
fn break_section(ui: &mut egui::Ui, current: JointCommon, fresh: f32) -> Option<JointCommon> {
    let mut result = None;
    let mut breakable = current.break_force.is_some();
    if ui
        .checkbox(&mut breakable, "breakable")
        .on_hover_text("starts at ten times the weight of the connected bodies")
        .changed()
    {
        result = Some(JointCommon {
            break_force: breakable.then_some(Force(fresh)),
            ..current
        });
    }
    if let Some(limit) = current.break_force {
        ui.horizontal(|ui| {
            ui.label("break force");
            let mut value = limit.value();
            if let Commit::Done(..) = precise_drag_unit(
                ui,
                egui::Id::new("j-break"),
                &mut value,
                fresh,
                // A drag moves about 1% of the fresh value per point, so a
                // pebble's and a truck's joints edit alike.
                (fresh * 0.01).max(1e-3),
                Dimension::Force.symbol(),
            ) {
                result = Some(JointCommon {
                    break_force: Some(Force(value.max(0.0))),
                    ..current
                });
            }
        });
    }
    if current.broken {
        ui.label(egui::RichText::new("broken (undo the run to repair)").weak());
    }
    result
}

/// A motor's editable scalars in unit-agnostic form.
///
/// The angular and linear motors are deliberately distinct domain types (they
/// carry different quantities), but their *widget* is identical — only the unit
/// labels differ. This view is the one place the two meet, so `motor_section`
/// stays a single implementation instead of a copy per kind.
#[derive(Clone, Copy)]
struct MotorView {
    /// Base-SI magnitude (rad/s or m/s).
    target_velocity: f32,
    /// Base-SI magnitude (N·m or N); `<= 0` = auto.
    max_effort: f32,
    damping: f32,
    oscillate: bool,
    enabled: bool,
}

/// How one motor kind is shown: its units, its drag step and the motor a
/// fresh toggle creates.
struct MotorUnits {
    /// The shown velocity unit.
    vel_unit: &'static str,
    /// SI velocity → shown unit.
    vel_scale: f32,
    /// Drag step of the velocity field, in the shown unit.
    vel_step: f32,
    /// "torque" or "force".
    effort_word: &'static str,
    /// The effort's unit symbol.
    effort_unit: &'static str,
    /// A fresh motor of this kind.
    fresh: fn() -> MotorView,
}

impl MotorUnits {
    /// A hinge motor: rpm in whole-rpm steps, torque in N·m.
    const HINGE: Self = Self {
        vel_unit: "rpm",
        vel_scale: RAD_PER_S_TO_RPM,
        vel_step: 1.0,
        effort_word: "torque",
        effort_unit: Dimension::Torque.symbol(),
        fresh: || MotorView::from(AngularMotorDef::default()),
    };
    /// A slider motor: m/s in 0.1 m/s steps, force in N.
    const SLIDER: Self = Self {
        vel_unit: Dimension::Velocity.symbol(),
        vel_scale: 1.0,
        vel_step: 0.1,
        effort_word: "force",
        effort_unit: Dimension::Force.symbol(),
        fresh: || MotorView::from(LinearMotorDef::default()),
    };
}

impl MotorView {
    fn into_angular(self) -> AngularMotorDef {
        AngularMotorDef {
            target_velocity: gradiance_units::AngularVelocity(self.target_velocity),
            max_torque: gradiance_units::Torque(self.max_effort),
            damping: self.damping,
            oscillate: self.oscillate,
            enabled: self.enabled,
        }
    }

    fn into_linear(self) -> LinearMotorDef {
        LinearMotorDef {
            target_velocity: gradiance_units::Velocity(self.target_velocity),
            max_force: gradiance_units::Force(self.max_effort),
            damping: self.damping,
            oscillate: self.oscillate,
            enabled: self.enabled,
        }
    }
}

impl From<AngularMotorDef> for MotorView {
    fn from(m: AngularMotorDef) -> Self {
        Self {
            target_velocity: m.target_velocity.value(),
            max_effort: m.max_torque.value(),
            damping: m.damping,
            oscillate: m.oscillate,
            enabled: m.enabled,
        }
    }
}

impl From<LinearMotorDef> for MotorView {
    fn from(m: LinearMotorDef) -> Self {
        Self {
            target_velocity: m.target_velocity.value(),
            max_effort: m.max_force.value(),
            damping: m.damping,
            oscillate: m.oscillate,
            enabled: m.enabled,
        }
    }
}

/// Motor UI: toggle plus parameters.
///
/// Returns `Some(new)` only on the frame something changed; the inner
/// option is the new motor (`None` = motor removed).
#[allow(clippy::option_option)]
fn motor_section(
    ui: &mut egui::Ui,
    current: Option<MotorView>,
    units: &MotorUnits,
    ceiling: Option<f32>,
) -> Option<Option<MotorView>> {
    let MotorUnits {
        vel_unit,
        vel_scale,
        vel_step,
        effort_word,
        effort_unit,
        fresh,
    } = *units;
    let mut enabled = current.is_some();
    let mut result = None;
    if ui.checkbox(&mut enabled, "motor").changed() {
        // A new motor is "auto": the physics seam sizes its ceiling to the
        // driven body's real inertia/mass (`max_effort <= 0`).
        result = Some(enabled.then(fresh));
    }
    if let Some(mut m) = current {
        ui.horizontal(|ui| {
            ui.label("target");
            // `target_velocity` is stored in SI (rad/s or m/s); `vel_scale`
            // converts to the shown unit (e.g. rpm for hinges) and back.
            let mut shown = m.target_velocity * vel_scale;
            if let Commit::Done(..) = precise_drag_unit(
                ui,
                egui::Id::new("jm-vel"),
                &mut shown,
                fresh().target_velocity * vel_scale,
                vel_step,
                vel_unit,
            ) {
                m.target_velocity = shown / vel_scale;
                result = Some(Some(m));
            }
        });
        ui.horizontal(|ui| {
            // `max_effort <= 0` = auto (physics scales it to the body). An
            // auto field shows the ceiling the physics seam derived, so a
            // drag starts from it and pins an explicit cap; middle-click
            // resets to 0, auto again.
            let auto = m.max_effort <= 0.0;
            ui.label(if auto {
                format!("max {effort_word} (auto)")
            } else {
                format!("max {effort_word}")
            });
            let mut shown = if auto {
                ceiling.unwrap_or(0.0)
            } else {
                m.max_effort
            };
            if let Commit::Done(..) = precise_drag_unit(
                ui,
                egui::Id::new("jm-force"),
                &mut shown,
                0.0,
                10.0,
                effort_unit,
            ) {
                m.max_effort = shown;
                result = Some(Some(m));
            }
        });
        let mut osc = m.oscillate;
        if ui.checkbox(&mut osc, "oscillate at limits").changed() {
            m.oscillate = osc;
            result = Some(Some(m));
        }
        let mut powered = m.enabled;
        if ui.checkbox(&mut powered, "powered").changed() {
            m.enabled = powered;
            result = Some(Some(m));
        }
    }
    result
}

/// Strut config: rest length, natural frequency, damping ratio, and an
/// optional hard range clamp (off by default = unbounded). Frequency and
/// ratio are mass-relative (brief 4b-4): the physics seam derives the
/// stiffness from the bodies' mass, so these feel the same on any body.
/// Each commits clamped to its `tuning` range. Returns the new `Spring` kind
/// only on the frame something committed.
fn spring_section(
    ui: &mut egui::Ui,
    rest_length: f32,
    frequency: f32,
    damping_ratio: f32,
    range: Option<[f32; 2]>,
) -> Option<JointKind> {
    let mut rest = rest_length;
    let mut f = frequency;
    let mut zeta = damping_ratio;
    let mut new_range = range;
    let mut edited = false;

    ui.horizontal(|ui| {
        ui.label("rest length");
        if let Commit::Done(..) = precise_drag_unit(
            ui,
            egui::Id::new("spring-rest"),
            &mut rest,
            rest_length,
            0.01,
            Dimension::Length.symbol(),
        ) {
            edited = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("frequency")
            .on_hover_text("bounces per second, whatever the bodies weigh");
        if let Commit::Done(..) = precise_drag_unit(
            ui,
            egui::Id::new("spring-f"),
            &mut f,
            tuning::SPRING_FREQUENCY.default,
            0.01,
            Dimension::Frequency.symbol(),
        ) {
            edited = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("damping ratio")
            .on_hover_text("0 bounces forever; 1 settles without overshoot");
        if let Commit::Done(..) = precise_drag(
            ui,
            egui::Id::new("spring-zeta"),
            &mut zeta,
            tuning::SPRING_DAMPING_RATIO.default,
            0.005,
        ) {
            edited = true;
        }
    });

    // Optional hard length clamp; off by default (unbounded travel).
    let mut clamped = new_range.is_some();
    if ui.checkbox(&mut clamped, "clamp length range").changed() {
        new_range = clamped.then(|| [0.0, (rest_length * 2.0).max(0.1)]);
        edited = true;
    }
    if let Some(bounds) = new_range.as_mut() {
        let m = Dimension::Length.symbol();
        ui.horizontal(|ui| {
            ui.label("min");
            if let Commit::Done(..) =
                precise_drag_unit(ui, egui::Id::new("spring-lo"), &mut bounds[0], 0.0, 0.01, m)
            {
                edited = true;
            }
            ui.label("max");
            if let Commit::Done(..) = precise_drag_unit(
                ui,
                egui::Id::new("spring-hi"),
                &mut bounds[1],
                rest_length,
                0.01,
                m,
            ) {
                edited = true;
            }
        });
        if bounds[0] > bounds[1] {
            bounds.swap(0, 1);
        }
    }

    if !edited {
        return None;
    }
    Some(JointKind::Spring {
        rest_length: Length(rest.max(0.0)),
        frequency: gradiance_units::Frequency(tuning::SPRING_FREQUENCY.clamp(f)),
        damping_ratio: tuning::SPRING_DAMPING_RATIO.clamp(zeta),
        range: new_range.map(|[lo, hi]| [Length(lo), Length(hi)]),
    })
}

#[cfg(test)]
mod motor_display_tests {
    use super::*;
    use egui_kittest::kittest::{NodeT, Queryable};

    /// The numbers the motor section's fields show, in order.
    fn shown(current: MotorView, units: &MotorUnits, ceiling: Option<f32>) -> Vec<f64> {
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            motor_section(ui, Some(current), units, ceiling);
        });
        harness.run();
        harness
            .query_all_by_role(egui::accesskit::Role::SpinButton)
            .filter_map(|n| n.accesskit_node().numeric_value())
            .collect()
    }

    /// A fresh hinge motor reads a round `20 rpm`, and while its torque is
    /// auto the field shows the ceiling the physics seam derived, not `0`
    /// (A-1 finding 12, brief A-2). An explicit cap shows itself.
    #[test]
    fn a_fresh_hinge_motor_reads_20_rpm_and_its_derived_torque() {
        let fresh = (MotorUnits::HINGE.fresh)();
        let values = shown(fresh, &MotorUnits::HINGE, Some(333.0));
        assert_eq!(values.len(), 2, "{values:?}");
        assert!((values[0] - 20.0).abs() < 1e-3, "{values:?}");
        assert!((values[1] - 333.0).abs() < 1e-3, "{values:?}");
        let pinned = MotorView {
            max_effort: 50.0,
            ..fresh
        };
        let values = shown(pinned, &MotorUnits::HINGE, Some(333.0));
        assert!((values[1] - 50.0).abs() < 1e-3, "{values:?}");
        // A fresh slider motor keeps its own default (2 m/s), not the hinge's.
        let slider = (MotorUnits::SLIDER.fresh)();
        assert!((slider.target_velocity - 2.0).abs() < 1e-6);
    }
}
