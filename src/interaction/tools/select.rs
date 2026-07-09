//! The select tool: click/shift/box selection, move, rotate, ctrl-drag
//! duplicate, and bounding-box scaling.
//!
//! Implemented as a [`ManipTool`]: every world read (hit-test, poses, the
//! selection box, group expansion, box/lasso hit sets) goes through the
//! read-total [`ToolWorld`] facade, and each gesture returns a [`ManipOutput`]
//! carrying the transient kinematic hold, the selection change, and — on
//! release — exactly one commit (move / scale / duplicate). The tool mutates
//! nothing directly, so the one-gesture-one-command contract holds.

use crate::command::intent::TransformChange;
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::shape::ShapeDef;
use crate::geometry::polygonize::polygonize;
use crate::geometry::scale::scale_point;
use crate::interaction::selection::{SelectTransition, Selection};
use crate::interaction::tools::context::{
    GesturePhase, HoldState, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview,
    ToolWorld,
};
use crate::interaction::tools::handles::{HandleKind, SelectionBox, hit_handle};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Minimum world-space delta before a click becomes a move/duplicate commit.
const MOVE_EPSILON: f32 = 0.5;
/// Screen-space handle capture radius (logical px).
const HANDLE_RADIUS_PX: f32 = 10.0;
/// Right-drag deadzone (logical px) before rotation engages.
const ROTATE_DEADZONE_PX: f32 = 8.0;

/// The select tool's active gesture.
#[derive(Resource, Default, Debug)]
pub enum SelectGesture {
    /// Nothing in progress.
    #[default]
    Idle,
    /// Dragging selected bodies (kinematic-held transform preview).
    Move {
        /// Cursor position at press.
        start: Vec2,
        /// `(entity, id, original pose)` per moved body.
        bodies: Vec<(Entity, StableId, PosRot)>,
    },
    /// Right-drag rotating the selection about its centroid.
    ///
    /// Armed on right-press but only **engages** (kinematic hold, preview)
    /// once the cursor leaves a small deadzone — a right *click* commits
    /// nothing and falls through to the context menu.
    Rotate {
        /// Rotation pivot (selection centroid).
        pivot: Vec2,
        /// Cursor angle at press.
        start_angle: f32,
        /// Cursor position at press (deadzone reference).
        press: Vec2,
        /// Whether the deadzone was left (rotation actually running).
        engaged: bool,
        /// `(entity, id, original pose)` per rotated body.
        bodies: Vec<(Entity, StableId, PosRot)>,
    },
    /// Rubber-band selection on empty canvas.
    BoxSelect {
        /// Press position.
        start: Vec2,
        /// Keep the existing selection (shift held at press).
        additive: bool,
    },
    /// Freeform loop selection (Alt-drag on empty canvas): bodies whose
    /// center falls inside the drawn loop are selected.
    Lasso {
        /// Loop points so far, world space.
        points: Vec<Vec2>,
        /// Keep the existing selection (shift held at press).
        additive: bool,
    },
    /// Ctrl-drag duplicating: ghosts preview at the offset, one
    /// `DuplicateIntent` on release.
    DupDrag {
        /// Press position.
        start: Vec2,
        /// Source body ids.
        sources: Vec<StableId>,
        /// `(shape, pose)` of sources, for ghost outlines.
        ghosts: Vec<(ShapeDef, PosRot)>,
    },
    /// Scaling via a bounding-box handle (gizmo preview only).
    Scale {
        /// The grabbed handle.
        handle: HandleKind,
        /// The frozen selection box at press time.
        sbox: SelectionBox,
        /// Fixed point (opposite handle), world space.
        pivot: Vec2,
        /// Cursor position at press, frame coords relative to pivot.
        start_f: Vec2,
        /// Current factors (updated during drag, committed on release).
        factors: Vec2,
        /// Target ids.
        targets: Vec<StableId>,
        /// `(shape, pose)` of targets for the gizmo preview.
        ghosts: Vec<(ShapeDef, PosRot)>,
    },
}

