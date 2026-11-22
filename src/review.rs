//! Review mode's loader: turns a [`ReviewShowRequest`] into a live scene
//! (brief 4b-0).
//!
//! Lives in the app shell because an option is a scene file **and**
//! scripts, and no layer below the shell sees both `persist` and `script`.
//! The session state, the formats and the answers write are lower down
//! (`persist::review`, `scene::review`); the panel is `ui::review`.
//!
//! A show request becomes a short **job**, one step per frame:
//!
//! 1. *Load* the option's base scene — its `.ron`, or an empty scene with
//!    default scene settings — with its settings overrides applied, through
//!    the ordinary [`LoadSceneIntent`]. The sim is paused first.
//! 2. *Script* each of its `.scm` files, one frame apart, through
//!    [`ScriptInputs`] — so a later script can index what an earlier one
//!    spawned. A script error is reported in the session's problems.
//! 3. Side by side only: *capture* the result, repeat 1–2 for every
//!    option, then load all of them as one scene, laid out left to right
//!    ([`lay_out_side_by_side`]).
//! 4. *Settle* a few frames (the settings-edit debounce), frame the camera,
//!    and *clear* the undo history ([`ClearHistoryIntent`]) — the
//!    demonstration leaves no undo steps behind.
//!
//! Every write goes through an existing seam: scene loads and the history
//! reset are intents, scripts are script runs, and the camera is view state
//! (like the view cube's).

use crate::command::CommandDispatchSet;
use crate::command::intent::{ClearHistoryIntent, LoadSceneIntent};
use crate::core::ids::StableId;
use crate::core::messages::drain;
use crate::core::states::GameState;
use crate::geometry::sdf::aabb;
use crate::interaction::camera::CameraRig;
use crate::persist::review::{ReviewRegion, ReviewSession, ReviewShowRequest, ReviewView};
use crate::scene::review::ReviewOption;
use crate::scene::{EnvironmentRecord, SceneRecord};
use crate::script::bridge::{RunReply, ScriptInputs, run_scripts};
use bevy::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::path::Path;

/// Gap between side-by-side regions, metres.
pub const SIDE_BY_SIDE_GAP: f32 = 2.0;

/// Frames to wait after the last load before clearing history: the
/// settings-edit debounce (`commit_settings_edits`) records a changed
/// setting on the frame after it settles, and that step must land before the
/// reset, not after it.
const SETTLE_FRAMES: u32 = 3;

/// Margin around framed content (fraction of its size).
const FRAME_MARGIN: f32 = 1.2;

/// Installs the review loader. Inert unless a [`ReviewSession`] exists.
#[derive(Default)]
pub struct ReviewPlugin;

impl Plugin for ReviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            show_first_question.run_if(resource_exists::<ReviewSession>),
        );
        app.add_systems(
            Update,
            drive_review
                .before(run_scripts)
                .before(CommandDispatchSet)
                .run_if(resource_exists::<ReviewSession>),
        );
    }
}

/// Opens the first unanswered question (or the first one) on its first
/// option, so `--review` starts on something to judge.
fn show_first_question(session: Res<ReviewSession>, mut show: MessageWriter<ReviewShowRequest>) {
    if session.questions.questions.is_empty() {
        return;
    }
    show.write(ReviewShowRequest {
        question: session.first_unanswered().unwrap_or(0),
        view: ReviewView::Single(0),
    });
}

/// One frame's work in a review load.
enum Step {
    /// Load this scene (replaces the world).
    Load(Box<SceneRecord>),
    /// Run this script source (the path is for messages).
    Script(String, String),
    /// Record the world as this option's side-by-side part.
    Capture(String),
    /// Load every captured part, laid out side by side.
    LoadMerged,
    /// Do nothing this frame.
    Wait,
    /// Frame the camera: on the side-by-side regions, or the home view.
    Frame,
    /// Discard the undo history.
    Clear,
}

/// The load in progress.
#[derive(Resource)]
struct ReviewJob {
    question: usize,
    view: ReviewView,
    steps: VecDeque<Step>,
    /// A script run whose outcome is still to be read.
    pending: Option<(String, RunReply)>,
    captures: Vec<(String, SceneRecord)>,
    regions: Vec<ReviewRegion>,
    /// Reset the camera to the home view when done (a new question).
    home: bool,
}

