//! The tool facade: `ToolContext` (read-side) → `DraftTool` → `(preview,
//! commit-intent)`.
//!
//! Every pure *creation* tool — box, circle, polygon, ground, cut — draws a
//! transient preview while a gesture is in progress and, on completion,
//! emits **exactly one** authoring command. Rather than repeat the
//! press/drag/release plumbing in each tool, they implement one interface:
//!
//! - [`ToolContext`] is everything a tool reads this frame (the gesture
//!   phase, the snapped cursor, gesture constraints, camera scale). A tool
//!   never touches the ECS world directly — it is a pure function of its
//!   own draft state plus this context.
//! - [`DraftTool::update`] advances the draft and returns an optional
//!   [`ToolCommit`] when the gesture completes.
//! - [`DraftTool::preview`] renders the in-progress gizmo from draft state.
//!
//! [`ToolCommit`] is a small **runtime representation** of an authoring
//! action; the generic driver ([`run_draft_tool`]) translates it into the
//! real intent message. This is the same shape the scripting layer will
//! use — a scripted tool is later just another `DraftTool` (eventually a
//! closure) returning `ToolCommit`s, going through the *same* intent seam
//! (see `docs/script-lisp-decision.md`). Tools therefore never get a
//! bespoke mutation path.

use crate::command::intent::{
    CommitTransformIntent, CutIntent, DuplicateIntent, ScaleIntent, SpawnBodyIntent,
    SpawnJointIntent, TransformChange,
};
use crate::command::snapshot::{BodyRecord, JointRecord};
use crate::core::ids::StableId;
use crate::domain::settings::SnapConfig;
use crate::interaction::PointerOverUi;
use crate::interaction::gesture::GestureConstraints;
use crate::interaction::pointer::PointerButtons;
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::ActiveGesture;
use bevy::ecs::component::Mutable;
use bevy::prelude::*;

/// The pointer-gesture phase for the primary (left) button this frame.
///
/// A press over the UI is reported as [`GesturePhase::Idle`] (a tool must
/// not start a gesture on a click that egui owns); a release always
/// reports [`GesturePhase::Released`] regardless of what the cursor is over,
/// so a drag begun on the canvas still commits if it ends over a panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GesturePhase {
    /// No relevant button activity.
    #[default]
    Idle,
    /// The button went down on the canvas this frame.
    Pressed,
    /// The button is held (neither the press nor the release frame).
    Held,
    /// The button came up this frame.
    Released,
}

/// Read-only inputs a [`DraftTool`] consumes for one frame.
///
/// Borrows the shared settings so tool math (snapping, rotation quantize)
/// behaves identically to every other gesture.
pub struct ToolContext<'a> {
    /// This frame's gesture phase.
    pub phase: GesturePhase,
    /// The snapped, effective cursor (grid/object snapped). `None` off-screen.
    pub cursor: Option<Vec2>,
    /// The snapped *raw* cursor (pre-effective) — freeform strokes want this.
    pub raw_cursor: Option<Vec2>,
    /// Whether the pointer is over egui (a tool should not start here).
    pub over_ui: bool,
    /// `Enter` pressed this frame (polygon close).
    pub confirm: bool,
    /// `Escape` pressed this frame (cancel the draft).
    pub cancel: bool,
    /// Axis/rotation gesture constraints.
    pub constraints: &'a GestureConstraints,
    /// Snapping configuration (rotation quantization step).
    pub snap: &'a SnapConfig,
    /// World units per logical screen pixel at the current zoom.
    pub cam_scale: f32,
}

/// A completed authoring action in a front-end-agnostic form.
///
/// The driver turns each variant into the corresponding intent message, so
/// the command discipline is untouched; scripting and a future node editor
/// produce the same values through the same seam. Creation tools emit the
/// first two; manipulation tools (drag/select/connector) emit the rest.
#[derive(Debug, Clone)]
pub enum ToolCommit {
    /// Author a new body (box/circle/polygon/ground).
    SpawnBody(Box<BodyRecord>),
    /// Sever bodies along a stroke.
    Cut {
        /// Stroke start, world space.
        a: Vec2,
        /// Stroke end, world space.
        b: Vec2,
        /// Stroke width, world space.
        width: f32,
    },
    /// Author a new joint (hinge/weld/slider — the connector tools).
    SpawnJoint(Box<JointRecord>),
    /// Commit a completed move/rotate gesture (one undo step for the batch).
    Move(Vec<TransformChange>),
    /// Commit a completed bounding-box scale gesture.
    Scale {
        /// Bodies to scale.
        targets: Vec<StableId>,
        /// Fixed point, world space.
        pivot: Vec2,
        /// Frame rotation: 0 = global axes, body rotation = local axes.
        frame_rot: f32,
        /// Per-axis factors along the frame axes.
        factors: Vec2,
    },
    /// Pattern-copy bodies at an offset (ctrl-drag duplicate).
    Duplicate {
        /// Bodies to clone.
        sources: Vec<StableId>,
        /// World-space offset applied to each clone.
        offset: Vec2,
    },
}

