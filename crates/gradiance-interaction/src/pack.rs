//! The close-packing session: run a [`gradiance_optimize`] solver over the
//! selection across frames, preview it, and commit it as one command.
//!
//! This is the ECS half of the optimizer. The solver itself is pure math in
//! `gradiance-optimize`; everything here is the editor plumbing around it:
//!
//! ```text
//! StartPackRequest ─▶ start_pack   (gather selection geometry → PackProblem)
//!                        │
//!                        ▼
//!                   PackSession ──▶ step_pack   (advance N iterations/frame)
//!                        │     └──▶ draw_pack_preview  (the ghost)
//!                        ▼
//!                  finish_pack ──▶ CommitTransformIntent  (exactly one)
//! ```
//!
//! # Why it behaves like a tool
//!
//! A pack obeys the same gesture contract as every drag: while it runs it
//! writes **nothing** — not authored components, not the command stack, only
//! gizmos — and on acceptance it emits **exactly one**
//! [`CommitTransformIntent`], so the whole rearrangement is a single undo
//! step no matter how many bodies moved or how many thousand iterations it
//! took. `Escape` cancels and leaves the scene untouched.
//!
//! It is not a [`ToolState`](gradiance_core::states::ToolState) because it is
//! not a pointer gesture: there is no press, drag, or release, and it must be
//! able to run while the user keeps working in the inspector. The session
//! resource *is* the mode.
//!
//! # Turning a solved layout back into body poses
//!
//! One packing item can stand for several bodies (a selection group moves
//! rigidly), and an item's pose refers to its **hull centroid**, not to any
//! body's origin — which for a CSG-reshaped body can be far off-centre. So a
//! target records the body's authored pose *and* the item pivot it was
//! measured against, and the commit re-derives each body's pose from the
//! item's rigid motion:
//!
//! ```text
//! Δrot   = final.rot − start.rot
//! new_pos = final.pos + R(Δrot) · (body_pos − pivot)
//! new_rot = body_rot + Δrot
//! ```

use bevy::prelude::*;
use gradiance_command::intent::{CommitTransformIntent, TransformChange};
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::group::SelectionGroup;
use gradiance_domain::shape::ShapeDef;
use gradiance_optimize::{
    Layout, PackConfig, PackItem, PackProblem, PackReport, PackRun, RunStatus,
};

use crate::overlay::OverlayGizmos;
use crate::selection::Selection;

/// Request to start (or restart) a packing run over the current selection.
///
/// Written by the UI — the context menu entry and the inspector's Optimizer
/// panel. Starting is not a world mutation, so this is a plain request
/// message rather than a command intent.
#[derive(Message, Debug, Clone, Copy, Default, Reflect)]
pub struct StartPackRequest;

/// What to do with a session that is already running.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum PackControl {
    /// Apply the best layout found so far and end the session.
    Apply,
    /// Discard everything and leave the scene as it was.
    Cancel,
}

/// One body moved by a packing item.
#[derive(Debug, Clone, Copy)]
struct PackTarget {
    /// Index of the packing item this body belongs to.
    item: usize,
    /// The body.
    id: StableId,
    /// The body's authored pose when the session started.
    start: PosRot,
    /// The item's hull centroid at session start — the pivot the item's
    /// rotation is measured about.
    pivot: Vec2,
}

/// The live packing session, or nothing.
///
/// Editor state, not authored state: it is never serialized, never captured
/// in an undo record, and holds no `Entity` across frames (targets are
/// [`StableId`]s, so a despawn mid-run just drops that body from the commit).
#[derive(Resource, Default)]
pub struct PackSession {
    run: Option<PackRun>,
    targets: Vec<PackTarget>,
    /// Set when a finished run has been applied, so the ghost stops drawing
    /// on the frame the intent goes out.
    committed: bool,
}

impl std::fmt::Debug for PackSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackSession")
            .field("active", &self.run.is_some())
            .field("bodies", &self.targets.len())
            .field("committed", &self.committed)
            .finish_non_exhaustive()
    }
}

impl PackSession {
    /// Whether a run is loaded (running or finished but not yet applied).
    pub fn is_active(&self) -> bool {
        self.run.is_some()
    }