impl ManipTool for SelectGesture {
    fn update(
        &mut self,
        ctx: &ManipContext,
        world: &ToolWorld,
        selection: &Selection,
    ) -> ManipOutput {
        // ---- Press (left): start a gesture (a joint pick this frame
        // suppresses it). ----
        if ctx.left == GesturePhase::Pressed
            && !ctx.suppress_press
            && let Some(p) = ctx.cursor
        {
            return self.left_press(ctx, world, selection, p);
        }

        // ---- Press (right) over the selection: start rotation. ----
        if ctx.right == GesturePhase::Pressed
            && matches!(self, SelectGesture::Idle)
            && !selection.is_empty()
            && let Some(p) = ctx.cursor
        {
            self.start_rotate(world, selection, p);
            return ManipOutput::default();
        }

        // ---- Release: commit exactly one intent / apply the selection. ----
        if ctx.left == GesturePhase::Released || ctx.right == GesturePhase::Released {
            return self.release(ctx, world);
        }

        // ---- Drag: advance the preview. ----
        self.drag(ctx)
    }

    fn drafting(&self) -> bool {
        !matches!(self, SelectGesture::Idle)
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        match self {
            SelectGesture::BoxSelect { start, .. } => {
                if let Some(p) = ctx.cursor {
                    let center = (*start + p) / 2.0;
                    let size = (p - *start).abs();
                    out.rect(center, size, css::LIGHT_SKY_BLUE.with_alpha(0.8));
                }
            }
            SelectGesture::Lasso { points, .. } if points.len() >= 2 => {
                let mut pts = points.clone();
                if let Some(first) = pts.first().copied() {
                    pts.push(first);
                }
                out.polyline(pts, css::LIGHT_SKY_BLUE.with_alpha(0.8));
            }
            SelectGesture::DupDrag { start, ghosts, .. } => {
                if let Some(p) = ctx.cursor {
                    let offset = ctx.constraints.constrain(p - *start);
                    for (shape, pose) in ghosts {
                        push_shape_outline(
                            out,
                            shape,
                            PosRot {
                                pos: pose.pos + offset,
                                rot: pose.rot,
                            },
                            css::AQUAMARINE.with_alpha(0.8),
                        );
                    }
                }
            }
            SelectGesture::Scale {
                sbox,
                pivot,
                factors,
                ghosts,
                ..
            } => {
                // Each selected contour mapped through the frame scale about
                // the pivot (world-space, exact).
                for (shape, pose) in ghosts {
                    let rot = Vec2::from_angle(pose.rot);
                    for ring in polygonize(shape).rings() {
                        let mut pts: Vec<Vec2> = ring
                            .iter()
                            .map(|v| {
                                let w = pose.pos + rot.rotate(*v);
                                scale_point(w, *pivot, sbox.rot, *factors)
                            })
                            .collect();
                        if let Some(first) = pts.first().copied() {
                            pts.push(first);
                        }
                        out.polyline(pts, css::GOLD.with_alpha(0.9));
                    }
                }
            }
            _ => {}
        }
    }
}

