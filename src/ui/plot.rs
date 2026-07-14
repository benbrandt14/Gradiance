//! The live plotter: a time-series panel for the selected body's or joint's
//! physics.
//!
//! A pure *read* — the plotter introspects `physics::queries` and `Transform`
//! and records a rolling history; it never mutates authored state (invariant
//! #4). This is the visualization half of the read-total governance model
//! (`docs/script-lisp-decision.md` §"Live plotters"): a plotter is just another
//! reader of the same facade scripts read through.
//!
//! The history is a set of **named signals**, so adding a plottable quantity is
//! a one-line change and the model is ready to back the future `(measure …)`
//! data-out seam. `sample_plot` fills it each frame while playing; `plot_panel`
//! (backslash) draws it, hand-rendered with the egui painter — no plotting
//! dependency.

use crate::core::ids::IdIndex;
use crate::core::units::PosRot;
use crate::domain::joint::{JointDef, JointKind};
use crate::interaction::selection::{SelectedJoint, Selection};
use crate::physics::queries::PhysicsQueries;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::collections::VecDeque;

/// One named live signal — a label plus its rolling samples.
struct Signal {
    label: &'static str,
    samples: VecDeque<f32>,
}

/// Rolling live-signal history for the plot panel. Records the currently
/// selected body *or* joint — or, when nothing is selected and the sim is
/// running, a particle group's aggregates; changing what (or which signals)
/// is tracked resets it, so a plot never mixes two sources' histories.
#[derive(Resource, Default)]
pub struct PlotHistory {
    tracked: Option<Entity>,
    /// The particle group being plotted (when no body/joint is selected).
    /// Mutually exclusive with `tracked`. Only meaningful with the sim
    /// feature (particles don't exist otherwise).
    #[cfg(feature = "sim")]
    tracked_group: Option<u32>,
    signals: Vec<Signal>,
}

impl PlotHistory {
    /// Samples kept per signal (~10 s at 60 fps).
    const CAP: usize = 600;

