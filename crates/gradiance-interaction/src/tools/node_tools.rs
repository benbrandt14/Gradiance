//! The placeable behavior-node tools: the **tracer**, the **thruster**
//! (brief N-3) and the **laser** (brief N-8).
//!
//! Tracer: one-click placement — click a body to attach (the trail rides
//! it), click empty space for a free node. Thruster: press on a body where
//! it pushes and release — a click pushes up, a drag aims the push along the
//! drag. Laser: press where it sits, drag to aim, release — over another
//! body, that body becomes its target. Sensors and other actuators are **not** tools — they are ports on
//! objects (read via `SignalSource`, written via `SignalSink`), wired in the
//! node editor. All three tools place a [`NodeKind`] entity through the shared
//! [`ToolCommit::SpawnNode`] seam: one gesture, one command.

use crate::selection::Selection;
use crate::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::laser::Laser;
use gradiance_domain::node::{NodeAttachment, NodeKind};
use gradiance_domain::theme::Role;
use gradiance_domain::thruster::Thruster;
use gradiance_domain::tracer::Tracer;
use gradiance_scene::NodeRecord;

/// A thruster drag shorter than this (screen pixels) is a click: the
/// thruster pushes up.
const AIM_DEADZONE_PX: f32 = 8.0;

/// Places a node of `kind` on release: attached to the body under the
/// cursor (offset in its frame) or free. One click commits one `SpawnNode`.
/// `armed` is set by a press on the canvas and spent by the release, so a
/// release that ends a click on the UI (a menu, a panel, the palette) places
/// nothing: the driver reports every release, wherever it lands.
fn place_node(
    ctx: &ManipContext,
    world: &ToolWorld,
    armed: &mut bool,
    kind: NodeKind,
) -> ManipOutput {
    match ctx.left {
        GesturePhase::Pressed => {
            *armed = true;
            return ManipOutput::default();
        }
        GesturePhase::Released if std::mem::take(armed) => {}
        _ => return ManipOutput::default(),
    }
    let Some(p) = ctx.cursor else {
        return ManipOutput::default();
    };
    let record = node_record(world, p, |_| kind);
    ManipOutput::commit(ToolCommit::SpawnNode(Box::new(record)))
}

/// The record of a node placed at `p`: attached to the body under `p`
/// (offset in its frame) or free. `kind` gets the attached body's rotation
/// (`0` for a free node), so a payload can store a body-frame angle.
fn node_record(world: &ToolWorld, p: Vec2, kind: impl FnOnce(f32) -> NodeKind) -> NodeRecord {
    let (attachment, body_rot) = match world
        .topmost_body_at(p)
        .and_then(|e| Some((world.id_of(e)?, world.pose_of(e)?)))
    {
        Some((id, pose)) => {
            let (id, pose): (StableId, PosRot) = (id, pose);
            (
                NodeAttachment::to(id, Vec2::from_angle(-pose.rot).rotate(p - pose.pos)),
                pose.rot,
            )
        }
        None => (NodeAttachment::free(), 0.0),
    };
    let id = StableId::new();
    NodeRecord {
        id,
        pose: PosRot { pos: p, rot: 0.0 },
        attachment,
        kind: kind(body_rot),
        appearance: gradiance_domain::appearance::Appearance::for_id(id),
    }
}

/// The world angle a thruster gesture aims at: along the drag from the
/// press point `anchor` to `release`, or straight up for a drag shorter
/// than `deadzone` (world units). Pure.
#[must_use]
pub fn aim_angle(anchor: Vec2, release: Vec2, deadzone: f32) -> f32 {
    let drag = release - anchor;
    if drag.length() <= deadzone {
        core::f32::consts::FRAC_PI_2
    } else {
        drag.to_angle()
    }
}

/// Ghost ring at the cursor — shared node-placement preview.
fn ghost(ctx: &ManipContext, out: &mut ToolPreview) {
    if let Some(p) = ctx.cursor {
        // Screen-sized ghost dot: 6 px, scaled to world by `cam_scale`.
        out.circle(p, 6.0 * ctx.cam_scale, Role::Preview);
    }
}

macro_rules! node_tool {
    ($ty:ident, $doc:literal, $make:expr) => {
        #[doc = $doc]
        #[derive(Resource, Default, Debug)]
        pub struct $ty {
            /// A canvas press is waiting for its release.
            armed: bool,
        }

        impl ManipTool for $ty {
            fn update(
                &mut self,
                ctx: &ManipContext,
                world: &ToolWorld,
                _: &Selection,
            ) -> ManipOutput {
                let make: fn() -> NodeKind = $make;
                place_node(ctx, world, &mut self.armed, make())
            }
            fn drafting(&self) -> bool {
                false
            }
            fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
                ghost(ctx, out);
            }
        }
    };
}

node_tool!(
    TracerTool,
    "Places tracer nodes (trajectory trails).",
    || { NodeKind::Tracer(Tracer::default()) }
);

/// Places thruster nodes (brief N-3): press where it pushes, release to
/// place. A click pushes up; a drag aims the push along the drag. The axis is
/// stored in the body's frame, so it turns with the body. One gesture commits
/// one `SpawnNode`.
#[derive(Resource, Default, Debug)]
pub struct ThrusterTool {
    /// Where the canvas press landed (the thruster's point), until release.
    anchor: Option<Vec2>,
}

