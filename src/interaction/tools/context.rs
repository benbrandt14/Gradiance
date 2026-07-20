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
//!
//! # The manipulation half
//!
//! Select, drag, and the connector tools need more than a pure context: they
//! **read the world** (hit-testing, poses, the selection box) and drive
//! **transient world state** (kinematic-held `Transform`, the mouse spring)
//! during a gesture. They implement the sibling [`ManipTool`] interface —
//! `update(ctx, world, selection) -> ManipOutput` — where reads flow through
//! the read-total [`ToolWorld`] facade and every write is a field of the
//! returned [`ManipOutput`] that the generic driver ([`run_manip_tool`])
//! applies through its existing seam: `commit` → the same `ToolCommit` →
//! intent path, `hold`/`grab` → transient physics preview, `selection` →
//! editor state. The tool still mutates nothing directly, so the same gesture
//! contract holds — reads are total, writes are seam-mediated.

use crate::command::intent::{
    CommitTransformIntent, CutIntent, DuplicateIntent, MergeIntent, PropertyEditIntent,
    ScaleIntent, SpawnBodyIntent, SpawnJointIntent, TransformChange,
};
use crate::command::property::{PropertyChange, PropertyValue};
use crate::scene::{BodyRecord, JointRecord};
use crate::core::ids::StableId;
use crate::core::states::{GameState, ToolState};
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::depth::DepthBand;
use crate::domain::group::SelectionGroup;
use crate::domain::settings::{SnapConfig, ToolDefaults};
use crate::domain::shape::ShapeDef;
use crate::geometry::contours::point_in_ring;
use crate::interaction::PointerOverUi;
use crate::interaction::gesture::GestureConstraints;
use crate::interaction::joint_edit::SuppressSelectPress;
use crate::interaction::pointer::PointerButtons;
use crate::interaction::selection::{SelectTransition, SelectedJoint, Selection, expand_groups};
use crate::interaction::snap::{SnapExclusions, SnappedCursor};
use crate::interaction::tools::ActiveGesture;
use crate::interaction::tools::handles::{ScaleFrame, SelectionBox, selection_box};
use crate::physics::grab::{Grab, MouseSpring, MouseTwist, Twist};
use crate::physics::hold::KinematicHold;
use crate::physics::queries::PhysicsQueries;
use crate::interaction::overlay::OverlayGizmos;
use avian2d::prelude::RigidBody;
use bevy::ecs::component::Mutable;
use bevy::ecs::system::SystemParam;
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
    /// Merge bodies into one (the weld tool over two bodies — SDF union).
    Merge {
        /// Bodies to merge; the first survives with the union of all shapes.
        targets: Vec<StableId>,
    },
    /// Pin a body to the world by making it static (the weld tool over a
    /// single body).
    MakeStatic {
        /// Target body.
        id: StableId,
        /// Its current kind, captured for undo.
        old: RigidBody,
    },
    /// Author a new behavior node (the tracer tool; future sensors/actuators).
    SpawnNode(Box<crate::scene::NodeRecord>),
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
    pub fn draw(&self, gizmos: &mut Gizmos<OverlayGizmos>) {
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
    spawn_node: MessageWriter<'w, crate::command::intent::SpawnNodeIntent>,
    cut: MessageWriter<'w, CutIntent>,
    joint: MessageWriter<'w, SpawnJointIntent>,
    moves: MessageWriter<'w, CommitTransformIntent>,
    scales: MessageWriter<'w, ScaleIntent>,
    dups: MessageWriter<'w, DuplicateIntent>,
    merges: MessageWriter<'w, MergeIntent>,
    props: MessageWriter<'w, PropertyEditIntent>,
}

impl ToolCommitWriters<'_> {
    /// Translates a commit into its intent message — the single point where
    /// a tool's front-end-agnostic action re-enters the command seam.
    pub fn emit(&mut self, commit: ToolCommit) {
        match commit {
            ToolCommit::SpawnBody(record) => {
                self.spawn.write(SpawnBodyIntent { record: *record });
            }
            ToolCommit::SpawnNode(record) => {
                self.spawn_node
                    .write(crate::command::intent::SpawnNodeIntent { record: *record });
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
            ToolCommit::Merge { targets } => {
                self.merges.write(MergeIntent { targets });
            }
            ToolCommit::MakeStatic { id, old } => {
                self.props.write(PropertyEditIntent {
                    changes: vec![PropertyChange {
                        id,
                        old: PropertyValue::RigidBody(old),
                        new: PropertyValue::RigidBody(RigidBody::Static),
                    }],
                });
            }
        }
    }
}

