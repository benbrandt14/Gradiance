//! The chain tool (brief N-5): drag a path; release lays a chain along it.
//!
//! The drag records the stroke. On release the tool commits one
//! [`ToolCommit::Chain`]: the stroke, the link length, width and pin rule
//! from [`ToolDefaults`], and the
//! bodies under the two ends (found with the rod tool's end hit-test, so a
//! snapped end on an edge still attaches). The commit invokes the
//! `body.chain` operation, which lays the links and hinges and spawns them
//! as **one** undo step; during the drag only the gizmo preview exists.
//!
//! The chain is the rod's many-link sibling: the same capsule links, the
//! same end hinges, the same command (`SpawnRodCommand`).

use crate::selection::Selection;
use crate::snap::DraftReference;
use crate::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::settings::{ChainPins, ToolDefaults};
use gradiance_domain::theme::Role;
use gradiance_geometry::chain::chain_points;
use gradiance_geometry::sketch::rdp;

/// End-attachment tolerance in screen pixels (the rod tool's).
const ATTACH_TOLERANCE_PX: f32 = 8.0;
/// The stroke records a point each time the pointer has moved this far
/// (screen pixels): smooth enough for a curve, few enough to keep the
/// operation's parameters short.
const SAMPLE_PX: f32 = 4.0;
/// The committed stroke is thinned to within this many screen pixels (the
/// sketch tool's), so a long stroke is a short `body.chain` call and a short
/// Adjust Last entry (brief Q-3).
const THIN_PX: f32 = 1.0;

/// In-progress chain gesture: the stroke so far (empty when idle).
#[derive(Resource, Default, Debug)]
pub struct ChainDraft {
    /// The recorded stroke, world space, in drawing order.
    pub path: Vec<Vec2>,
}

impl ChainDraft {
    /// Adds `p` to the stroke when it is at least `step` from the last
    /// recorded point.
    fn record(&mut self, p: Vec2, step: f32) {
        if self.path.last().is_none_or(|last| last.distance(p) >= step) {
            self.path.push(p);
        }
    }

    /// The stroke with the pointer's current point as its end.
    fn stroke_to(&self, end: Option<Vec2>) -> Vec<Vec2> {
        let mut path = self.path.clone();
        if let Some(end) = end
            && path.last() != Some(&end)
        {
            path.push(end);
        }
        path
    }
}

impl ManipTool for ChainDraft {
    fn update(
        &mut self,
        ctx: &ManipContext,
        world: &ToolWorld,
        _selection: &Selection,
    ) -> ManipOutput {
        let step = SAMPLE_PX * ctx.cam_scale;
        match ctx.left {
            GesturePhase::Pressed => {
                self.path.clear();
                if let Some(p) = ctx.cursor {
                    self.path.push(p);
                }
            }
            GesturePhase::Held => {
                if !self.path.is_empty()
                    && let Some(p) = ctx.cursor
                {
                    self.record(p, step);
                }
            }
            GesturePhase::Released if !self.path.is_empty() => {
                let path = rdp(&self.stroke_to(ctx.cursor), THIN_PX * ctx.cam_scale);
                self.path.clear();
                let tol = ATTACH_TOLERANCE_PX * ctx.cam_scale;
                if let Some(commit) = build_chain(path, &ctx.defaults, tol, world) {
                    return ManipOutput::commit(commit);
                }
            }
            GesturePhase::Released | GesturePhase::Idle => {}
        }
        ManipOutput::default()
    }

    fn drafting(&self) -> bool {
        !self.path.is_empty()
    }

    /// The stroke's last point: the chain is drawn on from it.
    fn snap_reference(&self) -> Option<DraftReference> {
        self.path.last().copied().map(DraftReference::Line)
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        let path = self.stroke_to(ctx.cursor);
        let Some(&start) = path.first() else {
            return;
        };
        out.circle(start, 4.0 * ctx.cam_scale, Role::JointOutline);
        // The links the release would lay: a line per link and a ring at
        // each hinge, the link's own width.
        match chain_points(&path, ctx.defaults.chain_link.0) {
            Some(points) => {
                let radius = (ctx.defaults.chain_width.0 / 2.0).max(ctx.cam_scale);
                out.polyline(points.clone(), Role::JointOutline);
                for p in points {
                    out.circle(p, radius, Role::JointOutline);
                }
            }
            None => out.polyline(path, Role::JointOutline),
        }
    }
}

