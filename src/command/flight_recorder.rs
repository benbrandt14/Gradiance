//! Dev-only flight recorder: a per-frame ring buffer of what the deferred
//! pipeline actually did.
//!
//! The architecture defers everything (intent → dispatch → `Changed<>` sync),
//! so a bug's symptom often lands frames after its cause. The recorder is the
//! compensating instrument: for each of the last [`FlightRecorder::CAP`]
//! frames it captures
//!
//! - every **intent** the dispatcher drained (cloned via reflection — no
//!   per-type code; see [`record_intent`]),
//! - every **command** the stack executed, with its `Debug` detail (which
//!   includes the `StableId` targets) and outcome,
//! - which **sync systems** fired, as the entity count each `Changed<>`
//!   query matched (read-only mirror queries — see [`record_sync_counts`]).
//!
//! It is a **pure reader**: nothing routes through it, it writes no authored
//! state, and it exists only under the `dev` feature. Dump the buffer to RON
//! with **F9** (handled in `persist::flight`, where serialization lives).

use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::joint::JointDef;
use crate::domain::layers::LayerMask32;
use crate::domain::settings::{RenderSettings, SimSettings};
use crate::domain::shape::ShapeDef;
use bevy::prelude::*;
use bevy::reflect::PartialReflect;
use std::collections::VecDeque;

/// One drained intent: its type path plus a reflected clone of its value.
pub struct IntentRecord {
    /// The intent's `TypePath` (e.g. `gradiance::command::intent::CutIntent`).
    pub type_path: &'static str,
    /// Reflected clone of the intent (RON-serialized on dump; `Debug` for logs).
    pub value: Box<dyn PartialReflect>,
}

/// One executed command: its canonical name, `Debug` detail, and outcome.
pub struct CommandRecord {
    /// The command's [`name`](crate::command::intent::name) constant
    /// (e.g. `cut`, `commit-transform`).
    pub name: &'static str,
    /// `Debug` of the command — includes its `StableId` targets.
    pub detail: String,
    /// `Ok`, or the `CommandError`'s display text.
    pub outcome: Result<(), String>,
}

/// Everything recorded for one frame.
#[derive(Default)]
pub struct FrameRecord {
    /// The recorder's own frame counter (monotonic from app start).
    pub frame: u64,
    /// Intents drained by the dispatcher this frame, in drain order.
    pub intents: Vec<IntentRecord>,
    /// Commands executed (applied / undone / redone) this frame, in order.
    pub commands: Vec<CommandRecord>,
    /// `(system name, entities matched)` for each sync system that fired.
    pub syncs: Vec<(&'static str, usize)>,
}

impl FrameRecord {
    /// Whether anything happened this frame.
    pub fn is_empty(&self) -> bool {
        self.intents.is_empty() && self.commands.is_empty() && self.syncs.is_empty()
    }
}

/// The ring buffer of the last [`Self::CAP`] frames. Dev-only, read-mostly;
/// written by the dispatcher and the mirror systems below.
#[derive(Resource, Default)]
pub struct FlightRecorder {
    frames: VecDeque<FrameRecord>,
    frame_counter: u64,
}

impl FlightRecorder {
    /// Frames retained (~4 s at 60 fps) — enough to span a deferred bug's
    /// cause and symptom.
    pub const CAP: usize = 240;

    /// Recorded frames, oldest first (empty frames included).
    pub fn frames(&self) -> impl DoubleEndedIterator<Item = &FrameRecord> {
        self.frames.iter()
    }

    /// The frame currently being recorded.
    fn current(&mut self) -> &mut FrameRecord {
        // begin_frame always pushes before any recording system runs; the
        // fallback only serves a world without the recorder systems (tests).
        if self.frames.is_empty() {
            self.frames.push_back(FrameRecord::default());
        }
        let last = self.frames.len() - 1;
        &mut self.frames[last]
    }

    /// Records one drained intent (a reflected clone — works for every
    /// intent type with zero per-type code).
    pub fn record_intent(&mut self, type_path: &'static str, value: &dyn PartialReflect) {
        self.current().intents.push(IntentRecord {
            type_path,
            value: value.to_dynamic(),
        });
    }

    /// Records one executed command and its outcome.
    pub fn record_command(
        &mut self,
        name: &'static str,
        detail: String,
        outcome: Result<(), String>,
    ) {
        self.current().commands.push(CommandRecord {
            name,
            detail,
            outcome,
        });
    }

