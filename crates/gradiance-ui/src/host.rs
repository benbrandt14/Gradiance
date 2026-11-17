//! The host adapter (`docs/unified-surface-plan.md` § 6): the one place the
//! window's own requests meet Bevy. Today it holds the screenshot request
//! (`ui.screenshot`); input injection (U-4) joins it.
//!
//! A screenshot is Bevy's ([`Screenshot::primary_window`] with
//! [`save_to_disk`]): the whole window, scene and egui chrome together. When
//! it is taken follows `SolidCraft`'s settle rule (`control-egui/src/
//! screenshot.rs`, `Shots::issue`; ported as a rule, not as a file, because
//! upstream asks egui for the frame and Bevy takes it from the swap chain):
//! a request waits at least [`SETTLE_MS`] and [`SETTLE_FRAMES`] frames, so a
//! panel the same script just opened is drawn; and while the view is still
//! moving (a camera glide) it waits up to [`MOVING_MS`] more, then takes the
//! view as it is.
//!
//! One rule is Bevy's own: the renderer starts after the app does and
//! compiles pipelines in the background, so the first frames are black and
//! a new material draws late. While [`RenderBusy`] says so, a request waits
//! (its frame count starts again), up to [`RENDER_WAIT_MS`]. Without it, a
//! start-up script's screenshot was black (measured on lavapipe,
//! 2026-11-17).

use bevy::prelude::*;
use bevy::render::render_resource::PipelineCache;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::window::PrimaryWindow;
use gradiance_interaction::camera::CameraRig;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The least time (ms) between a request and its frame.
pub const SETTLE_MS: f64 = 150.0;
/// The least number of frames drawn between a request and its frame.
pub const SETTLE_FRAMES: u32 = 3;
/// How much longer (ms) a request waits for a moving view to settle.
pub const MOVING_MS: f64 = 500.0;
/// How long (ms) a request waits for a busy renderer before it takes the
/// window as it is.
pub const RENDER_WAIT_MS: f64 = 20_000.0;

/// Whether the renderer has not drawn yet or is still compiling pipelines.
/// The render world writes it each frame; the main world reads it. Starts
/// busy: until the render schedule first runs, nothing has been drawn.
#[derive(Resource, Debug, Clone)]
pub struct RenderBusy(Arc<AtomicBool>);

impl Default for RenderBusy {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
}

impl RenderBusy {
    /// Whether the window may not yet show what the app holds.
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Render world, each frame: busy while any pipeline waits to compile.
fn publish_render_busy(cache: Res<PipelineCache>, busy: Res<RenderBusy>) {
    busy.0.store(
        cache.waiting_pipelines().next().is_some(),
        Ordering::Relaxed,
    );
}

/// Shares one [`RenderBusy`] between the main and the render world. Needs
/// the render plugin.
pub(crate) fn install_render_busy(app: &mut App) {
    let busy = RenderBusy::default();
    app.insert_resource(busy.clone());
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.insert_resource(busy);
        render.add_systems(Render, publish_render_busy.in_set(RenderSystems::Cleanup));
    }
}

/// A request for a PNG of the window at `path`, written by `ui.screenshot`.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotRequest {
    /// Where the PNG goes (a `.png` path; relative to the working directory).
    pub path: String,
}

/// One request waiting for its frame.
#[derive(Debug, Clone, PartialEq)]
struct Queued {
    path: String,
    /// The time (ms) from which the frame may be taken.
    at: f64,
    /// The frames drawn since the request.
    frames: u32,
}

/// The screenshots waiting for their frame (the settle rule, pure).
#[derive(Resource, Debug, Default)]
pub struct ShotQueue {
    queued: Vec<Queued>,
}

impl ShotQueue {
    /// Queues a request made at `now` (ms).
    pub fn request(&mut self, path: String, now: f64) {
        self.queued.push(Queued {
            path,
            at: now + SETTLE_MS,
            frames: 0,
        });
    }