/// Computes this frame's [`GesturePhase`] for `button`. A press over the UI is
/// suppressed to `Idle` (a tool must not start a gesture on a click egui owns);
/// a release always reports `Released` regardless of what the cursor is over.
fn phase_of_button(buttons: PointerButtons, button: MouseButton, over_ui: bool) -> GesturePhase {
    if buttons.just_released(button) {
        GesturePhase::Released
    } else if buttons.just_pressed(button) {
        if over_ui {
            GesturePhase::Idle
        } else {
            GesturePhase::Pressed
        }
    } else if buttons.pressed(button) {
        GesturePhase::Held
    } else {
        GesturePhase::Idle
    }
}

/// The primary (left) button's phase this frame.
fn phase_of(buttons: PointerButtons, over_ui: bool) -> GesturePhase {
    phase_of_button(buttons, MouseButton::Left, over_ui)
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
    cam_scale: Res<crate::interaction::camera::CameraScale>,
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
        cam_scale: cam_scale.0,
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
    mut gizmos: Gizmos<OverlayGizmos>,
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

// ============================================================================
// Manipulation-tool half of the facade
// ============================================================================
//
// Creation tools ([`DraftTool`]) are pure functions of draft state + a pure
// [`ToolContext`]. Manipulation tools (drag, select, connector) additionally
// need **total world reads** (hit-testing, poses, ids) and, during a gesture,
// **transient world writes** (kinematic-held `Transform`). They express those
// through this richer half: reads via [`ToolWorld`] (a read facade — "reads are
// total"), writes as a returned [`ManipOutput`] the driver applies (commits →
// intents, holds → kinematic `Transform`, selection → editor state). A tool
// still touches no authored component and no command stack, so the gesture
// contract (invariants 1–2) holds by construction.

/// Read-only world facade a [`ManipTool`] queries during a gesture.
///
/// Reads are total (the governance model's read half): a manipulation tool
/// hit-tests and resolves poses/ids through this instead of holding raw
/// queries, exactly as a script or plotter reads through `physics::queries`.
#[derive(SystemParam)]
pub struct ToolWorld<'w, 's> {
    physics: PhysicsQueries<'w, 's>,
    hit: Query<'w, 's, (&'static ShapeDef, &'static DepthBand), With<Body>>,
    shapes: Query<'w, 's, (&'static ShapeDef, &'static Transform), With<Body>>,
    centers: Query<'w, 's, (Entity, &'static Transform), With<Body>>,
    ids: Query<'w, 's, &'static StableId>,
    groups: Query<'w, 's, (Entity, &'static SelectionGroup), With<Body>>,
    kinds: Query<'w, 's, &'static RigidBody, With<Body>>,
}

impl ToolWorld<'_, '_> {
    /// The topmost authored body at a world point (front layer wins; the
    /// infinite ground sorts last).
    pub fn topmost_body_at(&self, p: Vec2) -> Option<Entity> {
        super::topmost_body_at(p, &self.physics, &self.hit)
    }

    /// Every authored body at a point, topmost first.
    pub fn bodies_at(&self, p: Vec2) -> Vec<Entity> {
        super::bodies_at_sorted(p, &self.physics, &self.hit)
    }

    /// A body entity's authored pose.
    pub fn pose_of(&self, entity: Entity) -> Option<PosRot> {
        self.shapes
            .get(entity)
            .ok()
            .map(|(_, t)| PosRot::from_transform(t))
    }

    /// A body entity's stable id.
    pub fn id_of(&self, entity: Entity) -> Option<StableId> {
        self.ids.get(entity).ok().copied()
    }

    /// A body entity's authored rigid-body kind.
    pub fn body_kind(&self, entity: Entity) -> Option<RigidBody> {
        self.kinds.get(entity).ok().copied()
    }

    /// A body entity's authored shape + pose (for ghost previews).
    pub fn shape_pose(&self, entity: Entity) -> Option<(ShapeDef, PosRot)> {
        self.shapes
            .get(entity)
            .ok()
            .map(|(s, t)| (s.clone(), PosRot::from_transform(t)))
    }

    /// Expands `members` with every body sharing a [`SelectionGroup`] with one
    /// of them (selecting one grouped body selects the whole assembly).
    pub fn expand_group_members(&self, members: &mut Vec<Entity>) {
        expand_groups(members, &self.groups);
    }

    /// The selection's oriented bounding box in `frame`, if anything scalable
    /// is selected.
    pub fn selection_box(&self, selection: &Selection, frame: ScaleFrame) -> Option<SelectionBox> {
        selection_box(selection, frame, &self.shapes)
    }

    /// Bodies fully inside the axis-aligned box (the "no partials" rule for
    /// box selection; the infinite ground is excluded).
    pub fn bodies_in_box(&self, min: Vec2, max: Vec2) -> Vec<Entity> {
        self.physics
            .bodies_in_aabb(min, max)
            .into_iter()
            .filter(|e| {
                self.shapes.get(*e).is_ok_and(|(shape, transform)| {
                    !shape.contains_half_plane() && body_fully_inside(shape, transform, min, max)
                })
            })
            .collect()
    }

    /// Bodies whose center lies inside the closed loop `ring` (the lasso rule;
    /// the infinite ground is excluded).
    pub fn bodies_in_ring(&self, ring: &[Vec2]) -> Vec<Entity> {
        self.centers
            .iter()
            .filter(|(_, t)| point_in_ring(t.translation.truncate(), ring))
            .filter(|(e, _)| {
                self.shapes
                    .get(*e)
                    .is_ok_and(|(shape, _)| !shape.contains_half_plane())
            })
            .map(|(e, _)| e)
            .collect()
    }
}

/// Whether a body's (conservative) world bounds lie fully inside the
/// axis-aligned box — the "no partials" rule for box selection.
fn body_fully_inside(shape: &ShapeDef, transform: &Transform, min: Vec2, max: Vec2) -> bool {
    let (smin, smax) = crate::geometry::sdf::aabb(shape);
    let affine = transform.compute_affine();
    [
        Vec2::new(smin.x, smin.y),
        Vec2::new(smax.x, smin.y),
        Vec2::new(smax.x, smax.y),
        Vec2::new(smin.x, smax.y),
    ]
    .into_iter()
    .all(|corner| {
        let w = affine.transform_point3(corner.extend(0.0)).truncate();
        w.x >= min.x && w.x <= max.x && w.y >= min.y && w.y <= max.y
    })
}

/// A transient kinematic-hold request for one frame.
///
/// Move/rotate previews compute target poses each frame and let the driver
/// apply them under [`KinematicHold`] — the sanctioned transient preview state
/// (invariant #2), never an authored-component write.
#[derive(Debug, Clone, Default)]
pub enum HoldState {
    /// Leave any existing hold untouched (tools that don't hold).
    #[default]
    Keep,
    /// Mark these bodies held (and snap-excluded) without moving them this
    /// frame — the acquire frame of a move/rotate gesture.
    Acquire(Vec<Entity>),
    /// Hold exactly these bodies at these poses this frame.
    Set(Vec<(Entity, PosRot)>),
    /// Release the hold (gesture ended).
    Clear,
}

/// A transient mouse-spring (physical grab) request for one frame — the
/// play-mode drag interaction. Not undoable (physical interaction, matching
/// Algodoo), so it never touches the command seam.
#[derive(Debug, Clone, Copy, Default)]
pub enum GrabState {
    /// Leave any existing spring untouched.
    #[default]
    Keep,
    /// Grab with this spring this frame.
    Set(Grab),
    /// Release the spring.
    Clear,
}

/// A transient angular-servo (physical twist) request for one frame — the
/// play-mode rotate interaction. Like [`GrabState`], it is physical and not
/// undoable, so it never touches the command seam.
#[derive(Debug, Clone, Default)]
pub enum TwistState {
    /// Leave any existing twist untouched.
    #[default]
    Keep,
    /// Twist exactly these bodies toward these angles this frame.
    Set(Vec<Twist>),
    /// Release the twist.
    Clear,
}

/// What a [`ManipTool`] asks the driver to apply this frame.
///
/// Every field routes through an existing seam: `commit` → intents (undoable),
/// `hold`/`grab` → transient physics preview state (kinematic `Transform` /
/// mouse spring), `selection` → editor-state resources. The tool itself
/// mutates nothing outside its own draft state.
#[derive(Default)]
pub struct ManipOutput {
    /// One authoring action to commit (at most one per gesture — invariant).
    pub commit: Option<ToolCommit>,
    /// Transient kinematic hold for this frame (move/rotate previews).
    pub hold: HoldState,
    /// Transient mouse-spring grab for this frame (play-mode drag).
    pub grab: GrabState,
    /// Transient angular servo for this frame (play-mode rotate).
    pub twist: TwistState,
    /// Editor selection change (applied via [`SelectTransition`]).
    pub selection: Option<SelectTransition>,
}

impl ManipOutput {
    /// Shorthand for a frame whose only effect is a commit.
    pub fn commit(commit: ToolCommit) -> Self {
        Self {
            commit: Some(commit),
            ..Default::default()
        }
    }
}

/// Read-only per-frame inputs for a manipulation gesture (the manipulation
/// analogue of [`ToolContext`]).
pub struct ManipContext<'a> {
    /// Left-button gesture phase.
    pub left: GesturePhase,
    /// Right-button gesture phase (rotation gestures).
    pub right: GesturePhase,
    /// Snapped, effective cursor. `None` off-screen.
    pub cursor: Option<Vec2>,
    /// Snapped raw cursor (freeform strokes / deadzone tests).
    pub raw_cursor: Option<Vec2>,
    /// Whether the pointer is over egui.
    pub over_ui: bool,
    /// A joint pick consumed the press this frame (skip body gestures).
    pub suppress_press: bool,
    /// Ctrl held (duplicate / freeform loop).
    pub ctrl: bool,
    /// Shift held (additive select / uniform scale).
    pub shift: bool,
    /// Alt held (freeform loop).
    pub alt: bool,
    /// Whether the simulation is playing (drag: spring vs. move).
    pub playing: bool,
    /// The active tool (connector variant: hinge/weld/slider).
    pub tool: ToolState,
    /// The active scale frame (global/local) for the selection box.
    pub scale_frame: ScaleFrame,
    /// Axis/rotation gesture constraints.
    pub constraints: &'a GestureConstraints,
    /// Snapping configuration.
    pub snap: &'a SnapConfig,
    /// Tool-creation defaults (slider limits etc.).
    pub defaults: ToolDefaults,
    /// World units per logical screen pixel.
    pub cam_scale: f32,
}

/// The uniform interface a world-reading, transient-writing tool implements —
/// the manipulation analogue of [`DraftTool`].
///
/// `update` is a function of draft state + [`ManipContext`] + read-only
/// [`ToolWorld`], returning a [`ManipOutput`] the driver applies. A scripted
/// manipulation is later just another implementor.
pub trait ManipTool: Resource<Mutability = Mutable> {
    /// Advances the gesture for this frame. `selection` is the current body
    /// selection (read-only); the driver applies any [`ManipOutput::selection`]
    /// change afterward.
    fn update(
        &mut self,
        ctx: &ManipContext,
        world: &ToolWorld,
        selection: &Selection,
    ) -> ManipOutput;

    /// Whether a gesture is in progress (drives [`ActiveGesture`] and preview).
    fn drafting(&self) -> bool;

    /// Renders the in-progress preview from draft state.
    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview);
}

/// Pointer/tool-state inputs for the manipulation drivers, bundled to stay
/// under the system-parameter limit.
#[derive(SystemParam)]
pub struct ManipInputs<'w> {
    buttons: Res<'w, PointerButtons>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    snapped: Res<'w, SnappedCursor>,
    over_ui: Res<'w, PointerOverUi>,
    suppress: Res<'w, SuppressSelectPress>,
    constraints: Res<'w, GestureConstraints>,
    snap: Res<'w, SnapConfig>,
    frame: Res<'w, ScaleFrame>,
    defaults: Res<'w, ToolDefaults>,
    tool_state: Res<'w, State<ToolState>>,
    game_state: Res<'w, State<GameState>>,
    cam_scale: Res<'w, crate::interaction::camera::CameraScale>,
}