impl SelectGesture {
    /// Left-press dispatch: scale handle, body gesture, or empty-canvas band.
    fn left_press(
        &mut self,
        ctx: &ManipContext,
        world: &ToolWorld,
        selection: &Selection,
        p: Vec2,
    ) -> ManipOutput {
        // 1. Scale handle?
        if let Some(sbox) = world.selection_box(selection, ctx.scale_frame)
            && let Some(handle) = hit_handle(&sbox, p, HANDLE_RADIUS_PX * ctx.cam_scale)
        {
            let pivot = sbox.point(handle.anchor_unit());
            let start_f = sbox.to_frame(p, pivot);
            let targets: Vec<StableId> = selection.iter().filter_map(|e| world.id_of(e)).collect();
            let ghosts: Vec<(ShapeDef, PosRot)> = selection
                .iter()
                .filter_map(|e| world.shape_pose(e))
                .collect();
            if !targets.is_empty() {
                *self = SelectGesture::Scale {
                    handle,
                    sbox,
                    pivot,
                    start_f,
                    factors: Vec2::ONE,
                    targets,
                    ghosts,
                };
                return ManipOutput::default();
            }
        }

        // 2. Body under cursor?
        if let Some(hit) = world.topmost_body_at(p) {
            return self.press_on_body(ctx, world, selection, p, hit);
        }

        // 3. Empty canvas → rubber band (Ctrl/Alt = freeform loop). Pressing
        // empty space drops any selected joint immediately.
        *self = if ctx.alt || ctx.ctrl {
            SelectGesture::Lasso {
                points: vec![p],
                additive: ctx.shift,
            }
        } else {
            SelectGesture::BoxSelect {
                start: p,
                additive: ctx.shift,
            }
        };
        ManipOutput {
            selection: Some(SelectTransition::DeselectJoint),
            ..Default::default()
        }
    }

    /// Left-press on a body: duplicate-drag (ctrl), toggle (shift), or start
    /// a move of the (group-expanded) selection.
    fn press_on_body(
        &mut self,
        ctx: &ManipContext,
        world: &ToolWorld,
        selection: &Selection,
        p: Vec2,
        hit: Entity,
    ) -> ManipOutput {
        let mut members = vec![hit];
        world.expand_group_members(&mut members);

        if ctx.ctrl {
            // Duplicate-drag the hit (or the whole selection if it was the
            // hit that was already selected).
            let sources_entities: Vec<Entity> = if selection.contains(hit) {
                selection.iter().collect()
            } else {
                members.clone()
            };
            let sources: Vec<StableId> = sources_entities
                .iter()
                .filter_map(|e| world.id_of(*e))
                .collect();
            let ghosts: Vec<(ShapeDef, PosRot)> = sources_entities
                .iter()
                .filter_map(|e| world.shape_pose(*e))
                .collect();
            if !sources.is_empty() {
                *self = SelectGesture::DupDrag {
                    start: p,
                    sources,
                    ghosts,
                };
            }
            return ManipOutput::default();
        }

        if ctx.shift {
            return ManipOutput {
                selection: Some(SelectTransition::ToggleBodies(members)),
                ..Default::default()
            };
        }

        // Set (or keep) the selection, then start moving it. The move bodies
        // are the selection *as it will be* after the transition the driver
        // applies this frame.
        let (transition, move_entities): (SelectTransition, Vec<Entity>) =
            if selection.contains(hit) {
                (SelectTransition::DeselectJoint, selection.iter().collect())
            } else {
                (
                    SelectTransition::SetBodies(members.clone()),
                    dedup_entities(members),
                )
            };
        let bodies: Vec<(Entity, StableId, PosRot)> = move_entities
            .iter()
            .filter_map(|e| Some((*e, world.id_of(*e)?, world.pose_of(*e)?)))
            .collect();
        let held: Vec<Entity> = bodies.iter().map(|(e, ..)| *e).collect();
        *self = SelectGesture::Move { start: p, bodies };
        ManipOutput {
            selection: Some(transition),
            hold: HoldState::Acquire(held),
            ..Default::default()
        }
    }

    /// Arms a rotation gesture if the right-press landed on a selected body.
    fn start_rotate(&mut self, world: &ToolWorld, selection: &Selection, p: Vec2) {
        let over_selected = world
            .topmost_body_at(p)
            .is_some_and(|hit| selection.contains(hit));
        if !over_selected {
            return;
        }
        let bodies: Vec<(Entity, StableId, PosRot)> = selection
            .iter()
            .filter_map(|e| Some((e, world.id_of(e)?, world.pose_of(e)?)))
            .collect();
        let pivot =
            bodies.iter().map(|(_, _, pose)| pose.pos).sum::<Vec2>() / bodies.len().max(1) as f32;
        // Hold/exclusions wait for the deadzone: a plain right click must
        // leave the world untouched (context menu).
        *self = SelectGesture::Rotate {
            pivot,
            start_angle: (p - pivot).to_angle(),
            press: p,
            engaged: false,
            bodies,
        };
    }