    /// A snapshot of the run for the UI, if there is one.
    pub fn report(&self) -> Option<PackReport> {
        self.run.as_ref().map(PackRun::report)
    }

    /// The run's status, if there is one.
    pub fn status(&self) -> Option<RunStatus> {
        self.run.as_ref().map(PackRun::status)
    }

    /// How many bodies the session is arranging.
    pub fn body_count(&self) -> usize {
        self.targets.len()
    }

    /// Forgets the session without touching the world.
    pub fn clear(&mut self) {
        self.run = None;
        self.targets.clear();
        self.committed = false;
    }
}

/// Everything the packer reads off a selected body.
type BodyQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static ShapeDef,
        &'static Transform,
        &'static DepthBand,
        Option<&'static SelectionGroup>,
        Option<&'static gradiance_domain::props::BodyPhysics>,
    ),
    With<Body>,
>;

/// Builds a [`PackProblem`] from the selection and starts a run.
///
/// Bodies contribute their **world-space outline**, so the solver sees what
/// the user sees. Two kinds are skipped: ground half-planes (an infinite
/// plane has no footprint to pack, and moving the floor is never what the
/// user meant) and anything whose shape fails to contour.
pub fn start_pack(
    mut requests: MessageReader<StartPackRequest>,
    mut session: ResMut<PackSession>,
    config: Res<PackConfig>,
    selection: Res<Selection>,
    bodies: BodyQuery,
) {
    // Only the last request in a frame matters; a session is singular.
    if requests.read().last().is_none() {
        return;
    }
    session.clear();

    let (problem, targets) = build_problem(&selection, &bodies, &config);
    if problem.len() < 2 {
        return;
    }
    session.targets = targets;
    session.run = Some(PackRun::new(problem));
}

/// One gathered selection member, before grouping.
struct Gathered {
    id: StableId,
    outline: Vec<Vec2>,
    pose: PosRot,
    layers: u32,
    group: Option<u32>,
    pinned: bool,
}

/// Gathers the selection into packing items plus the body table that maps a
/// solved layout back onto transforms.
fn build_problem(
    selection: &Selection,
    bodies: &BodyQuery<'_, '_>,
    config: &PackConfig,
) -> (PackProblem, Vec<PackTarget>) {
    let mut gathered: Vec<Gathered> = Vec::new();
    for entity in selection.iter() {
        let Ok((id, shape, transform, band, group, body_kind)) = bodies.get(entity) else {
            continue;
        };
        // An infinite ground plane has no footprint worth packing, and
        // rearranging the floor under everything is never the intent.
        if shape.contains_half_plane() {
            continue;
        }
        let pose = PosRot::from_transform(transform);
        let outline = world_outline(shape, pose);
        if outline.len() < 3 {
            continue;
        }
        gathered.push(Gathered {
            id: *id,
            outline,
            pose,
            layers: band.sanitized().bits(),
            group: group.and_then(SelectionGroup::outermost),
            // A static body is scenery the rest should pack around, which is
            // the same role the solver's "pinned" flag describes.
            pinned: matches!(
                body_kind.map(|p| p.kind),
                Some(gradiance_domain::props::BodyKind::Static)
            ),
        });
    }

    // Partition into items: one per group (when groups are kept rigid), one
    // per body otherwise.
    let mut items: Vec<PackItem> = Vec::new();
    let mut targets: Vec<PackTarget> = Vec::new();
    let mut buckets: Vec<(Option<u32>, Vec<usize>)> = Vec::new();
    for (index, g) in gathered.iter().enumerate() {
        let key = if config.keep_groups { g.group } else { None };
        match key.and_then(|k| buckets.iter_mut().find(|(b, _)| *b == Some(k))) {
            Some((_, members)) => members.push(index),
            None => buckets.push((key, vec![index])),
        }
    }

    for (_, members) in buckets {
        let mut outline: Vec<Vec2> = Vec::new();
        let mut layers = 0u32;
        let mut pinned = false;
        for m in &members {
            let Some(g) = gathered.get(*m) else {
                continue;
            };
            outline.extend_from_slice(&g.outline);
            layers |= g.layers;
            pinned |= g.pinned;
        }
        if outline.len() < 3 {
            continue;
        }
        // A multi-body item has no single authored angle; measuring it at
        // zero keeps the group's internal arrangement rigid and makes the
        // solved `rot` a clean delta.
        let rot = match members.as_slice() {
            [only] => gathered.get(*only).map_or(0.0, |g| g.pose.rot),
            _ => 0.0,
        };
        let item = PackItem::from_world_outline(&outline, rot, layers, pinned);
        let pivot = item.start.pos;
        let item_index = items.len();
        items.push(item);
        for m in members {
            let Some(g) = gathered.get(m) else {
                continue;
            };
            targets.push(PackTarget {
                item: item_index,
                id: g.id,
                start: g.pose,
                pivot,
            });
        }
    }

    (PackProblem::new(items, config.clone()), targets)
}