impl ManipInputs<'_> {
    /// Builds this frame's read-only manipulation context.
    fn manip_context(&self) -> ManipContext<'_> {
        let held = |a, b| self.keys.pressed(a) || self.keys.pressed(b);
        ManipContext {
            left: phase_of_button(*self.buttons, MouseButton::Left, self.over_ui.0),
            right: phase_of_button(*self.buttons, MouseButton::Right, self.over_ui.0),
            cursor: self.snapped.effective(),
            raw_cursor: self.snapped.raw,
            over_ui: self.over_ui.0,
            suppress_press: self.suppress.0,
            ctrl: held(KeyCode::ControlLeft, KeyCode::ControlRight),
            shift: held(KeyCode::ShiftLeft, KeyCode::ShiftRight),
            alt: held(KeyCode::AltLeft, KeyCode::AltRight),
            playing: *self.game_state.get() == GameState::Playing,
            tool: *self.tool_state.get(),
            scale_frame: *self.frame,
            constraints: &self.constraints,
            snap: &self.snap,
            defaults: *self.defaults,
            cam_scale: self.cam_scale.0,
        }
    }
}

/// Editor-state a manipulation gesture drives (transient hold + selection),
/// bundled for the driver.
#[derive(SystemParam)]
pub struct ManipEditor<'w, 's> {
    active: ResMut<'w, ActiveGesture>,
    hold: ResMut<'w, KinematicHold>,
    exclusions: ResMut<'w, SnapExclusions>,
    spring: ResMut<'w, MouseSpring>,
    twist: ResMut<'w, MouseTwist>,
    selection: ResMut<'w, Selection>,
    joint: ResMut<'w, SelectedJoint>,
    groups: Query<'w, 's, (Entity, &'static SelectionGroup), With<Body>>,
}

/// Generic driver: builds a [`ManipContext`], advances the tool `T`, and
/// applies its [`ManipOutput`] through the existing seams (commit → intent,
/// hold → kinematic `Transform`, selection → editor state). One instance runs
/// per manipulation tool, gated by `run_if(in_state(..))`.
pub fn run_manip_tool<T: ManipTool>(
    inputs: ManipInputs,
    mut editor: ManipEditor,
    mut world: ParamSet<(ToolWorld, Query<&mut Transform, With<Body>>)>,
    mut writers: ToolCommitWriters,
    mut tool: ResMut<T>,
) {
    let ctx = inputs.manip_context();
    let out = tool.update(&ctx, &world.p0(), &editor.selection);

    if let Some(commit) = out.commit {
        writers.emit(commit);
    }
    if let Some(transition) = out.selection {
        transition.apply(&mut editor.selection, &mut editor.joint, &editor.groups);
    }
    match out.grab {
        GrabState::Keep => {}
        GrabState::Set(grab) => editor.spring.0 = Some(grab),
        GrabState::Clear => editor.spring.0 = None,
    }
    match out.twist {
        TwistState::Keep => {}
        TwistState::Set(twists) => editor.twist.0 = twists,
        TwistState::Clear => editor.twist.0.clear(),
    }
    match out.hold {
        HoldState::Keep => {}
        HoldState::Acquire(entities) => {
            editor.hold.entities = entities;
            editor.exclusions.0.clone_from(&editor.hold.entities);
        }
        HoldState::Clear => {
            editor.hold.entities.clear();
            editor.exclusions.0.clear();
        }
        HoldState::Set(poses) => {
            editor.hold.entities = poses.iter().map(|(e, _)| *e).collect();
            editor.exclusions.0.clone_from(&editor.hold.entities);
            let mut transforms = world.p1();
            for (entity, pose) in &poses {
                if let Ok(mut transform) = transforms.get_mut(*entity) {
                    pose.apply_to(&mut transform);
                }
            }
        }
    }

    let drafting = tool.drafting();
    if editor.active.0 != drafting {
        editor.active.0 = drafting;
    }
}

/// Generic preview: draws manipulation tool `T`'s in-progress gizmo. Runs only
/// with the render plugin present.
pub fn draw_manip_preview<T: ManipTool>(
    inputs: ManipInputs,
    tool: Res<T>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    if !tool.drafting() {
        return;
    }
    let ctx = inputs.manip_context();
    let mut preview = ToolPreview::default();
    tool.preview(&ctx, &mut preview);
    preview.draw(&mut gizmos);
}