    /// Advances the in-progress gesture's transient preview.
    fn drag(&mut self, ctx: &ManipContext) -> ManipOutput {
        match self {
            SelectGesture::Move { start, bodies } => {
                if let Some(p) = ctx.cursor {
                    let delta = ctx.constraints.constrain(p - *start);
                    let poses = bodies
                        .iter()
                        .map(|(e, _, original)| {
                            (
                                *e,
                                PosRot {
                                    pos: original.pos + delta,
                                    rot: original.rot,
                                },
                            )
                        })
                        .collect();
                    return ManipOutput {
                        hold: HoldState::Set(poses),
                        ..Default::default()
                    };
                }
            }
            SelectGesture::Rotate {
                pivot,
                start_angle,
                press,
                engaged,
                bodies,
            } => {
                if let Some(p) = ctx.raw_cursor {
                    if !*engaged && press.distance(p) > ROTATE_DEADZONE_PX * ctx.cam_scale {
                        *engaged = true;
                    }
                    if *engaged {
                        let angle = ctx
                            .constraints
                            .apply_rotation((p - *pivot).to_angle() - *start_angle, ctx.snap);
                        let rot = Vec2::from_angle(angle);
                        let poses = bodies
                            .iter()
                            .map(|(e, _, original)| {
                                (
                                    *e,
                                    PosRot {
                                        pos: *pivot + rot.rotate(original.pos - *pivot),
                                        rot: original.rot + angle,
                                    },
                                )
                            })
                            .collect();
                        return ManipOutput {
                            hold: HoldState::Set(poses),
                            ..Default::default()
                        };
                    }
                }
            }
            SelectGesture::Scale {
                handle,
                sbox,
                pivot,
                start_f,
                factors,
                ..
            } => {
                if let Some(p) = ctx.cursor {
                    let cur_f = sbox.to_frame(p, *pivot);
                    let (sx, sy) = handle.scales();
                    let mut f = Vec2::ONE;
                    if sx && start_f.x.abs() > 1.0 {
                        f.x = cur_f.x / start_f.x;
                    }
                    if sy && start_f.y.abs() > 1.0 {
                        f.y = cur_f.y / start_f.y;
                    }
                    // Shift on a corner = uniform scale (larger magnitude wins).
                    if sx && sy && ctx.shift {
                        let m = if f.x.abs() > f.y.abs() { f.x } else { f.y };
                        f = Vec2::new(m.abs().copysign(f.x), m.abs().copysign(f.y));
                    }
                    *factors = f;
                }
            }
            SelectGesture::Lasso { points, .. } => {
                if let Some(p) = ctx.raw_cursor
                    && points.last().is_none_or(|last| last.distance(p) > 4.0)
                {
                    points.push(p);
                }
            }
            SelectGesture::Idle
            | SelectGesture::BoxSelect { .. }
            | SelectGesture::DupDrag { .. } => {}
        }
        ManipOutput::default()
    }