/// One gizmo primitive for a tool's transient preview.
#[derive(Debug, Clone)]
pub enum PreviewPrim {
    /// A straight segment.
    Line {
        /// Start point.
        a: Vec2,
        /// End point.
        b: Vec2,
        /// Stroke color.
        color: Color,
    },
    /// An axis-aligned rectangle by center and full size.
    Rect {
        /// Center point.
        center: Vec2,
        /// Full width/height.
        size: Vec2,
        /// Stroke color.
        color: Color,
    },
    /// A circle by center and radius.
    Circle {
        /// Center point.
        center: Vec2,
        /// Radius.
        radius: f32,
        /// Stroke color.
        color: Color,
    },
    /// An open polyline through the given points.
    Polyline {
        /// Ordered points.
        points: Vec<Vec2>,
        /// Stroke color.
        color: Color,
    },
}

/// A tool's transient preview: a list of gizmo primitives to draw.
#[derive(Debug, Default)]
pub struct ToolPreview {
    /// Primitives, drawn in order.
    pub prims: Vec<PreviewPrim>,
}

impl ToolPreview {
    /// Adds a segment.
    pub fn line(&mut self, a: Vec2, b: Vec2, color: impl Into<Color>) {
        self.prims.push(PreviewPrim::Line {
            a,
            b,
            color: color.into(),
        });
    }

    /// Adds a center/size rectangle.
    pub fn rect(&mut self, center: Vec2, size: Vec2, color: impl Into<Color>) {
        self.prims.push(PreviewPrim::Rect {
            center,
            size,
            color: color.into(),
        });
    }

    /// Adds a circle.
    pub fn circle(&mut self, center: Vec2, radius: f32, color: impl Into<Color>) {
        self.prims.push(PreviewPrim::Circle {
            center,
            radius,
            color: color.into(),
        });
    }

    /// Adds an open polyline.
    pub fn polyline(&mut self, points: Vec<Vec2>, color: impl Into<Color>) {
        self.prims.push(PreviewPrim::Polyline {
            points,
            color: color.into(),
        });
    }

    /// Emits every primitive to the gizmo buffer.
    pub fn draw(&self, gizmos: &mut Gizmos) {
        for prim in &self.prims {
            match prim {
                PreviewPrim::Line { a, b, color } => {
                    gizmos.line_2d(*a, *b, *color);
                }
                PreviewPrim::Rect {
                    center,
                    size,
                    color,
                } => {
                    gizmos.rect_2d(Isometry2d::from_translation(*center), *size, *color);
                }
                PreviewPrim::Circle {
                    center,
                    radius,
                    color,
                } => {
                    gizmos.circle_2d(Isometry2d::from_translation(*center), *radius, *color);
                }
                PreviewPrim::Polyline { points, color } => {
                    gizmos.linestrip_2d(points.iter().copied(), *color);
                }
            }
        }
    }
}

/// The uniform interface every pure creation tool implements.
///
/// Implementors are plain [`Resource`]s holding only their draft state;
/// all gesture plumbing lives in [`run_draft_tool`]. A scripted tool is
/// later just another implementor returning the same [`ToolCommit`]s.
pub trait DraftTool: Resource<Mutability = Mutable> {
    /// Advances the draft for this frame; returns a commit when the
    /// gesture completes (otherwise `None`).
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit>;

    /// Whether a draft is currently in progress (drives [`ActiveGesture`]
    /// and whether the preview draws).
    fn drafting(&self) -> bool;

    /// Renders the in-progress preview from draft state.
    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview);
}