    /// Records that a sync system's `Changed<>` query matched `count` entities.
    fn record_sync(&mut self, system: &'static str, count: usize) {
        if count > 0 {
            self.current().syncs.push((system, count));
        }
    }
}

/// Starts a fresh frame record and rotates the ring.
fn begin_frame(mut recorder: ResMut<FlightRecorder>) {
    recorder.frame_counter += 1;
    let frame = recorder.frame_counter;
    recorder
        .frames
        .push_back(FrameRecord { frame, ..default() });
    while recorder.frames.len() > FlightRecorder::CAP {
        recorder.frames.pop_front();
    }
}

/// Logs a one-line summary of any frame that did something (the recorder's
/// live "overlay" — follow with `RUST_LOG=gradiance=debug`).
fn log_active_frames(recorder: Res<FlightRecorder>) {
    let Some(frame) = recorder.frames.back() else {
        return;
    };
    if frame.is_empty() {
        return;
    }
    let intents: Vec<&str> = frame.intents.iter().map(|i| i.type_path).collect();
    let commands: Vec<String> = frame
        .commands
        .iter()
        .map(|c| {
            let ok = if c.outcome.is_ok() { "ok" } else { "ERR" };
            format!("{} ({ok})", c.name)
        })
        .collect();
    debug!(
        frame = frame.frame,
        intents = ?intents,
        commands = ?commands,
        syncs = ?frame.syncs,
        "flight recorder"
    );
}

/// Read-only mirrors of the real sync systems' `Changed<>` queries.
///
/// Change detection is tracked **per system**, so a mirror query with the
/// same filter, running once per frame like the real one, matches exactly the
/// entities the real system will process this frame — without touching the
/// sync systems themselves. Names below are the real systems'.
#[allow(clippy::type_complexity)]
fn record_sync_counts(
    mut recorder: ResMut<FlightRecorder>,
    changed_shapes: Query<(), (With<Body>, Changed<ShapeDef>)>,
    changed_layers: Query<(), (With<Body>, Changed<LayerMask32>)>,
    changed_meshes: Query<(), (With<Body>, Or<(Changed<ShapeDef>, Changed<LayerMask32>)>)>,
    changed_materials: Query<(), (With<Body>, Changed<Appearance>)>,
    changed_joints: Query<(), Changed<JointDef>>,
    sim_settings: Option<Res<SimSettings>>,
    render_settings: Option<Res<RenderSettings>>,
) {
    recorder.record_sync(
        "physics::body_sync::sync_colliders",
        changed_shapes.iter().count(),
    );
    recorder.record_sync(
        "physics::body_sync::sync_collision_layers",
        changed_layers.iter().count(),
    );
    recorder.record_sync(
        "render::extrude_sync::sync_body_meshes",
        changed_meshes.iter().count(),
    );
    recorder.record_sync(
        "render::material_sync::sync_body_materials",
        changed_materials.iter().count(),
    );
    recorder.record_sync(
        "physics::joint_sync::sync_joints",
        changed_joints.iter().count(),
    );
    if sim_settings.is_some_and(|s| s.is_changed()) {
        recorder.record_sync("physics::apply_sim_settings", 1);
    }
    if render_settings.is_some_and(|s| s.is_changed()) {
        recorder.record_sync("render::toon::apply_render_settings", 1);
    }
}

/// Installs the recorder: ring rotation in `First`, sync mirrors in
/// `PostUpdate` (alongside the real sync systems), and the log tail in `Last`.
/// Registered by `CommandPlugin` under the `dev` feature.
pub fn install(app: &mut App) {
    app.init_resource::<FlightRecorder>();
    app.add_systems(First, begin_frame);
    app.add_systems(PostUpdate, record_sync_counts);
    app.add_systems(Last, log_active_frames);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::intent::CutIntent;

    #[test]
    fn ring_buffer_rotates_at_cap() {
        let mut app = App::new();
        app.init_resource::<FlightRecorder>();
        app.add_systems(First, begin_frame);
        for _ in 0..(FlightRecorder::CAP + 10) {
            app.update();
        }
        let recorder = app.world().resource::<FlightRecorder>();
        assert_eq!(recorder.frames.len(), FlightRecorder::CAP);
        let first = recorder.frames().next().map(|f| f.frame);
        assert_eq!(first, Some(11), "oldest frames were evicted");
    }

    #[test]
    fn intents_record_via_reflection() {
        let mut recorder = FlightRecorder::default();
        let intent = CutIntent {
            a: Vec2::ZERO,
            b: Vec2::new(10.0, 0.0),
            width: 4.0,
        };
        recorder.record_intent(
            <CutIntent as bevy::reflect::TypePath>::type_path(),
            intent.as_partial_reflect(),
        );
        let frame = recorder
            .frames()
            .next_back()
            .unwrap_or_else(|| unreachable!());
        assert_eq!(frame.intents.len(), 1);
        assert!(frame.intents[0].type_path.ends_with("CutIntent"));
        // The reflected clone carries the field values (Debug via reflection).
        let debug = format!("{:?}", frame.intents[0].value);
        assert!(
            debug.contains("width"),
            "reflect debug shows fields: {debug}"
        );
    }

    #[test]
    fn sync_mirror_counts_changed_entities() {
        let mut app = App::new();
        app.init_resource::<FlightRecorder>();
        app.add_systems(First, begin_frame);
        app.add_systems(PostUpdate, record_sync_counts);
        app.update(); // settle Added<> ticks for the resources (none here)
        app.world_mut().spawn((
            Body,
            ShapeDef::Box {
                width: 10.0,
                height: 10.0,
            },
        ));
        app.update();
        let recorder = app.world().resource::<FlightRecorder>();
        let frame = recorder
            .frames()
            .next_back()
            .unwrap_or_else(|| unreachable!());
        assert!(
            frame
                .syncs
                .iter()
                .any(|(name, n)| name.ends_with("sync_colliders") && *n == 1),
            "spawn shows up as a Changed<ShapeDef> match: {:?}",
            frame.syncs
        );
    }
}