    /// Finishes a gesture on button release: one commit or one selection edit.
    fn release(&mut self, ctx: &ManipContext, world: &ToolWorld) -> ManipOutput {
        let left_up = ctx.left == GesturePhase::Released;
        let right_up = ctx.right == GesturePhase::Released;
        match std::mem::take(self) {
            SelectGesture::Move { start, bodies } if left_up => {
                let changes = ctx.cursor.map_or_else(Vec::new, |p| {
                    let delta = ctx.constraints.constrain(p - start);
                    move_changes(&bodies, |old| PosRot {
                        pos: old.pos + delta,
                        rot: old.rot,
                    })
                });
                ManipOutput {
                    commit: (!changes.is_empty()).then_some(ToolCommit::Move(changes)),
                    hold: HoldState::Clear,
                    ..Default::default()
                }
            }
            SelectGesture::Rotate {
                pivot,
                start_angle,
                engaged,
                bodies,
                ..
            } if right_up => {
                let changes = if let (true, Some(p)) = (engaged, ctx.raw_cursor) {
                    let angle = ctx
                        .constraints
                        .apply_rotation((p - pivot).to_angle() - start_angle, ctx.snap);
                    let rot = Vec2::from_angle(angle);
                    move_changes(&bodies, |old| PosRot {
                        pos: pivot + rot.rotate(old.pos - pivot),
                        rot: old.rot + angle,
                    })
                } else {
                    Vec::new()
                };
                ManipOutput {
                    commit: (!changes.is_empty()).then_some(ToolCommit::Move(changes)),
                    hold: HoldState::Clear,
                    ..Default::default()
                }
            }
            SelectGesture::BoxSelect { start, additive } if left_up => ManipOutput {
                selection: ctx.cursor.map(|p| {
                    let hits = world.bodies_in_box(start.min(p), start.max(p));
                    box_transition(hits, additive)
                }),
                ..Default::default()
            },
            SelectGesture::Lasso { points, additive } if left_up => ManipOutput {
                selection: (points.len() >= 3)
                    .then(|| box_transition(world.bodies_in_ring(&points), additive)),
                ..Default::default()
            },
            SelectGesture::DupDrag { start, sources, .. } if left_up => ManipOutput {
                commit: ctx.cursor.and_then(|p| {
                    let offset = ctx.constraints.constrain(p - start);
                    (offset.length() > MOVE_EPSILON)
                        .then_some(ToolCommit::Duplicate { sources, offset })
                }),
                ..Default::default()
            },
            SelectGesture::Scale {
                sbox,
                pivot,
                factors,
                targets,
                ..
            } if left_up => ManipOutput {
                commit: ((factors - Vec2::ONE).length() > 1e-3).then_some(ToolCommit::Scale {
                    targets,
                    pivot,
                    frame_rot: sbox.rot,
                    factors,
                }),
                ..Default::default()
            },
            // Released the other button mid-gesture: keep going.
            other => {
                *self = other;
                ManipOutput::default()
            }
        }
    }
}

/// Builds the `TransformChange` list for a completed move/rotate, keeping only
/// bodies that actually moved (position for moves, angle for rotates).
fn move_changes(
    bodies: &[(Entity, StableId, PosRot)],
    new_pose: impl Fn(&PosRot) -> PosRot,
) -> Vec<TransformChange> {
    bodies
        .iter()
        .filter_map(|(_, id, old)| {
            let new = new_pose(old);
            let moved =
                new.pos.distance(old.pos) > MOVE_EPSILON || (new.rot - old.rot).abs() > 1e-4;
            moved.then_some(TransformChange {
                id: *id,
                old: *old,
                new,
            })
        })
        .collect()
}

/// Set or add the box/lasso hits depending on whether shift was held.
fn box_transition(hits: Vec<Entity>, additive: bool) -> SelectTransition {
    if additive {
        SelectTransition::AddBodies(hits)
    } else {
        SelectTransition::SetBodies(hits)
    }
}

/// Deduplicates entities, preserving first-seen order (matches
/// `SelectTransition::SetBodies`).
fn dedup_entities(entities: Vec<Entity>) -> Vec<Entity> {
    let mut out = Vec::with_capacity(entities.len());
    for e in entities {
        if !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

/// Pushes a shape's world-space outline rings into the preview.
fn push_shape_outline(
    out: &mut ToolPreview,
    shape: &ShapeDef,
    pose: PosRot,
    color: impl Into<Color>,
) {
    let color = color.into();
    let rot = Vec2::from_angle(pose.rot);
    for ring in polygonize(shape).rings() {
        let mut pts: Vec<Vec2> = ring.iter().map(|v| pose.pos + rot.rotate(*v)).collect();
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
        out.polyline(pts, color);
    }
}
