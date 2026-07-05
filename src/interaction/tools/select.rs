//! The select tool: click/shift/box selection, move, rotate, ctrl-drag
//! duplicate, and bounding-box scaling.
//!
//! One gesture = one committed intent. Previews are transforms under
//! kinematic hold (move/rotate) or pure gizmos (scale, duplicate, box
//! select).

use crate::command::intent::{
    CommitTransformIntent, DuplicateIntent, ScaleIntent, TransformChange,
};
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use crate::domain::Body;
use crate::domain::group::SelectionGroup;
use crate::domain::layers::LayerMask32;
use crate::domain::settings::SnapConfig;
use crate::domain::shape::ShapeDef;
use crate::geometry::contours::point_in_ring;
use crate::geometry::polygonize::polygonize;
use crate::geometry::scale::scale_point;
use crate::interaction::PointerOverUi;
use crate::interaction::gesture::GestureConstraints;
use crate::interaction::selection::{Selection, expand_groups};
use crate::interaction::snap::SnappedCursor;
use crate::interaction::tools::handles::{
    HandleKind, ScaleFrame, SelectionBox, hit_handle, selection_box,
};
use crate::interaction::tools::{ActiveGesture, topmost_body_at};
use crate::physics::hold::KinematicHold;
use crate::physics::queries::PhysicsQueries;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Minimum world-space delta before a click becomes a move commit.
const MOVE_EPSILON: f32 = 0.5;
/// Screen-space handle capture radius (logical px).
const HANDLE_RADIUS_PX: f32 = 10.0;

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
    Rotate {
        /// Rotation pivot (selection centroid).
        pivot: Vec2,
        /// Cursor angle at press.
        start_angle: f32,
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

/// Read-only inputs for the select tool.
#[derive(bevy::ecs::system::SystemParam)]
pub struct SelectInputs<'w> {
    buttons: Res<'w, crate::interaction::pointer::PointerButtons>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    snapped: Res<'w, SnappedCursor>,
    over_ui: Res<'w, PointerOverUi>,
    constraints: Res<'w, GestureConstraints>,
    config: Res<'w, SnapConfig>,
    frame: Res<'w, ScaleFrame>,
}

/// Mutable editor state driven by the select tool.
#[derive(bevy::ecs::system::SystemParam)]
pub struct SelectState<'w> {
    gesture: ResMut<'w, SelectGesture>,
    selection: ResMut<'w, Selection>,
    active: ResMut<'w, ActiveGesture>,
    hold: ResMut<'w, KinematicHold>,
    exclusions: ResMut<'w, crate::interaction::snap::SnapExclusions>,
}

/// Intent writers for gesture commits.
#[derive(bevy::ecs::system::SystemParam)]
pub struct SelectCommits<'w> {
    moves: MessageWriter<'w, CommitTransformIntent>,
    scales: MessageWriter<'w, ScaleIntent>,
    duplicates: MessageWriter<'w, DuplicateIntent>,
}