/// The loader: starts a job per request, runs one step per frame.
fn drive_review(world: &mut World) {
    let requests: Vec<ReviewShowRequest> = drain(world);
    if let Some(request) = requests.last() {
        start_job(world, *request);
    }
    let Some(mut job) = world.remove_resource::<ReviewJob>() else {
        return;
    };
    if let Some((path, reply)) = job.pending.take() {
        let outcome = reply.lock().ok().and_then(|mut slot| slot.take());
        match outcome {
            Some(Err(e)) => problem(world, format!("{path}: {e}")),
            Some(Ok(_)) => {}
            // Not run yet (should not happen: scripts run the frame they are
            // submitted) — check again next frame.
            None => {
                job.pending = Some((path, reply));
                world.insert_resource(job);
                return;
            }
        }
    }
    let Some(step) = job.steps.pop_front() else {
        finish(world, job);
        return;
    };
    match step {
        Step::Load(scene) => {
            world.write_message(LoadSceneIntent { scene: *scene });
        }
        Step::Script(path, source) => {
            let reply = world
                .resource_mut::<ScriptInputs>()
                .submit_with_reply(source);
            job.pending = Some((path, reply));
        }
        Step::Capture(label) => {
            let scene = SceneRecord::capture(world);
            job.captures.push((label, scene));
        }
        Step::LoadMerged => {
            let (scene, regions, warnings) = lay_out_side_by_side(&job.captures, SIDE_BY_SIDE_GAP);
            for warning in warnings {
                problem(world, warning);
            }
            job.regions = regions;
            world.write_message(LoadSceneIntent { scene });
        }
        Step::Wait => {}
        Step::Frame => {
            let bounds = union(&job.regions);
            let inset = world
                .get_resource::<ReviewSession>()
                .map_or(0.0, |s| s.view_inset_right);
            frame_camera(world, bounds, job.home, inset);
        }
        Step::Clear => {
            world.write_message(ClearHistoryIntent);
        }
    }
    world.insert_resource(job);
}

fn problem(world: &mut World, message: String) {
    warn!("review: {message}");
    if let Some(mut session) = world.get_resource_mut::<ReviewSession>() {
        session.problems.push(message);
    }
}

fn finish(world: &mut World, job: ReviewJob) {
    if let Some(mut session) = world.get_resource_mut::<ReviewSession>() {
        session.busy = false;
        session.shown = Some((job.question, job.view));
        session.regions = job.regions;
    }
}

/// Builds the step list for a request and installs it as the job.
fn start_job(world: &mut World, request: ReviewShowRequest) {
    let Some(session) = world.get_resource::<ReviewSession>().cloned() else {
        return;
    };
    let Some(question) = session.questions.questions.get(request.question) else {
        problem(world, format!("no question {}", request.question));
        return;
    };
    let base = session.base_dir();
    // The empty base keeps the person's workstation config (grid, snap,
    // camera feel) and resets every scene-content setting to its default.
    let mut empty_env = EnvironmentRecord::capture(world);
    let defaults = EnvironmentRecord::default();
    empty_env.sim = defaults.sim;
    empty_env.render = defaults.render;
    empty_env.lighting = defaults.lighting;
    empty_env.scenery = defaults.scenery;
    empty_env.signals = defaults.signals;
    empty_env.params = defaults.params;
    empty_env.computed = defaults.computed;

    let mut problems = Vec::new();
    let mut steps = VecDeque::new();
    let options: Vec<(usize, &ReviewOption)> = match request.view {
        ReviewView::Single(i) => {
            let Some(option) = question.options.get(i) else {
                problem(world, format!("`{}` has no option {i}", question.id));
                return;
            };
            vec![(i, option)]
        }
        ReviewView::SideBySide => question.options.iter().enumerate().collect(),
    };
    for (_, option) in &options {
        match option_steps(option, &base, &empty_env) {
            Ok(mut option_steps) => {
                steps.append(&mut option_steps);
                if request.view == ReviewView::SideBySide {
                    // The last script's spawns land in its own frame; capture
                    // on the next.
                    steps.push_back(Step::Capture(option.label.clone()));
                }
            }
            Err(e) => problems.push(format!("option `{}`: {e}", option.label)),
        }
    }
    if request.view == ReviewView::SideBySide {
        steps.push_back(Step::LoadMerged);
    }
    for _ in 0..SETTLE_FRAMES {
        steps.push_back(Step::Wait);
    }
    let home = session.shown.is_none_or(|(q, view)| {
        q != request.question
            || view == ReviewView::SideBySide
            || request.view == ReviewView::SideBySide
    });
    steps.push_back(Step::Frame);
    steps.push_back(Step::Clear);
    // One frame for the clear to dispatch before the job reports done.
    steps.push_back(Step::Wait);

    if let Some(mut session) = world.get_resource_mut::<ReviewSession>() {
        session.busy = true;
        session.problems = problems;
        session.regions.clear();
    }
    world
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    world.insert_resource(ReviewJob {
        question: request.question,
        view: request.view,
        steps,
        pending: None,
        captures: Vec::new(),
        regions: Vec::new(),
        home,
    });
}