    /// Called once per frame: counts the frame and returns the paths whose
    /// frame is due. `settled`: the view is not moving. `rendering`: the
    /// renderer is busy ([`RenderBusy`]).
    pub fn due(&mut self, now: f64, settled: bool, rendering: bool) -> Vec<String> {
        let mut due = Vec::new();
        self.queued.retain_mut(|q| {
            if rendering && now < q.at + RENDER_WAIT_MS {
                q.frames = 0;
                return true;
            }
            q.frames += 1;
            let ready =
                now >= q.at && q.frames >= SETTLE_FRAMES && (settled || now >= q.at + MOVING_MS);
            if ready {
                due.push(std::mem::take(&mut q.path));
            }
            !ready
        });
        due
    }

    /// Whether no request waits.
    pub fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }
}

/// Moves each [`ScreenshotRequest`] into the [`ShotQueue`]. Without a window
/// (a headless run) there is nothing to take: the request is dropped with a
/// warning.
pub fn queue_screenshots(
    mut requests: MessageReader<ScreenshotRequest>,
    mut queue: ResMut<ShotQueue>,
    windows: Query<(), With<PrimaryWindow>>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed_secs_f64() * 1000.0;
    let windowed = !windows.is_empty();
    for request in requests.read() {
        if windowed {
            queue.request(request.path.clone(), now);
        } else {
            warn!("ui.screenshot {}: no window to take", request.path);
        }
    }
}

/// Takes each screenshot whose frame is due. The view counts as moving while
/// the camera glides (a view-cube or `Home` glide).
pub fn take_screenshots(
    mut commands: Commands,
    mut queue: ResMut<ShotQueue>,
    rig: Option<Res<CameraRig>>,
    busy: Option<Res<RenderBusy>>,
    time: Res<Time<Real>>,
) {
    if queue.is_empty() {
        return;
    }
    let settled = rig.is_none_or(|rig| rig.target.is_none());
    let rendering = busy.is_some_and(|busy| busy.get());
    let now = time.elapsed_secs_f64() * 1000.0;
    for path in queue.due(now, settled, rendering) {
        info!("ui.screenshot: saving {path}");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_taken_after_the_settle_time_and_on_the_third_frame() {
        let mut queue = ShotQueue::default();
        queue.request("a.png".into(), 0.0);
        assert!(queue.due(200.0, true, false).is_empty(), "one frame drawn");
        assert!(queue.due(200.0, true, false).is_empty(), "two frames drawn");
        assert_eq!(queue.due(200.0, true, false), ["a.png"], "the third frame");
        assert!(queue.is_empty());

        queue.request("b.png".into(), 0.0);
        for _ in 0..3 {
            assert!(queue.due(149.0, true, false).is_empty(), "too soon");
        }
        assert_eq!(queue.due(150.0, true, false), ["b.png"]);
    }

    #[test]
    fn a_moving_view_is_taken_as_it_is_after_the_moving_wait() {
        let mut queue = ShotQueue::default();
        queue.request("c.png".into(), 0.0);
        for _ in 0..3 {
            assert!(queue.due(649.0, false, false).is_empty(), "still moving");
        }
        assert_eq!(queue.due(650.0, false, false), ["c.png"], "taken as it is");
    }

    #[test]
    fn requests_are_independent() {
        let mut queue = ShotQueue::default();
        queue.request("early.png".into(), 0.0);
        queue.request("late.png".into(), 100.0);
        queue.due(150.0, true, false);
        queue.due(150.0, true, false);
        assert_eq!(queue.due(150.0, true, false), ["early.png"]);
        assert_eq!(queue.due(250.0, true, false), ["late.png"]);
    }

    #[test]
    fn a_busy_renderer_holds_a_request_until_it_draws_or_the_wait_ends() {
        let mut queue = ShotQueue::default();
        queue.request("d.png".into(), 0.0);
        for _ in 0..5 {
            assert!(
                queue.due(1000.0, true, true).is_empty(),
                "nothing drawn yet"
            );
        }
        assert!(
            queue.due(1100.0, true, false).is_empty(),
            "the count starts again"
        );
        assert!(queue.due(1100.0, true, false).is_empty(), "two frames");
        assert_eq!(queue.due(1100.0, true, false), ["d.png"]);

        queue.request("e.png".into(), 0.0);
        let late = SETTLE_MS + RENDER_WAIT_MS;
        for _ in 0..2 {
            assert!(queue.due(late, true, true).is_empty(), "settling");
        }
        assert_eq!(queue.due(late, true, true), ["e.png"], "taken as it is");
    }
}