/// Message writers a [`ToolCommit`] fans out to. Grouped so the generic
/// drivers stay under the system-parameter limit.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ToolCommitWriters<'w> {
    spawn: MessageWriter<'w, SpawnBodyIntent>,
    cut: MessageWriter<'w, CutIntent>,
    joint: MessageWriter<'w, SpawnJointIntent>,
    moves: MessageWriter<'w, CommitTransformIntent>,
    scales: MessageWriter<'w, ScaleIntent>,
    dups: MessageWriter<'w, DuplicateIntent>,
}

impl ToolCommitWriters<'_> {
    /// Translates a commit into its intent message — the single point where
    /// a tool's front-end-agnostic action re-enters the command seam.
    pub fn emit(&mut self, commit: ToolCommit) {
        match commit {
            ToolCommit::SpawnBody(record) => {
                self.spawn.write(SpawnBodyIntent { record: *record });
            }
            ToolCommit::Cut { a, b, width } => {
                self.cut.write(CutIntent { a, b, width });
            }
            ToolCommit::SpawnJoint(record) => {
                self.joint.write(SpawnJointIntent { record: *record });
            }
            ToolCommit::Move(changes) => {
                self.moves.write(CommitTransformIntent { changes });
            }
            ToolCommit::Scale {
                targets,
                pivot,
                frame_rot,
                factors,
            } => {
                self.scales.write(ScaleIntent {
                    targets,
                    pivot,
                    frame_rot,
                    factors,
                });
            }
            ToolCommit::Duplicate { sources, offset } => {
                self.dups.write(DuplicateIntent { sources, offset });
            }
        }
    }
}

/// Computes this frame's [`GesturePhase`] from the pointer state. A press
/// over the UI is suppressed to `Idle`.
fn phase_of(buttons: PointerButtons, over_ui: bool) -> GesturePhase {
    if buttons.just_released(MouseButton::Left) {
        GesturePhase::Released
    } else if buttons.just_pressed(MouseButton::Left) {
        if over_ui {
            GesturePhase::Idle
        } else {
            GesturePhase::Pressed
        }
    } else if buttons.pressed(MouseButton::Left) {
        GesturePhase::Held
    } else {
        GesturePhase::Idle
    }
}

/// Generic driver: builds a [`ToolContext`], advances the tool `T`, and
/// emits any commit through the shared intent writers. One instance runs
/// per creation-tool state (gated by `run_if(in_state(..))`).
pub fn run_draft_tool<T: DraftTool>(
    buttons: Res<PointerButtons>,
    keys: Res<ButtonInput<KeyCode>>,
    snapped: Res<SnappedCursor>,
    over_ui: Res<PointerOverUi>,
    constraints: Res<GestureConstraints>,
    snap: Res<SnapConfig>,
    projections: Query<&Projection, With<Camera3d>>,
    mut tool: ResMut<T>,
    mut active: ResMut<ActiveGesture>,
    mut writers: ToolCommitWriters,
) {
    let ctx = ToolContext {
        phase: phase_of(*buttons, over_ui.0),
        cursor: snapped.effective(),
        raw_cursor: snapped.raw,
        over_ui: over_ui.0,
        confirm: keys.just_pressed(KeyCode::Enter),
        cancel: keys.just_pressed(KeyCode::Escape),
        constraints: &constraints,
        snap: &snap,
        cam_scale: crate::interaction::camera::camera_scale(&projections),
    };
    if let Some(commit) = tool.update(&ctx) {
        writers.emit(commit);
    }
    // Avoid spurious change-detection: only write when the flag flips.
    let drafting = tool.drafting();
    if active.0 != drafting {
        active.0 = drafting;
    }
}

/// Generic preview: draws tool `T`'s in-progress gizmo. Runs only with the
/// render plugin present.
pub fn draw_draft_preview<T: DraftTool>(
    snapped: Res<SnappedCursor>,
    constraints: Res<GestureConstraints>,
    snap: Res<SnapConfig>,
    tool: Res<T>,
    mut gizmos: Gizmos,
) {
    if !tool.drafting() {
        return;
    }
    let ctx = ToolContext {
        phase: GesturePhase::Idle,
        cursor: snapped.effective(),
        raw_cursor: snapped.raw,
        over_ui: false,
        confirm: false,
        cancel: false,
        constraints: &constraints,
        snap: &snap,
        cam_scale: 1.0,
    };
    let mut preview = ToolPreview::default();
    tool.preview(&ctx, &mut preview);
    preview.draw(&mut gizmos);
}