/// The load + script steps for one option. File problems (missing, does
/// not parse) are errors: nothing of a broken option is shown.
fn option_steps(
    option: &ReviewOption,
    base: &Path,
    empty_env: &EnvironmentRecord,
) -> Result<VecDeque<Step>, String> {
    let mut scene = match option.scene_path(base) {
        Some(path) => std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| crate::scene::from_ron(&text).map_err(|e| e.to_string()))
            .map_err(|e| format!("{}: {e}", path.display()))?,
        None => SceneRecord {
            version: crate::scene::FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            bodies: Vec::new(),
            joints: Vec::new(),
            nodes: Vec::new(),
            environment: empty_env.clone(),
        },
    };
    option.settings.apply_to(&mut scene.environment);
    let mut steps = VecDeque::from([Step::Load(Box::new(scene))]);
    for path in option.script_paths(base) {
        let source =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        steps.push_back(Step::Script(path.display().to_string(), source));
    }
    Ok(steps)
}

/// World-space bounds `(min, max)` of a scene's finite bodies, or `None`
/// when it has none. Half-planes are infinite and are left out.
pub fn scene_bounds(scene: &SceneRecord) -> Option<(Vec2, Vec2)> {
    let mut bounds: Option<(Vec2, Vec2)> = None;
    for body in &scene.bodies {
        if matches!(body.shape, crate::domain::shape::ShapeDef::HalfPlane) {
            continue;
        }
        let (lo, hi) = aabb(&body.shape);
        let rot = Vec2::from_angle(body.pose.rot);
        for corner in [lo, hi, Vec2::new(lo.x, hi.y), Vec2::new(hi.x, lo.y)] {
            let p = body.pose.pos + rot.rotate(corner);
            bounds = Some(match bounds {
                Some((min, max)) => (min.min(p), max.max(p)),
                None => (p, p),
            });
        }
    }
    bounds
}