/// A body's outline in world space, through the one discretization point.
fn world_outline(shape: &ShapeDef, pose: PosRot) -> Vec<Vec2> {
    let contours = gradiance_geometry::polygonize::polygonize(shape);
    let (sin, cos) = pose.rot.sin_cos();
    contours
        .rings()
        .flatten()
        .map(|v| {
            Vec2::new(
                pose.pos.x + v.x * cos - v.y * sin,
                pose.pos.y + v.x * sin + v.y * cos,
            )
        })
        .collect()
}

/// Advances the run by the configured per-frame budget.
pub fn step_pack(mut session: ResMut<PackSession>, config: Res<PackConfig>) {
    let budget = config.iterations_per_frame;
    let Some(run) = session.run.as_mut() else {
        return;
    };
    if run.status().is_done() {
        return;
    }
    run.advance(budget);
}

/// Applies a finished run (or an explicit Apply), cancels on request or on
/// `Escape`, and emits the single transform command.
pub fn finish_pack(
    mut controls: MessageReader<PackControl>,
    keys: Res<ButtonInput<KeyCode>>,
    keyboard_captured: Res<crate::KeyboardCaptured>,
    config: Res<PackConfig>,
    mut session: ResMut<PackSession>,
    mut moves: MessageWriter<CommitTransformIntent>,
) {
    if !session.is_active() {
        return;
    }

    let mut apply = false;
    let mut cancel = false;
    for control in controls.read() {
        match control {
            PackControl::Apply => apply = true,
            PackControl::Cancel => cancel = true,
        }
    }
    // Escape aborts, matching every other gesture — but not while a text
    // field owns the keyboard.
    if !keyboard_captured.0 && keys.just_pressed(KeyCode::Escape) {
        cancel = true;
    }

    if cancel {
        session.clear();
        return;
    }

    let finished = session.status().is_some_and(RunStatus::is_done);
    if !apply && !finished {
        return;
    }
    if !apply && !config.auto_apply {
        return;
    }

    let changes = session.pending_changes();
    session.clear();
    if !changes.is_empty() {
        moves.write(CommitTransformIntent { changes });
    }
}

impl PackSession {
    /// The pose changes the current best layout implies — the payload of the
    /// single command a session commits.
    ///
    /// Public so a test (or a future script verb) can inspect what a run
    /// would do without going through the message plumbing.
    pub fn pending_changes(&self) -> Vec<TransformChange> {
        let Some(run) = self.run.as_ref() else {
            return Vec::new();
        };
        layout_changes(run.problem(), run.best_layout(), &self.targets)
    }
}

/// Turns a solved layout into per-body pose changes.
fn layout_changes(
    problem: &PackProblem,
    layout: &Layout,
    targets: &[PackTarget],
) -> Vec<TransformChange> {
    let mut changes = Vec::with_capacity(targets.len());
    for target in targets {
        let (Some(item), Some(pose)) = (
            problem.items.get(target.item),
            layout.poses.get(target.item),
        ) else {
            continue;
        };
        let delta_rot = pose.rot - item.start.rot;
        let (sin, cos) = delta_rot.sin_cos();
        let offset = target.start.pos - target.pivot;
        let new = PosRot {
            pos: pose.pos
                + Vec2::new(
                    offset.x * cos - offset.y * sin,
                    offset.x * sin + offset.y * cos,
                ),
            rot: target.start.rot + delta_rot,
        };
        // Skip bodies the solver left where they were: an unchanged pose in
        // an undo record is noise.
        if new.pos.distance(target.start.pos) < 1e-4 && (new.rot - target.start.rot).abs() < 1e-5 {
            continue;
        }
        changes.push(TransformChange {
            id: target.id,
            old: target.start,
            new,
        });
    }
    changes
}