/// The select tool state machine (runs while the select tool is active).
// One function on purpose: the press/drag/release arms share gesture state,
// and splitting them would smear one state machine across several systems.
#[expect(clippy::too_many_lines)]
pub fn run_select_tool(
    inputs: SelectInputs,
    mut state: SelectState,
    mut commits: SelectCommits,
    physics: PhysicsQueries,
    mut transforms: ParamSet<(
        Query<(&ShapeDef, &LayerMask32), With<Body>>,
        Query<(&ShapeDef, &Transform), With<Body>>,
        Query<&mut Transform, With<Body>>,
        Query<(Entity, &Transform), With<Body>>,
    )>,
    ids: Query<&StableId>,
    groups: Query<(Entity, &SelectionGroup), With<Body>>,
    projections: Query<&Projection, With<Camera3d>>,
) {
    let SelectInputs {
        buttons,
        keys,
        snapped,
        over_ui,
        constraints,
        config,
        frame,
    } = &inputs;
    let SelectState {
        gesture,
        selection,
        active,
        hold,
        exclusions,
    } = &mut state;
    let cursor = snapped.effective();
    let cam_scale = crate::interaction::camera::camera_scale(&projections);

    // ---- Press: start a gesture. ----
    if buttons.just_pressed(MouseButton::Left)
        && !over_ui.0
        && let Some(p) = cursor
    {
        {
            let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
            let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

            // 1. Scale handle?
            let sbox = selection_box(selection, **frame, &transforms.p1());
            if let Some(sbox) = sbox
                && let Some(handle) = hit_handle(&sbox, p, HANDLE_RADIUS_PX * cam_scale)
            {
                {
                    let pivot = sbox.point(handle.anchor_unit());
                    let start_f = sbox.to_frame(p, pivot);
                    let targets: Vec<StableId> = selection
                        .iter()
                        .filter_map(|e| ids.get(e).ok().copied())
                        .collect();
                    let ghosts: Vec<(ShapeDef, PosRot)> = selection
                        .iter()
                        .filter_map(|e| {
                            transforms
                                .p1()
                                .get(e)
                                .ok()
                                .map(|(s, t)| (s.clone(), PosRot::from_transform(t)))
                        })
                        .collect();
                    if !targets.is_empty() {
                        **gesture = SelectGesture::Scale {
                            handle,
                            sbox,
                            pivot,
                            start_f,
                            factors: Vec2::ONE,
                            targets,
                            ghosts,
                        };
                        active.0 = true;
                        return;
                    }
                }
            }

            // 2. Body under cursor?
            if let Some(hit) = topmost_body_at(p, &physics, &transforms.p0()) {
                let mut members = vec![hit];
                expand_groups(&mut members, &groups);
                if ctrl {
                    // Duplicate-drag the hit (or the whole selection if the
                    // hit was already selected).
                    let source_entities: Vec<Entity> = if selection.contains(hit) {
                        selection.iter().collect()
                    } else {
                        members.clone()
                    };
                    let sources: Vec<StableId> = source_entities
                        .iter()
                        .filter_map(|e| ids.get(*e).ok().copied())
                        .collect();
                    let ghosts: Vec<(ShapeDef, PosRot)> = source_entities
                        .iter()
                        .filter_map(|e| {
                            transforms
                                .p1()
                                .get(*e)
                                .ok()
                                .map(|(s, t)| (s.clone(), PosRot::from_transform(t)))
                        })
                        .collect();
                    if !sources.is_empty() {
                        **gesture = SelectGesture::DupDrag {
                            start: p,
                            sources,
                            ghosts,
                        };
                        active.0 = true;
                    }
                    return;
                }
                if shift {
                    for m in &members {
                        selection.toggle(*m);
                    }
                    return; // toggle click, no drag gesture
                }
                if !selection.contains(hit) {
                    selection.clear();
                    for m in &members {
                        selection.add(*m);
                    }
                }
                // Start moving the selection.
                let poses = transforms.p1();
                let bodies: Vec<(Entity, StableId, PosRot)> = selection
                    .iter()
                    .filter_map(|e| {
                        let id = ids.get(e).ok().copied()?;
                        let (_, t) = poses.get(e).ok()?;
                        Some((e, id, PosRot::from_transform(t)))
                    })
                    .collect();
                hold.entities = bodies.iter().map(|(e, ..)| *e).collect();
                exclusions.0.clone_from(&hold.entities);
                **gesture = SelectGesture::Move { start: p, bodies };
                active.0 = true;
                return;
            }

            // 3. Empty canvas → rubber band (Alt = freeform loop).
            let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
            **gesture = if alt {
                SelectGesture::Lasso {
                    points: vec![p],
                    additive: shift,
                }
            } else {
                SelectGesture::BoxSelect {
                    start: p,
                    additive: shift,
                }
            };
            active.0 = true;
        }
    }

    // Right press over the selection starts rotation.
    if buttons.just_pressed(MouseButton::Right)
        && !over_ui.0
        && matches!(**gesture, SelectGesture::Idle)
        && !selection.is_empty()
        && let Some(p) = cursor
    {
        {
            let over_selected = topmost_body_at(p, &physics, &transforms.p0())
                .is_some_and(|hit| selection.contains(hit));
            if over_selected {
                let poses = transforms.p1();
                let bodies: Vec<(Entity, StableId, PosRot)> = selection
                    .iter()
                    .filter_map(|e| {
                        let id = ids.get(e).ok().copied()?;
                        let (_, t) = poses.get(e).ok()?;
                        Some((e, id, PosRot::from_transform(t)))
                    })
                    .collect();
                let pivot =
                    bodies.iter().map(|(_, _, p)| p.pos).sum::<Vec2>() / bodies.len().max(1) as f32;
                let start_angle = (p - pivot).to_angle();
                hold.entities = bodies.iter().map(|(e, ..)| *e).collect();
                exclusions.0.clone_from(&hold.entities);
                **gesture = SelectGesture::Rotate {
                    pivot,
                    start_angle,
                    bodies,
                };
                active.0 = true;
            }
        }
    }

    // ---- Drag: update previews. ----
    match &mut **gesture {
        SelectGesture::Move { start, bodies } => {
            if let Some(p) = cursor {
                let delta = constraints.constrain(p - *start);
                for (entity, _, original) in bodies.iter() {
                    if let Ok(mut t) = transforms.p2().get_mut(*entity) {
                        PosRot {
                            pos: original.pos + delta,
                            rot: original.rot,
                        }
                        .apply_to(&mut t);
                    }
                }
            }
        }
        SelectGesture::Rotate {
            pivot,
            start_angle,
            bodies,
        } => {
            if let Some(p) = snapped.raw {
                let mut angle = (p - *pivot).to_angle() - *start_angle;
                angle = constraints.apply_rotation(angle, config);
                let rot = Vec2::from_angle(angle);
                for (entity, _, original) in bodies.iter() {
                    if let Ok(mut t) = transforms.p2().get_mut(*entity) {
                        PosRot {
                            pos: *pivot + rot.rotate(original.pos - *pivot),
                            rot: original.rot + angle,
                        }
                        .apply_to(&mut t);
                    }
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
            if let Some(p) = cursor {
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
                if sx && sy && keys.pressed(KeyCode::ShiftLeft) {
                    let m = if f.x.abs() > f.y.abs() { f.x } else { f.y };
                    f = Vec2::new(m.abs().copysign(f.x), m.abs().copysign(f.y));
                }
                *factors = f;
            }
        }
        SelectGesture::Lasso { points, .. } => {
            if let Some(p) = snapped.raw
                && points.last().is_none_or(|last| last.distance(p) > 4.0)
            {
                points.push(p);
            }
        }
        SelectGesture::Idle | SelectGesture::BoxSelect { .. } | SelectGesture::DupDrag { .. } => {}
    }

    // ---- Release: commit exactly one intent. ----
    let left_up = buttons.just_released(MouseButton::Left);
    let right_up = buttons.just_released(MouseButton::Right);
    if !(left_up || right_up) {
        return;
    }
    match std::mem::take(&mut **gesture) {
        SelectGesture::Move { bodies, .. } if left_up => {
            hold.entities.clear();
            exclusions.0.clear();
            active.0 = false;
            let changes: Vec<TransformChange> = bodies
                .iter()
                .filter_map(|(entity, id, old)| {
                    let t = transforms.p2().get(*entity).ok().copied()?;
                    let new = PosRot::from_transform(&t);
                    (new.pos.distance(old.pos) > MOVE_EPSILON).then_some(TransformChange {
                        id: *id,
                        old: *old,
                        new,
                    })
                })
                .collect();
            if !changes.is_empty() {
                commits.moves.write(CommitTransformIntent { changes });
            }
        }
        SelectGesture::Rotate { bodies, .. } if right_up => {
            hold.entities.clear();
            exclusions.0.clear();
            active.0 = false;
            let changes: Vec<TransformChange> = bodies
                .iter()
                .filter_map(|(entity, id, old)| {
                    let t = transforms.p2().get(*entity).ok().copied()?;
                    let new = PosRot::from_transform(&t);
                    ((new.rot - old.rot).abs() > 1e-4).then_some(TransformChange {
                        id: *id,
                        old: *old,
                        new,
                    })
                })
                .collect();
            if !changes.is_empty() {
                commits.moves.write(CommitTransformIntent { changes });
            }
        }
        SelectGesture::BoxSelect { start, additive } if left_up => {
            active.0 = false;
            if let Some(p) = cursor {
                if !additive {
                    selection.clear();
                }
                let (min, max) = (start.min(p), start.max(p));
                let mut hits: Vec<Entity> = physics
                    .bodies_in_aabb(min, max)
                    .into_iter()
                    .filter(|e| transforms.p0().contains(*e))
                    .collect();
                expand_groups(&mut hits, &groups);
                for e in hits {
                    selection.add(e);
                }
            }
        }
        SelectGesture::Lasso { points, additive } if left_up => {
            active.0 = false;
            if points.len() >= 3 {
                if !additive {
                    selection.clear();
                }
                // CONTRACT: a body is loop-selected iff its center lies
                // inside the drawn ring (matching Algodoo); group members
                // come along with any selected member.
                let mut hits: Vec<Entity> = transforms
                    .p3()
                    .iter()
                    .filter(|(_, t)| point_in_ring(t.translation.truncate(), &points))
                    .map(|(e, _)| e)
                    .collect();
                expand_groups(&mut hits, &groups);
                for e in hits {
                    selection.add(e);
                }
            }
        }
        SelectGesture::DupDrag { start, sources, .. } if left_up => {
            active.0 = false;
            if let Some(p) = cursor {
                let offset = constraints.constrain(p - start);
                if offset.length() > MOVE_EPSILON {
                    commits
                        .duplicates
                        .write(DuplicateIntent { sources, offset });
                }
            }
        }
        SelectGesture::Scale {
            sbox,
            pivot,
            factors,
            targets,
            ..
        } if left_up => {
            active.0 = false;
            if (factors - Vec2::ONE).length() > 1e-3 {
                commits.scales.write(ScaleIntent {
                    targets,
                    pivot,
                    frame_rot: sbox.rot,
                    factors,
                });
            }
        }
        other => {
            // Released the other button mid-gesture: keep going.
            **gesture = other;
        }
    }
}

/// Draws gizmo previews for box-select, duplicate ghosts, and scale.
pub fn draw_select_previews(
    gesture: Res<SelectGesture>,
    snapped: Res<SnappedCursor>,
    constraints: Res<GestureConstraints>,
    mut gizmos: Gizmos,
) {
    match &*gesture {
        SelectGesture::BoxSelect { start, .. } => {
            if let Some(p) = snapped.effective() {
                let center = (*start + p) / 2.0;
                let size = (p - *start).abs();
                gizmos.rect_2d(
                    Isometry2d::from_translation(center),
                    size,
                    css::LIGHT_SKY_BLUE.with_alpha(0.8),
                );
            }
        }
        SelectGesture::Lasso { points, .. } => {
            if points.len() >= 2 {
                let mut pts = points.clone();
                if let Some(first) = pts.first().copied() {
                    pts.push(first);
                }
                gizmos.linestrip_2d(pts, css::LIGHT_SKY_BLUE.with_alpha(0.8));
            }
        }
        SelectGesture::DupDrag { start, ghosts, .. } => {
            if let Some(p) = snapped.effective() {
                let offset = constraints.constrain(p - *start);
                for (shape, pose) in ghosts {
                    draw_shape_outline(
                        &mut gizmos,
                        shape,
                        PosRot {
                            pos: pose.pos + offset,
                            rot: pose.rot,
                        },
                        css::AQUAMARINE.with_alpha(0.8).into(),
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
            // Preview: each selected contour mapped through the frame
            // scale about the pivot (world-space, exact).
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
                    gizmos.linestrip_2d(pts, css::GOLD.with_alpha(0.9));
                }
            }
        }
        _ => {}
    }
}

fn draw_shape_outline(gizmos: &mut Gizmos, shape: &ShapeDef, pose: PosRot, color: Color) {
    let rot = Vec2::from_angle(pose.rot);
    for ring in polygonize(shape).rings() {
        let mut pts: Vec<Vec2> = ring.iter().map(|v| pose.pos + rot.rotate(*v)).collect();
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
        gizmos.linestrip_2d(pts, color);
    }
}
