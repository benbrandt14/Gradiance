//! The Depth section of the right dock: an editor for authored
//! [`DepthBand`]s (hosted by `ui::dock` alongside the script console).
//!
//! Selected bodies appear as colored bars (their fill color) on a
//! vertical depth axis — front (the interaction plane) at the top,
//! deeper layers below, layer gridlines in the shared layer hues. Drag a
//! bar's end to resize its band, drag its middle to move it; edges snap
//! to the quarter-layer grid and the view auto-grows when dragged past
//! the bottom. Dragging previews locally (panel-scratch state, invariant
//! 2) and commits exactly **one** `PropertyEditIntent` on release.

use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_command::intent::PropertyEditIntent;
use gradiance_command::property::{PropertyChange, PropertyValue};
use gradiance_core::constants::LAYER_HEIGHT;
use gradiance_core::ids::StableId;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::layers::layer_hue;

/// Pixels of grab slop around a bar edge before it counts as the middle.
const EDGE_GRAB_PX: f32 = 6.0;
/// Default visible depth: 8 layers.
const DEFAULT_VIEW_DEPTH: f32 = 8.0 * LAYER_HEIGHT;

/// Which part of a band bar a drag grabbed.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Grab {
    /// The front (top) edge — resizes `near`.
    Near,
    /// The back (bottom) edge — resizes `far`.
    Far,
    /// The middle — moves the whole band.
    Whole,
}

/// One in-flight bar drag (panel-local preview; authored state is only
/// written by the commit intent on release).
#[derive(Debug, Clone, Copy)]
struct BandDrag {
    id: StableId,
    grab: Grab,
    start: DepthBand,
    current: DepthBand,
    /// Pointer depth minus `near` at grab time (move mode keeps it).
    offset: f32,
}

/// Dock state (`EditorState`: never persisted).
#[derive(Resource, Debug)]
pub struct DepthPanel {
    /// Whether the dock is showing.
    pub open: bool,
    /// Deepest depth the axis shows (auto-grows past content and drags).
    pub view_depth: f32,
    drag: Option<BandDrag>,
}

impl Default for DepthPanel {
    fn default() -> Self {
        Self {
            open: false,
            view_depth: DEFAULT_VIEW_DEPTH,
            drag: None,
        }
    }
}

/// The band a drag produces for a pointer at `depth`, snapped and sane.
fn dragged_band(drag: &BandDrag, depth: f32) -> DepthBand {
    let band = match drag.grab {
        Grab::Near => DepthBand {
            near: depth.min(drag.current.far - DepthBand::MIN_THICKNESS),
            far: drag.current.far,
        },
        Grab::Far => DepthBand {
            near: drag.current.near,
            far: depth.max(drag.current.near + DepthBand::MIN_THICKNESS),
        },
        Grab::Whole => {
            let near = (depth - drag.offset).max(0.0);
            DepthBand {
                near,
                far: near + drag.start.thickness(),
            }
        }
    };
    band.snapped()
}