/// The commit for a finished stroke, or `None` when it makes no chain (too
/// short). The ends' bodies are looked up only when the pins can use them.
fn build_chain(
    path: Vec<Vec2>,
    defaults: &ToolDefaults,
    tol: f32,
    world: &ToolWorld,
) -> Option<ToolCommit> {
    chain_points(&path, defaults.chain_link.0)?;
    let (&first, &last) = (path.first()?, path.last()?);
    let end_body = |p: Vec2| -> Option<StableId> {
        if defaults.chain_pins == ChainPins::Free {
            return None;
        }
        let hit = *world.attachment_bodies_at(p, tol).first()?;
        world.id_of(hit)
    };
    Some(ToolCommit::Chain {
        ends: [end_body(first), end_body(last)],
        path,
        link: defaults.chain_link.0,
        width: defaults.chain_width.0,
        pins: defaults.chain_pins,
    })
}

/// The `body.chain` invocation for a chain commit: the operation lays the
/// links, so a drawn chain and a scripted one are the same records and
/// Adjust Last Operation can change it afterwards. An end with no body is
/// `-1` (names nothing).
pub fn chain_invocation(
    path: &[Vec2],
    link: f32,
    width: f32,
    ends: [Option<StableId>; 2],
    pins: ChainPins,
) -> gradiance_op::InvokeOp {
    let flat: Vec<f32> = path.iter().flat_map(|p| [p.x, p.y]).collect();
    let end = |id: Option<StableId>| match id {
        Some(id) => gradiance_op::json!(id.0.to_string()),
        None => gradiance_op::json!(-1),
    };
    gradiance_op::InvokeOp::with(
        "body.chain",
        gradiance_op::json!({
            "path": flat,
            "link": link,
            "width": width,
            "a": end(ends[0]),
            "b": end(ends[1]),
            "pins": pins.name(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stroke_records_points_a_step_apart_and_ends_at_the_pointer() {
        let mut draft = ChainDraft::default();
        draft.record(Vec2::ZERO, 0.1);
        draft.record(Vec2::new(0.05, 0.0), 0.1);
        draft.record(Vec2::new(0.2, 0.0), 0.1);
        assert_eq!(draft.path, vec![Vec2::ZERO, Vec2::new(0.2, 0.0)]);
        let stroke = draft.stroke_to(Some(Vec2::new(0.25, 0.0)));
        assert_eq!(stroke.len(), 3);
        assert_eq!(draft.stroke_to(Some(Vec2::new(0.2, 0.0))).len(), 2);
    }

    /// Distance from `p` to the polyline `path`.
    fn distance_to(path: &[Vec2], p: Vec2) -> f32 {
        path.windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                p.distance(a + ab * t)
            })
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn a_committed_stroke_is_thinned_within_a_pixel() {
        let px = 0.01;
        // A straight 4 m drag sampled every 4 px: two points survive.
        let straight: Vec<Vec2> = (0..=100).map(|i| Vec2::new(i as f32 * 0.04, 0.0)).collect();
        assert_eq!(rdp(&straight, THIN_PX * px).len(), 2);
        // A quarter circle of 2 m: far fewer points, and every link point
        // the thinned stroke lays lies within a pixel of the raw stroke.
        let arc: Vec<Vec2> = (0..=80)
            .map(|i| {
                let a = i as f32 / 80.0 * std::f32::consts::FRAC_PI_2;
                Vec2::new(a.cos(), a.sin()) * 2.0
            })
            .collect();
        let thin = rdp(&arc, THIN_PX * px);
        assert!(
            thin.len() < arc.len() / 2,
            "{} of {}",
            thin.len(),
            arc.len()
        );
        let links = chain_points(&thin, 0.25).expect("a chain");
        for p in links {
            let d = distance_to(&arc, p);
            assert!(d <= THIN_PX * px + 1e-4, "a hinge {d} m off the stroke");
        }
    }

    #[test]
    fn the_invocation_names_the_ends_and_flattens_the_path() {
        let id = StableId::new();
        let invoke = chain_invocation(
            &[Vec2::ZERO, Vec2::new(1.0, 2.0)],
            0.25,
            0.06,
            [Some(id), None],
            ChainPins::World,
        );
        let params = &invoke.params;
        assert_eq!(params["path"], gradiance_op::json!([0.0, 0.0, 1.0, 2.0]));
        assert_eq!(params["a"], gradiance_op::json!(id.0.to_string()));
        assert_eq!(params["b"], gradiance_op::json!(-1));
        assert_eq!(params["pins"], gradiance_op::json!("world"));
    }
}