/// Lays captured option scenes out left to right, `gap` metres apart, as one
/// scene: the first stays where it is, each next one is shifted in x to
/// clear the one before. Returns the scene, each option's region, and
/// warnings (half-plane grounds, which span every region).
///
/// Later parts get fresh [`StableId`]s (two options may load the same scene
/// file) with their joints and nodes re-pointed, and their group numbers
/// shifted past the earlier parts', so nothing is shared between options.
/// The environment is the first part's: side-by-side questions carry no
/// settings overrides, so the parts agree on it.
pub fn lay_out_side_by_side(
    parts: &[(String, SceneRecord)],
    gap: f32,
) -> (SceneRecord, Vec<ReviewRegion>, Vec<String>) {
    let mut merged = SceneRecord {
        version: crate::scene::FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        bodies: Vec::new(),
        joints: Vec::new(),
        nodes: Vec::new(),
        environment: parts.first().map_or_default(|(_, s)| s.environment.clone()),
    };
    let mut regions = Vec::new();
    let mut warnings = Vec::new();
    let mut right_edge: Option<f32> = None;
    let mut group_base = 0_u32;
    for (i, (label, part)) in parts.iter().enumerate() {
        if part
            .bodies
            .iter()
            .any(|b| matches!(b.shape, crate::domain::shape::ShapeDef::HalfPlane))
        {
            warnings.push(format!(
                "option `{label}` has a ground half-plane, which spans every side-by-side region"
            ));
        }
        // An empty option still gets a 2 m slot, so its label has a place.
        let (min, max) = scene_bounds(part).unwrap_or((Vec2::splat(-1.0), Vec2::splat(1.0)));
        let dx = right_edge.map_or(0.0, |edge| edge + gap - min.x);
        right_edge = Some(max.x + dx);
        let shift = Vec2::new(dx, 0.0);
        regions.push(ReviewRegion {
            label: label.clone(),
            min: min + shift,
            max: max + shift,
        });

        let remap: HashMap<StableId, StableId> = part
            .bodies
            .iter()
            .map(|b| b.id)
            .chain(part.joints.iter().map(|j| j.id))
            .chain(part.nodes.iter().map(|n| n.id))
            .map(|id| (id, if i == 0 { id } else { StableId::new() }))
            .collect();
        let id = |old: StableId| remap.get(&old).copied().unwrap_or(old);
        let next_base = group_base
            + part
                .bodies
                .iter()
                .flat_map(|b| b.groups.iter().copied())
                .max()
                .map_or(0, |g| g + 1);
        for body in &part.bodies {
            let mut body = body.clone();
            body.id = id(body.id);
            body.pose.pos += shift;
            for g in &mut body.groups {
                *g += group_base;
            }
            merged.bodies.push(body);
        }
        for joint in &part.joints {
            let mut joint = joint.clone();
            joint.id = id(joint.id);
            joint.def.body_a = id(joint.def.body_a);
            match joint.def.body_b {
                Some(b) => joint.def.body_b = Some(id(b)),
                // A world pin's anchor is a world point.
                None => joint.def.anchor_b.0 += shift,
            }
            merged.joints.push(joint);
        }
        for node in &part.nodes {
            let mut node = node.clone();
            node.id = id(node.id);
            node.pose.pos += shift;
            node.attachment.target = node.attachment.target.map(id);
            merged.nodes.push(node);
        }
        group_base = next_base;
    }
    (merged, regions, warnings)
}

fn union(regions: &[ReviewRegion]) -> Option<(Vec2, Vec2)> {
    regions.iter().fold(None, |acc, r| {
        Some(match acc {
            Some((min, max)) => (r.min.min(min), r.max.max(max)),
            None => (r.min, r.max),
        })
    })
}

/// World-per-pixel that fits `size` (metres) into `viewport` (logical
/// pixels) with a margin, within the camera's zoom limits.
pub fn fit_scale(size: Vec2, viewport: Vec2) -> f32 {
    let need = size * FRAME_MARGIN / viewport.max(Vec2::ONE);
    need.x.max(need.y).clamp(0.0005, 0.2)
}