/// Draws the optimizer ghost: the layout being searched right now, the best
/// one found, and the bounding box being shrunk.
///
/// Two layers on purpose. The faint outlines are the *working* layout, which
/// moves every frame and is what makes the search legible — you can see
/// relaxation clench and annealing rattle. The bright ones are the best
/// layout, which is what pressing Apply would actually do.
pub fn draw_pack_preview(session: Res<PackSession>, mut gizmos: Gizmos<OverlayGizmos>) {
    use bevy::color::palettes::css;

    let Some(run) = session.run.as_ref() else {
        return;
    };
    if session.committed {
        return;
    }
    let problem = run.problem();
    let done = run.status().is_done();

    if !done {
        for (i, item) in problem.items.iter().enumerate() {
            let Some(pose) = run.working_layout().poses.get(i) else {
                continue;
            };
            outline(
                &mut gizmos,
                &item.placed(*pose),
                css::SLATE_GRAY.with_alpha(0.35),
            );
        }
    }

    let accent = if done {
        css::SPRING_GREEN
    } else {
        css::AQUAMARINE
    };
    for (i, item) in problem.items.iter().enumerate() {
        let Some(pose) = run.best_layout().poses.get(i) else {
            continue;
        };
        let color = if problem.movable(i) {
            accent
        } else {
            // Pinned obstacles are drawn differently: they are constraints,
            // not results, and it should be obvious they will not move.
            css::ORANGE.with_alpha(0.6)
        };
        outline(&mut gizmos, &item.placed(*pose), color);
    }

    // The box being minimized.
    let (min, max) = run.report().best.bounds;
    let size = max - min;
    if size.x > 0.0 && size.y > 0.0 {
        gizmos.rect_2d(
            Isometry2d::from_translation((min + max) * 0.5),
            size,
            accent.with_alpha(0.5),
        );
    }
}

