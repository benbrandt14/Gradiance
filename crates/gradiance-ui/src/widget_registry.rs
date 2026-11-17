//! The widget registry's Bevy adapter: names a widget and records where it
//! was drawn this frame, so `ui.inspect` (and later `ui.click` by name, U-4)
//! finds widgets by name, not by pixel.
//!
//! The registry itself is `SolidCraft`'s, ported in upstream shape
//! ([`crate::vendor::solidcraft::ui_registry`]); this module re-exports its
//! publishing API and adds the two things Bevy needs:
//!
//! - **One thread per frame.** Upstream keeps the frame's widgets in
//!   thread-locals (an eframe app draws on one thread). A Bevy schedule may
//!   run each system on another thread, so the egui pass runs on the
//!   single-threaded executor (`GradianceUiPlugin`): [`begin_frame`], every
//!   panel that publishes, and [`end_frame`] then share one thread.
//! - **A mirror.** [`end_frame`] copies the frame's names into the
//!   [`WidgetSnapshot`] resource, which any system or the operation context
//!   reads without touching a thread-local.
//!
//! Names: an area's own rect is published under its name (`"tools"`,
//! `"right_dock"`); a widget inside it as `"<area>:<widget>"`
//! (`"tools:Box"`), so the area is the text before the first `:`
//! ([`Widget::area_of`]). Coordinates are egui logical points, origin at the
//! window's top left: the pointer's own space.

use bevy::prelude::*;
use bevy_egui::egui;

pub use crate::vendor::solidcraft::ui_registry::{
    clear_handles, handle_at, handles, publish_handle, publish_rect, publish_target, rect_of, rects,
};

/// One named widget as last drawn: its rect, its handle (the point to click
/// or drag), or both (a widget published with [`publish_target`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    /// The published name: `"tools:Box"`.
    pub name: String,
    /// The area it belongs to: the name up to the first `:`.
    pub area: String,
    /// `[x0, y0, x1, y1]` in logical points, if a rect was published.
    pub rect: Option<[f32; 4]>,
    /// `[x, y]` in logical points, if a handle was published.
    pub at: Option<[f32; 2]>,
}

impl Widget {
    /// The area of a published name: the text before the first `:`, or the
    /// whole name (an area's own rect).
    pub fn area_of(name: &str) -> &str {
        name.split_once(':').map_or(name, |(area, _)| area)
    }

    fn named(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            area: Self::area_of(name).to_owned(),
            rect: None,
            at: None,
        }
    }
}

/// The widgets drawn in the last finished egui pass, one row per name, in
/// first-published order (a name published twice keeps its latest geometry,
/// as upstream's lookups do). Empty without a window.
#[derive(Resource, Debug, Clone, Default)]
pub struct WidgetSnapshot(pub Vec<Widget>);

impl WidgetSnapshot {
    /// Collects the rects and handles published on this thread since the
    /// last [`clear_handles`].
    pub fn collect() -> Self {
        let mut rows: Vec<Widget> = Vec::new();
        let mut row = |name: &str| -> usize {
            if let Some(i) = rows.iter().position(|w| w.name == name) {
                i
            } else {
                rows.push(Widget::named(name));
                rows.len() - 1
            }
        };
        let mut placed: Vec<(usize, Option<egui::Rect>, Option<egui::Pos2>)> = Vec::new();
        for (name, rect) in rects("") {
            placed.push((row(&name), Some(rect), None));
        }
        for (name, at) in handles("") {
            placed.push((row(&name), None, Some(at)));
        }
        for (i, rect, at) in placed {
            if let Some(r) = rect {
                rows[i].rect = Some([r.min.x, r.min.y, r.max.x, r.max.y]);
            }
            if let Some(p) = at {
                rows[i].at = Some([p.x, p.y]);
            }
        }
        Self(rows)
    }

    /// The widgets whose name starts with `prefix` (`""`: all of them).
    pub fn matching<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a Widget> + 'a {
        self.0.iter().filter(move |w| w.name.starts_with(prefix))
    }
}

/// First in the egui pass: forgets the previous frame's widgets.
pub fn begin_frame() {
    clear_handles();
}

/// Last in the egui pass: publishes the frame's widgets as the
/// [`WidgetSnapshot`] mirror.
pub fn end_frame(mut snapshot: ResMut<WidgetSnapshot>) {
    *snapshot = WidgetSnapshot::collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f32) -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(x, 0.0), egui::pos2(x + 10.0, 10.0))
    }

    #[test]
    fn a_target_is_one_row_with_its_rect_and_its_centre() {
        clear_handles();
        publish_rect("tools", square(0.0));
        publish_target("tools:Box", square(20.0));
        publish_handle("view_cube:x", egui::pos2(3.0, 4.0));
        let snapshot = WidgetSnapshot::collect();
        let names: Vec<&str> = snapshot.0.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["tools", "tools:Box", "view_cube:x"]);
        let target = &snapshot.0[1];
        assert_eq!(target.area, "tools");
        assert_eq!(target.rect, Some([20.0, 0.0, 30.0, 10.0]));
        assert_eq!(target.at, Some([25.0, 5.0]));
        assert_eq!(snapshot.0[2].rect, None);
        assert_eq!(snapshot.matching("tools").count(), 2);
        assert_eq!(snapshot.matching("tools:").count(), 1);
    }

    #[test]
    fn a_name_published_twice_keeps_its_latest_geometry() {
        clear_handles();
        publish_rect("menu", square(0.0));
        publish_rect("menu", square(5.0));
        let snapshot = WidgetSnapshot::collect();
        assert_eq!(snapshot.0.len(), 1);
        assert_eq!(snapshot.0[0].rect, Some([5.0, 0.0, 15.0, 10.0]));
    }

    #[test]
    fn the_area_is_the_name_up_to_the_first_colon() {
        assert_eq!(Widget::area_of("tools:Box"), "tools");
        assert_eq!(Widget::area_of("menu"), "menu");
        assert_eq!(Widget::area_of("a:b:c"), "a");
    }
}