/// Points the editor camera straight on at `bounds` (side by side), or —
/// when `home` — back at the start-up view. View state only. `inset` is the
/// width (logical pixels) of the viewport's right side that the review
/// window covers: the bounds are fitted into what is left of it.
fn frame_camera(world: &mut World, bounds: Option<(Vec2, Vec2)>, home: bool, inset: f32) {
    let (focus, scale) = match bounds {
        Some((min, max)) => {
            let viewport = {
                let mut cameras = world.query_filtered::<&Camera, With<Camera3d>>();
                cameras
                    .iter(world)
                    .next()
                    .and_then(Camera::logical_viewport_size)
                    .unwrap_or(Vec2::new(1280.0, 720.0))
            };
            let inset = inset.clamp(0.0, viewport.x * 0.6);
            let visible = Vec2::new(viewport.x - inset, viewport.y);
            // Labels sit above each region: leave them room.
            let size = (max - min) + Vec2::new(0.0, 1.0);
            let scale = fit_scale(size, visible);
            // Centre the content in the uncovered part: move the focus right
            // by half the covered width, in world units.
            let focus = (min + max) / 2.0 + Vec2::new(inset * scale / 2.0, 0.5);
            (focus, scale)
        }
        None if home => {
            // The start-up zoom, with the world origin centred in the part
            // of the view the review window leaves uncovered.
            let scale = 1.0 / crate::units::world::PIXELS_PER_METER;
            (Vec2::new(inset.max(0.0) * scale / 2.0, 0.0), scale)
        }
        None => return,
    };
    if let Some(mut rig) = world.get_resource_mut::<CameraRig>() {
        rig.focus = focus.extend(0.0);
        rig.yaw = 0.0;
        rig.pitch = 0.0;
        rig.roll = 0.0;
        rig.target = None;
    }
    let mut cameras = world.query_filtered::<(&mut Projection, &Camera), With<Camera3d>>();
    for (mut projection, camera) in cameras.iter_mut(world) {
        match &mut *projection {
            Projection::Orthographic(o) => o.scale = scale,
            Projection::Perspective(p) => {
                let height = camera.logical_viewport_size().map_or(720.0, |s| s.y);
                let distance = scale * height / (2.0 * (p.fov * 0.5).tan());
                if let Some(mut rig) = world.get_resource_mut::<CameraRig>() {
                    rig.distance = distance.max(0.01);
                }
                break;
            }
            Projection::Custom(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::units::PosRot;
    use crate::domain::shape::ShapeDef;
    use crate::scene::BodyRecord;

    fn body(x: f32, w: f32) -> BodyRecord {
        BodyRecord {
            id: StableId::new(),
            pose: PosRot {
                pos: Vec2::new(x, 0.0),
                rot: 0.0,
            },
            shape: ShapeDef::Box {
                width: w,
                height: 1.0,
            },
            physics: crate::domain::props::BodyPhysics::default(),
            appearance: crate::domain::appearance::Appearance::default(),
            depth: crate::domain::depth::DepthBand::default(),
            layers: None,
            groups: vec![0],
            field: None,
            tracer: None,
            fluid: None,
            motion: None,
            optics: None,
            lifetime: None,
        }
    }

    fn scene(bodies: Vec<BodyRecord>) -> SceneRecord {
        SceneRecord {
            version: crate::scene::FORMAT_VERSION,
            app_version: String::new(),
            bodies,
            joints: Vec::new(),
            nodes: Vec::new(),
            environment: EnvironmentRecord::default(),
        }
    }

    #[test]
    fn parts_are_laid_out_left_to_right_without_overlap() {
        let shared = body(0.0, 4.0);
        let parts = vec![
            ("A".to_owned(), scene(vec![shared.clone(), body(3.0, 1.0)])),
            // The same body again: must get a fresh id and its own group.
            ("B".to_owned(), scene(vec![shared])),
            ("C".to_owned(), scene(Vec::new())),
        ];
        let (merged, regions, warnings) = lay_out_side_by_side(&parts, 2.0);
        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(merged.bodies.len(), 3);
        assert_eq!(regions.len(), 3);
        // A spans [-2, 3.5]; B (4 m wide) starts 2 m after; C after B.
        assert!((regions[0].min.x + 2.0).abs() < 1e-5);
        assert!((regions[1].min.x - 5.5).abs() < 1e-5, "{regions:?}");
        assert!((regions[2].min.x - (regions[1].max.x + 2.0)).abs() < 1e-5);
        let ids: std::collections::HashSet<_> = merged.bodies.iter().map(|b| b.id).collect();
        assert_eq!(ids.len(), 3, "ids collide");
        assert_eq!(
            merged.bodies[2].groups,
            vec![1],
            "groups shared across options"
        );
        let b = scene_bounds(&scene(vec![merged.bodies[2].clone()])).unwrap();
        assert!(b.0.x >= regions[1].min.x - 1e-5 && b.1.x <= regions[1].max.x + 1e-5);
    }

    #[test]
    fn a_half_plane_is_left_out_of_bounds_and_warned_about() {
        let mut ground = body(0.0, 1.0);
        ground.shape = ShapeDef::HalfPlane;
        let parts = vec![
            ("A".to_owned(), scene(vec![ground, body(0.0, 2.0)])),
            ("B".to_owned(), scene(vec![body(0.0, 2.0)])),
        ];
        let (_, regions, warnings) = lay_out_side_by_side(&parts, 1.0);
        assert_eq!(warnings.len(), 1);
        assert!((regions[0].max.x - 1.0).abs() < 1e-5);
    }

    #[test]
    fn fit_scale_fits_the_tighter_axis() {
        let s = fit_scale(Vec2::new(20.0, 2.0), Vec2::new(1000.0, 500.0));
        assert!((s - 20.0 * FRAME_MARGIN / 1000.0).abs() < 1e-6);
        assert!((fit_scale(Vec2::splat(1e6), Vec2::ONE) - 0.2).abs() < 1e-6);
    }
}