impl ManipTool for ThrusterTool {
    fn update(&mut self, ctx: &ManipContext, world: &ToolWorld, _: &Selection) -> ManipOutput {
        match ctx.left {
            GesturePhase::Pressed => {
                self.anchor = ctx.cursor;
                ManipOutput::default()
            }
            GesturePhase::Released => {
                let (Some(anchor), Some(release)) = (self.anchor.take(), ctx.cursor) else {
                    return ManipOutput::default();
                };
                let aim = aim_angle(anchor, release, AIM_DEADZONE_PX * ctx.cam_scale);
                let record = node_record(world, anchor, |body_rot| {
                    NodeKind::Thruster(Thruster {
                        angle: gradiance_units::Angle(aim - body_rot),
                        ..Thruster::default()
                    })
                });
                ManipOutput::commit(ToolCommit::SpawnNode(Box::new(record)))
            }
            _ => ManipOutput::default(),
        }
    }

    fn drafting(&self) -> bool {
        self.anchor.is_some()
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        let Some(anchor) = self.anchor else {
            ghost(ctx, out);
            return;
        };
        let r = 6.0 * ctx.cam_scale;
        out.circle(anchor, r, Role::Preview);
        let Some(cursor) = ctx.cursor else {
            return;
        };
        // The push direction as an arrow from the anchor.
        let aim = Vec2::from_angle(aim_angle(anchor, cursor, AIM_DEADZONE_PX * ctx.cam_scale));
        let tip = anchor + aim * 40.0 * ctx.cam_scale;
        out.line(anchor, tip, Role::Preview);
        let barb = 8.0 * ctx.cam_scale;
        out.line(
            tip,
            tip - aim.rotate(Vec2::from_angle(0.5)) * barb,
            Role::Preview,
        );
        out.line(
            tip,
            tip - aim.rotate(Vec2::from_angle(-0.5)) * barb,
            Role::Preview,
        );
    }
}

/// The world angle a laser gesture aims at: along the drag from `anchor` to
/// `release`, or along +x for a drag shorter than `deadzone` (world units).
/// Pure.
#[must_use]
pub fn laser_aim(anchor: Vec2, release: Vec2, deadzone: f32) -> f32 {
    let drag = release - anchor;
    if drag.length() <= deadzone {
        0.0
    } else {
        drag.to_angle()
    }
}

/// Places laser nodes (brief N-8): press where the laser sits (on a body it
/// rides, or free), drag to aim, release. Releasing over another body makes
/// that body the laser's target; a click aims along +x with no target. The
/// axis is stored in the body's frame, so it turns with the body. One
/// gesture commits one `SpawnNode`.
#[derive(Resource, Default, Debug)]
pub struct LaserTool {
    /// Where the canvas press landed (the laser's point), until release.
    anchor: Option<Vec2>,
}

impl ManipTool for LaserTool {
    fn update(&mut self, ctx: &ManipContext, world: &ToolWorld, _: &Selection) -> ManipOutput {
        match ctx.left {
            GesturePhase::Pressed => {
                self.anchor = ctx.cursor;
                ManipOutput::default()
            }
            GesturePhase::Released => {
                let (Some(anchor), Some(release)) = (self.anchor.take(), ctx.cursor) else {
                    return ManipOutput::default();
                };
                let deadzone = AIM_DEADZONE_PX * ctx.cam_scale;
                let aim = laser_aim(anchor, release, deadzone);
                let rides = world.topmost_body_at(anchor).and_then(|e| world.id_of(e));
                let target = ((release - anchor).length() > deadzone)
                    .then(|| world.topmost_body_at(release))
                    .flatten()
                    .and_then(|e| world.id_of(e))
                    .filter(|id| Some(*id) != rides);
                let record = node_record(world, anchor, |body_rot| {
                    NodeKind::Laser(Laser {
                        angle: gradiance_units::Angle(aim - body_rot),
                        target,
                        ..Laser::default()
                    })
                });
                ManipOutput::commit(ToolCommit::SpawnNode(Box::new(record)))
            }
            _ => ManipOutput::default(),
        }
    }

    fn drafting(&self) -> bool {
        self.anchor.is_some()
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        let Some(anchor) = self.anchor else {
            ghost(ctx, out);
            return;
        };
        out.circle(anchor, 6.0 * ctx.cam_scale, Role::Preview);
        let Some(cursor) = ctx.cursor else {
            return;
        };
        // The aim as a line to the cursor (or a stub along +x for a click).
        let aim = Vec2::from_angle(laser_aim(anchor, cursor, AIM_DEADZONE_PX * ctx.cam_scale));
        let reach = (cursor - anchor).length().max(40.0 * ctx.cam_scale);
        out.line(anchor, anchor + aim * reach, Role::Preview);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_drag_aims_up_and_a_long_one_along_the_drag() {
        let up = aim_angle(Vec2::ZERO, Vec2::new(0.01, 0.0), 0.1);
        assert!((up - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
        let left = aim_angle(Vec2::ZERO, Vec2::new(-1.0, 0.0), 0.1);
        assert!((left.abs() - core::f32::consts::PI).abs() < 1e-6);
    }

    #[test]
    fn a_laser_click_aims_along_x_and_a_drag_along_the_drag() {
        assert!(laser_aim(Vec2::ZERO, Vec2::new(0.0, 0.01), 0.1).abs() < 1e-6);
        let down = laser_aim(Vec2::ZERO, Vec2::new(0.0, -2.0), 0.1);
        assert!((down + core::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }
}
