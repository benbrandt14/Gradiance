//! Alignment snapping for the select tool's move gesture (M18, feedback
//! 3.1: "lightly snap to collinear on move … optionally to centerlines").
//!
//! The math is [`gradiance_geometry::align`]; this module is the ECS half.
//! At gesture start it measures the moving selection and its neighbours
//! once, in the oblique coordinates of the active grid's axes
//! ([`GestureConstraints::basis`](crate::gesture::GestureConstraints)). Each
//! frame [`move_delta`] turns the cursor offset into the move's delta —
//! axis lock first, then alignment — and both the preview hold and the
//! release commit call it, so what you see is what commits (invariant 2).
//!
//! Rules (recorded in brief 1-5's Outcome):
//! - Alignment runs only while `SnapConfig::objects_enabled` is on and at
//!   least one of `align_edges` / `align_centers` is set.
//! - An axis lock freezes the coordinate it zeroes; alignment only shifts
//!   the coordinate the lock leaves free, so it never reintroduces locked
//!   motion.
//! - Tolerance is half the object-snap capture radius, in world units at
//!   the current zoom ("light").
//! - References are gathered once (bodies near the selection, excluding
//!   it and infinite ground); no alignment while the simulation plays,
//!   since the references would go stale under the drag.

use crate::tools::context::{ManipContext, ToolPreview, ToolWorld};
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_geometry::align::{
    AlignFeatures, AlignFrame, AxisMatch, Extents, align, correction, guide_segment,
};

/// How far (logical px) around the selection's bounds neighbours are
/// gathered as alignment references.
const ALIGN_REACH_PX: f32 = 400.0;

/// The frozen alignment inputs of one move gesture.
#[derive(Debug, Clone)]
pub struct MoveAlignment {
    frame: AlignFrame,
    /// The selection's extents at its original position.
    moving: Extents,
    /// Neighbours' extents (they do not move during a paused drag).
    refs: Vec<Extents>,
}

/// A move delta and the alignments holding on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlignedMove {
    /// The translation to apply (constrained, then aligned).
    pub delta: Vec2,
    /// The match per oblique coordinate, if any.
    pub matches: [Option<AxisMatch>; 2],
}

impl MoveAlignment {
    /// Measures `moving` and its neighbours for a move gesture, or `None`
    /// when there is nothing to align against (or the simulation plays).
    pub fn gather(ctx: &ManipContext, world: &ToolWorld, moving: &[Entity]) -> Option<Self> {
        if ctx.playing {
            return None;
        }
        let frame = AlignFrame::new(ctx.constraints.basis)?;
        let moving_ext = moving
            .iter()
            .filter_map(|e| world.body_extents(&frame, *e))
            .reduce(|a, b| [a[0].union(b[0]), a[1].union(b[1])])?;
        let corners = [
            Vec2::new(moving_ext[0].min, moving_ext[1].min),
            Vec2::new(moving_ext[0].max, moving_ext[1].min),
            Vec2::new(moving_ext[0].max, moving_ext[1].max),
            Vec2::new(moving_ext[0].min, moving_ext[1].max),
        ]
        .map(|c| frame.point(c));
        let reach = Vec2::splat(ALIGN_REACH_PX * ctx.cam_scale);
        let min = corners.iter().fold(Vec2::INFINITY, |a, c| a.min(*c)) - reach;
        let max = corners.iter().fold(Vec2::NEG_INFINITY, |a, c| a.max(*c)) + reach;
        let refs: Vec<Extents> = world
            .finite_bodies_touching(min, max)
            .into_iter()
            .filter(|e| !moving.contains(e))
            .filter_map(|e| world.body_extents(&frame, e))
            .collect();
        (!refs.is_empty()).then_some(Self {
            frame,
            moving: moving_ext,
            refs,
        })
    }

    /// The selection's extents after moving by `delta`.
    fn moved(&self, delta: Vec2) -> Extents {
        let c = self.frame.coords(delta);
        [self.moving[0].shifted(c.x), self.moving[1].shifted(c.y)]
    }
}

/// The move delta for a cursor offset `raw`: the axis lock applied, then
/// (when enabled and a [`MoveAlignment`] exists) the alignment correction.
/// The preview and the commit both call this.
pub fn move_delta(
    ctx: &ManipContext,
    raw: Vec2,
    align_state: Option<&MoveAlignment>,
) -> AlignedMove {
    let delta = ctx.constraints.constrain(raw);
    let unaligned = AlignedMove {
        delta,
        matches: [None, None],
    };
    let snap = ctx.snap;
    let features = AlignFeatures {
        edges: snap.align_edges,
        centers: snap.align_centers,
    };
    let Some(state) = align_state else {
        return unaligned;
    };
    if !snap.objects_enabled
        || !(features.edges || features.centers)
        || state.frame.axes() != ctx.constraints.basis
    {
        return unaligned;
    }
    let basis = ctx.constraints.basis;
    let free = match ctx.constraints.lock_axis(raw) {
        None => [true, true],
        Some(axis) if axis == basis[0] => [true, false],
        Some(axis) if axis == basis[1] => [false, true],
        Some(_) => return unaligned,
    };
    let tolerance = 0.5 * snap.max_screen_distance * ctx.cam_scale;
    let matches = align(state.moved(delta), &state.refs, tolerance, features, free);
    AlignedMove {
        delta: delta + correction(&state.frame, &matches),
        matches,
    }
}

/// Draws a faint guide along each alignment holding in `aligned`.
pub fn preview_guides(state: &MoveAlignment, aligned: &AlignedMove, out: &mut ToolPreview) {
    let moved = state.moved(aligned.delta);
    for (k, m) in aligned.matches.iter().enumerate() {
        let Some(m) = m else { continue };
        let Some(reference) = state.refs.get(m.reference) else {
            continue;
        };
        let (a, b) = guide_segment(&state.frame, k, m, &moved, reference);
        out.line(a, b, css::HOT_PINK.with_alpha(0.45));
    }
}