/// Draws a closed polygon.
fn outline(gizmos: &mut Gizmos<OverlayGizmos>, poly: &[Vec2], color: impl Into<Color> + Copy) {
    for i in 0..poly.len() {
        gizmos.line_2d(poly[i], poly[(i + 1) % poly.len()], color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_optimize::{Metrics, Scratch, SolverKind, metrics};

    fn square_item(center: Vec2, half: f32) -> PackItem {
        PackItem::from_world_outline(
            &[
                center + Vec2::new(-half, -half),
                center + Vec2::new(half, -half),
                center + Vec2::new(half, half),
                center + Vec2::new(-half, half),
            ],
            0.0,
            1,
            false,
        )
    }

    #[test]
    fn a_solved_layout_becomes_one_pose_change_per_moved_body() {
        let items = vec![
            square_item(Vec2::ZERO, 0.5),
            square_item(Vec2::new(9.0, 0.0), 0.5),
        ];
        let targets = vec![
            PackTarget {
                item: 0,
                id: StableId::new(),
                start: items[0].start,
                pivot: items[0].start.pos,
            },
            PackTarget {
                item: 1,
                id: StableId::new(),
                start: items[1].start,
                pivot: items[1].start.pos,
            },
        ];
        let problem = PackProblem::new(
            items,
            PackConfig {
                solver: SolverKind::Shelf,
                ..Default::default()
            },
        );
        let mut run = PackRun::new(problem);
        run.solve();
        let changes = layout_changes(run.problem(), run.best_layout(), &targets);
        assert_eq!(changes.len(), 2, "both bodies moved closer together");
        for (change, target) in changes.iter().zip(&targets) {
            assert_eq!(change.id, target.id);
            assert_eq!(change.old, target.start);
        }
    }

    #[test]
    fn an_unchanged_layout_produces_no_changes_at_all() {
        // Committing a no-op must not push an empty-but-present undo step.
        let items = vec![
            square_item(Vec2::ZERO, 0.5),
            square_item(Vec2::new(3.0, 0.0), 0.5),
        ];
        let targets: Vec<PackTarget> = items
            .iter()
            .enumerate()
            .map(|(i, item)| PackTarget {
                item: i,
                id: StableId::new(),
                start: item.start,
                pivot: item.start.pos,
            })
            .collect();
        let problem = PackProblem::new(items, PackConfig::default());
        let layout = Layout::from_starts(&problem.items);
        assert!(layout_changes(&problem, &layout, &targets).is_empty());
    }

    #[test]
    fn a_group_moves_rigidly_and_keeps_its_internal_spacing() {
        // One item standing for two bodies: both must receive the same
        // translation, so their separation survives the pack.
        let group = PackItem::from_world_outline(
            &[
                Vec2::new(-1.0, -0.5),
                Vec2::new(1.0, -0.5),
                Vec2::new(1.0, 0.5),
                Vec2::new(-1.0, 0.5),
            ],
            0.0,
            1,
            false,
        );
        let pivot = group.start.pos;
        let member_a = PosRot {
            pos: Vec2::new(-0.5, 0.0),
            rot: 0.0,
        };
        let member_b = PosRot {
            pos: Vec2::new(0.5, 0.0),
            rot: 0.0,
        };
        let targets = vec![
            PackTarget {
                item: 0,
                id: StableId::new(),
                start: member_a,
                pivot,
            },
            PackTarget {
                item: 0,
                id: StableId::new(),
                start: member_b,
                pivot,
            },
        ];
        let problem = PackProblem::new(vec![group], PackConfig::default());
        let moved = Layout {
            poses: vec![PosRot {
                pos: Vec2::new(7.0, 3.0),
                rot: 0.0,
            }],
        };
        let changes = layout_changes(&problem, &moved, &targets);
        assert_eq!(changes.len(), 2);
        let delta_a = changes[0].new.pos - changes[0].old.pos;
        let delta_b = changes[1].new.pos - changes[1].old.pos;
        assert!(
            delta_a.distance(delta_b) < 1e-5,
            "the group translated rigidly"
        );
        let spacing = changes[1].new.pos - changes[0].new.pos;
        assert!(
            spacing.distance(Vec2::new(1.0, 0.0)) < 1e-5,
            "spacing preserved"
        );
    }

    #[test]
    fn a_rotated_item_carries_its_bodies_around_the_hull_pivot() {
        // The body sits away from the item pivot, so a pure rotation of the
        // item must swing it — not just spin it in place.
        let item = square_item(Vec2::ZERO, 1.0);
        let pivot = item.start.pos;
        let body = PosRot {
            pos: Vec2::new(1.0, 0.0),
            rot: 0.0,
        };
        let targets = vec![PackTarget {
            item: 0,
            id: StableId::new(),
            start: body,
            pivot,
        }];
        let problem = PackProblem::new(vec![item], PackConfig::default());
        let quarter = std::f32::consts::FRAC_PI_2;
        let turned = Layout {
            poses: vec![PosRot {
                pos: pivot,
                rot: quarter,
            }],
        };
        let changes = layout_changes(&problem, &turned, &targets);
        assert_eq!(changes.len(), 1);
        assert!(
            changes[0].new.pos.distance(Vec2::new(0.0, 1.0)) < 1e-5,
            "a quarter turn about the pivot swings (1,0) to (0,1), got {:?}",
            changes[0].new.pos
        );
        assert!((changes[0].new.rot - quarter).abs() < 1e-5);
    }

    #[test]
    fn the_committed_layout_is_the_one_the_report_scored() {
        let items: Vec<PackItem> = (0..5)
            .map(|i| square_item(Vec2::new(i as f32 * 4.0, 0.0), 0.5))
            .collect();
        let mut run = PackRun::new(PackProblem::new(items, PackConfig::default()));
        run.solve();
        let mut scratch = Scratch::new(run.problem().len());
        let recomputed: Metrics = metrics(run.problem(), run.best_layout(), &mut scratch);
        assert!(
            recomputed.is_feasible(),
            "never commit an overlapping layout"
        );
        assert!((recomputed.extent - run.report().best.extent).abs() < 1e-3);
    }
}