    /// Points the history at `entity` with the given signal labels, rebuilding
    /// (and clearing) only when the target or the signal set actually changed.
    fn retarget(&mut self, entity: Entity, labels: &[&'static str]) {
        let unchanged = self.tracked == Some(entity)
            && self.signals.len() == labels.len()
            && self
                .signals
                .iter()
                .zip(labels)
                .all(|(signal, label)| signal.label == *label);
        if !unchanged {
            self.tracked = Some(entity);
            #[cfg(feature = "sim")]
            {
                self.tracked_group = None;
            }
            self.rebuild_signals(labels);
        }
    }

    /// Points the history at particle `group` (when no body/joint is
    /// selected), rebuilding only when the group or signal set changed.
    #[cfg(feature = "sim")]
    fn retarget_group(&mut self, group: u32, labels: &[&'static str]) {
        let unchanged = self.tracked_group == Some(group)
            && self.signals.len() == labels.len()
            && self
                .signals
                .iter()
                .zip(labels)
                .all(|(signal, label)| signal.label == *label);
        if !unchanged {
            self.tracked = None;
            self.tracked_group = Some(group);
            self.rebuild_signals(labels);
        }
    }

    /// (Re)creates the empty signal set for `labels`.
    fn rebuild_signals(&mut self, labels: &[&'static str]) {
        self.signals = labels
            .iter()
            .map(|&label| Signal {
                label,
                samples: VecDeque::new(),
            })
            .collect();
    }

    /// Appends one sample per signal (values in signal order).
    fn push(&mut self, values: &[f32]) {
        for (signal, &value) in self.signals.iter_mut().zip(values) {
            signal.samples.push_back(value);
            while signal.samples.len() > Self::CAP {
                signal.samples.pop_front();
            }
        }
    }

    /// Forgets the tracked source and its signals (nothing plottable selected).
    fn clear(&mut self) {
        self.tracked = None;
        #[cfg(feature = "sim")]
        {
            self.tracked_group = None;
        }
        self.signals.clear();
    }
}

/// Plot panel visibility.
#[derive(Resource, Default)]
pub struct PlotPanel {
    open: bool,
}

impl PlotPanel {
    /// Whether the panel is currently shown (read by the transport button so it
    /// can reflect the open state).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Flips the panel's visibility. Both the `\` shortcut and the transport
    /// button toggle through here.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// Colours cycled across a source's signals.
const SIGNAL_COLORS: [egui::Color32; 4] = [
    egui::Color32::from_rgb(120, 200, 255),
    egui::Color32::from_rgb(255, 190, 120),
    egui::Color32::from_rgb(150, 230, 150),
    egui::Color32::from_rgb(230, 150, 230),
];

/// Samples the selected body's or joint's live signals into the history. Gated
/// on `Playing`, so a pause freezes the plot instead of scrolling flat lines.
pub fn sample_plot(
    selection: Res<Selection>,
    selected_joint: Res<SelectedJoint>,
    joints: Query<&JointDef>,
    transforms: Query<&Transform>,
    index: Res<IdIndex>,
    physics: PhysicsQueries,
    mut history: ResMut<PlotHistory>,
    #[cfg(feature = "sim")] particles: Option<Res<crate::sim::bridge::Particles>>,
    #[cfg(feature = "sim")] groups: Option<Res<crate::sim::groups::ParticleGroups>>,
) {
    // A selected joint takes precedence (body/joint selection are exclusive):
    // plot its current length, and for a hinge its relative angle too.
    if let Some(joint_entity) = selected_joint.0 {
        if let Ok(def) = joints.get(joint_entity)
            && let Some((length, angle)) = joint_signals(def, &transforms, &index)
        {
            if matches!(def.kind, JointKind::Hinge { .. }) {
                history.retarget(joint_entity, &["length (px)", "angle (deg)"]);
                history.push(&[length, angle]);
            } else {
                history.retarget(joint_entity, &["length (px)"]);
                history.push(&[length]);
            }
        } else {
            history.clear();
        }
        return;
    }

    // Otherwise, a single selected body: speed + height.
    let mut selected = selection.iter();
    if let (Some(entity), None) = (selected.next(), selected.next()) {
        let height = transforms.get(entity).map_or(0.0, |t| t.translation.y);
        let speed = physics.velocity_of(entity).map_or(0.0, |(v, _)| v.length());
        history.retarget(entity, &["speed (px/s)", "height (px)"]);
        history.push(&[speed, height]);
        return;
    }

    // Nothing (or a multi-selection) picked. When the sim is running, fall
    // back to the most-populous particle group's aggregates — the "plotters
    // show average position/velocity" ask. Same read-facade philosophy; a
    // plotter is just another reader.
    #[cfg(feature = "sim")]
    if let (Some(particles), Some(groups)) = (particles, groups)
        && let Some((group, agg)) = largest_group_aggregate(&particles.0, &groups)
    {
        history.retarget_group(group, &["count", "avg speed (px/s)", "centroid y (px)"]);
        history.push(&[agg.count as f32, agg.mean_velocity.length(), agg.centroid.y]);
        return;
    }
    history.clear();
}

/// The particle group with the most live members, and its aggregate — the
/// group the plotter summarizes when nothing is selected. `None` if there
/// are no particles.
#[cfg(feature = "sim")]
fn largest_group_aggregate(
    state: &crate::sim::kernel::ParticleState,
    groups: &crate::sim::groups::ParticleGroups,
) -> Option<(u32, crate::sim::kernel::GroupAggregate)> {
    (0..groups.0.len() as u32)
        .filter_map(|g| state.aggregate(g).map(|a| (g, a)))
        .max_by_key(|(_, a)| a.count)
}

/// A joint's live geometry: current anchor-to-anchor length and the relative
/// angle of its bodies (degrees). `None` if an endpoint can't be resolved.
fn joint_signals(
    def: &JointDef,
    transforms: &Query<&Transform>,
    index: &IdIndex,
) -> Option<(f32, f32)> {
    let pose_a = index
        .entity(def.body_a)
        .and_then(|e| transforms.get(e).ok())
        .map(PosRot::from_transform)?;
    let world_a = def.anchor_world(pose_a.pos, pose_a.rot);
    let (world_b, rot_b) = match def.body_b {
        Some(id) => {
            let pose_b = index
                .entity(id)
                .and_then(|e| transforms.get(e).ok())
                .map(PosRot::from_transform)?;
            (
                pose_b.pos + Vec2::from_angle(pose_b.rot).rotate(def.anchor_b),
                pose_b.rot,
            )
        }
        None => (def.anchor_b, 0.0), // world pin
    };
    Some((world_a.distance(world_b), (rot_b - pose_a.rot).to_degrees()))
}

/// Renders the live-plot panel (toggle with backslash).
pub fn plot_panel(
    mut contexts: EguiContexts,
    mut panel: ResMut<PlotPanel>,
    history: Res<PlotHistory>,
    keys: Res<ButtonInput<KeyCode>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if keys.just_pressed(KeyCode::Backslash) && !ctx.egui_wants_keyboard_input() {
        panel.toggle();
    }
    if !panel.open {
        return Ok(());
    }

    let mut open = true;
    egui::Window::new("Live Plot")
        .open(&mut open)
        .default_width(320.0)
        .show(ctx, |ui| {
            if history.signals.is_empty() {
                ui.label(
                    "Select one body (speed, height) or a joint (length, angle) and press \
                     Play. With no selection, a running particle group plots its aggregates.",
                );
                return;
            }
            for (i, signal) in history.signals.iter().enumerate() {
                if i > 0 {
                    ui.add_space(4.0);
                }
                draw_series(
                    ui,
                    signal.label,
                    &signal.samples,
                    SIGNAL_COLORS[i % SIGNAL_COLORS.len()],
                );
            }
        });
    panel.open = open;
    Ok(())
}

/// Hand-draws one signal as a line plot auto-scaled to its own min/max.
/// Host-agnostic (pure `Ui` + data in) so `tests/it/ui_panels.rs` can
/// exercise it under `egui_kittest`.
pub fn draw_series(ui: &mut egui::Ui, label: &str, data: &VecDeque<f32>, color: egui::Color32) {
    ui.label(label);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 70.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(24));
    if data.len() < 2 {
        return;
    }

    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for &v in data {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if !(hi - lo).is_finite() || (hi - lo).abs() < 1e-6 {
        hi = lo + 1.0;
    }

    let n = data.len();
    let map = |i: usize, v: f32| {
        let x = rect.left() + rect.width() * (i as f32 / (n - 1) as f32);
        let y = rect.bottom() - rect.height() * ((v - lo) / (hi - lo));
        egui::pos2(x, y)
    };
    let stroke = egui::Stroke::new(1.5, color);
    let mut prev = map(0, data[0]);
    for (i, &v) in data.iter().enumerate().skip(1) {
        let point = map(i, v);
        painter.line_segment([prev, point], stroke);
        prev = point;
    }

    // Min/max labels in the corners.
    let font = egui::FontId::monospace(9.0);
    painter.text(
        rect.right_top() + egui::vec2(-2.0, 1.0),
        egui::Align2::RIGHT_TOP,
        format!("{hi:.0}"),
        font.clone(),
        egui::Color32::GRAY,
    );
    painter.text(
        rect.right_bottom() + egui::vec2(-2.0, -1.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("{lo:.0}"),
        font,
        egui::Color32::GRAY,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_retargets_caps_and_keeps_matching_target() {
        let mut world = World::new();
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let mut history = PlotHistory::default();

        history.retarget(a, &["x", "y"]);
        for i in 0..(PlotHistory::CAP + 20) {
            history.push(&[i as f32, 0.0]);
        }
        assert_eq!(history.tracked, Some(a));
        assert_eq!(history.signals.len(), 2);
        assert_eq!(history.signals[0].samples.len(), PlotHistory::CAP, "capped");

        // Retargeting to the same target with the same labels keeps the data.
        history.retarget(a, &["x", "y"]);
        assert_eq!(history.signals[0].samples.len(), PlotHistory::CAP);

        // A different target (or signal set) resets.
        history.retarget(b, &["len"]);
        assert_eq!(history.tracked, Some(b));
        assert_eq!(history.signals.len(), 1);
        assert!(history.signals[0].samples.is_empty(), "reset on change");

        history.clear();
        assert_eq!(history.tracked, None);
        assert!(history.signals.is_empty());
    }

    #[cfg(feature = "sim")]
    #[test]
    fn group_tracking_is_exclusive_with_body_tracking() {
        let mut world = World::new();
        let body = world.spawn_empty().id();
        let mut history = PlotHistory::default();

        // Track a group; accumulate a few samples.
        history.retarget_group(2, &["count"]);
        for _ in 0..5 {
            history.push(&[10.0]);
        }
        assert_eq!(history.tracked_group, Some(2));
        assert_eq!(
            history.tracked, None,
            "group and body tracking are exclusive"
        );
        assert_eq!(history.signals[0].samples.len(), 5);

        // Same group + labels keeps the history.
        history.retarget_group(2, &["count"]);
        assert_eq!(
            history.signals[0].samples.len(),
            5,
            "unchanged target keeps data"
        );

        // Switching to a body clears the group tracking and resets.
        history.retarget(body, &["speed (px/s)", "height (px)"]);
        assert_eq!(history.tracked, Some(body));
        assert_eq!(history.tracked_group, None);
        assert!(history.signals[0].samples.is_empty());
    }
}