/// Renders the depth section and turns completed bar drags into property
/// intents. `rows` is the selection projected to (id, band, fill color);
/// `max_height` bounds the axis so the dock can share space with the
/// console below.
#[expect(clippy::too_many_lines)] // one panel, drawn top to bottom
pub fn depth_section(
    ui: &mut egui::Ui,
    panel: &mut DepthPanel,
    rows: &[(StableId, DepthBand, egui::Color32)],
    edits: &mut MessageWriter<PropertyEditIntent>,
    max_height: f32,
) {
    // The axis grows to fit the content (and stays grown while dragging).
    let content_deepest = rows
        .iter()
        .map(|(_, b, _)| b.far)
        .chain(panel.drag.iter().map(|d| d.current.far))
        .fold(DEFAULT_VIEW_DEPTH, f32::max);
    panel.view_depth = panel
        .view_depth
        .max((content_deepest / LAYER_HEIGHT).ceil() * LAYER_HEIGHT);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Depth").strong())
            .on_hover_text("front ↑ · back ↓ — a body collides with what its band overlaps");
        if ui.button("✕").clicked() {
            panel.open = false;
            panel.drag = None;
        }
    });
    if rows.is_empty() {
        ui.weak("select bodies to edit their depth bands");
        return;
    }

    let height = ui.available_height().min(max_height).max(120.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    let view = panel.view_depth.max(LAYER_HEIGHT);
    let scale = rect.height() / view;
    let y_of = |depth: f32| rect.top() + depth * scale;
    let depth_of = |y: f32| (y - rect.top()) / scale;

    // Layer gridlines + indices in the shared layer hues.
    let mut layer = 0u32;
    loop {
        let d = layer as f32 * LAYER_HEIGHT;
        if d > view {
            break;
        }
        let y = y_of(d);
        let hue = layer_hue(layer.min(31));
        let c = gradiance_domain::appearance::Rgba::from_hsl(hue, 0.6, 0.5);
        let color = egui::Color32::from_rgba_unmultiplied(
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            90,
        );
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, color),
        );
        if layer < 32 {
            painter.text(
                egui::pos2(rect.left() + 2.0, y + 1.0),
                egui::Align2::LEFT_TOP,
                format!("{layer}"),
                egui::FontId::proportional(9.0),
                color,
            );
        }
        layer += 1;
    }

    // Band bars, one column per selected body.
    let cols = rows.len() as f32;
    let left_gutter = 16.0;
    let col_w = ((rect.width() - left_gutter) / cols).clamp(10.0, 48.0);
    let pointer = response.hover_pos().or(response.interact_pointer_pos());
    for (i, (id, band, color)) in rows.iter().enumerate() {
        // The dragged bar previews its in-flight band.
        let shown = match panel.drag {
            Some(d) if d.id == *id => d.current,
            _ => *band,
        };
        let x0 = rect.left() + left_gutter + i as f32 * col_w + 2.0;
        let bar = egui::Rect::from_min_max(
            egui::pos2(x0, y_of(shown.near)),
            egui::pos2(x0 + col_w - 4.0, y_of(shown.far)),
        );
        let hovered = pointer.is_some_and(|p| bar.expand(EDGE_GRAB_PX).contains(p));
        painter.rect_filled(
            bar,
            3.0,
            color.gamma_multiply(if hovered { 1.0 } else { 0.85 }),
        );
        painter.rect_stroke(
            bar,
            3.0,
            egui::Stroke::new(
                if hovered { 2.0 } else { 1.0 },
                egui::Color32::from_gray(if hovered { 230 } else { 140 }),
            ),
            egui::StrokeKind::Outside,
        );

        // Drag start: classify the grab (edge wins near the edge).
        if response.drag_started()
            && panel.drag.is_none()
            && let Some(p) = response.interact_pointer_pos()
            && bar.expand(EDGE_GRAB_PX).contains(p)
        {
            let grab = if (p.y - bar.top()).abs() <= EDGE_GRAB_PX {
                Grab::Near
            } else if (p.y - bar.bottom()).abs() <= EDGE_GRAB_PX {
                Grab::Far
            } else {
                Grab::Whole
            };
            panel.drag = Some(BandDrag {
                id: *id,
                grab,
                start: *band,
                current: *band,
                offset: depth_of(p.y) - band.near,
            });
        }
    }

    // Drag update / commit.
    if let Some(mut drag) = panel.drag {
        if let Some(p) = response.interact_pointer_pos() {
            drag.current = dragged_band(&drag, depth_of(p.y));
            // Auto-grow while dragging past the bottom edge.
            if p.y > rect.bottom() - 4.0 {
                panel.view_depth += LAYER_HEIGHT;
            }
        }
        if response.drag_stopped() {
            if drag.current != drag.start {
                edits.write(PropertyEditIntent {
                    changes: vec![PropertyChange {
                        id: drag.id,
                        old: PropertyValue::Depth(drag.start),
                        new: PropertyValue::Depth(drag.current),
                    }],
                });
            }
            panel.drag = None;
        } else {
            panel.drag = Some(drag);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_drags_resize_and_snap() {
        let drag = BandDrag {
            id: StableId::new(),
            grab: Grab::Far,
            start: DepthBand::default(),
            current: DepthBand::default(),
            offset: 0.0,
        };
        let band = dragged_band(&drag, 0.234);
        assert!((band.far - 0.225).abs() < 1e-4, "snaps to quarter layers");
        assert!((band.near - 0.0).abs() < 1e-6, "near end untouched");
    }

    #[test]
    fn near_drag_cannot_cross_the_far_edge() {
        let drag = BandDrag {
            id: StableId::new(),
            grab: Grab::Near,
            start: DepthBand {
                near: 0.0,
                far: 0.1,
            },
            current: DepthBand {
                near: 0.0,
                far: 0.1,
            },
            offset: 0.0,
        };
        let band = dragged_band(&drag, 0.5);
        assert!(band.near < band.far);
        assert!(band.thickness() >= DepthBand::MIN_THICKNESS - 1e-4);
    }

    #[test]
    fn whole_drag_preserves_thickness_and_clamps_front() {
        let start = DepthBand {
            near: 0.1,
            far: 0.25,
        };
        let drag = BandDrag {
            id: StableId::new(),
            grab: Grab::Whole,
            start,
            current: start,
            offset: 0.05, // grabbed 0.05 m below the front face
        };
        let moved = dragged_band(&drag, 0.425);
        assert!((moved.thickness() - start.thickness()).abs() < 1e-4);
        let clamped = dragged_band(&drag, -1.0);
        assert!(
            (clamped.near - 0.0).abs() < 1e-6,
            "cannot go in front of the plane"
        );
    }
}
