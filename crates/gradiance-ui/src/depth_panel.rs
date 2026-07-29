//! The Depth section of the right dock: a **top-down plan view** of the scene
//! and an editor for authored [`DepthBand`]s.
//!
//! # Why a plan view
//!
//! This used to be a bar chart of the *selection* — one column per selected
//! body on a vertical depth axis. It edited bands correctly and told you
//! nothing about the scene: you could not see what a body was in front of,
//! whether two bodies overlapped in depth, or where the empty space was,
//! because everything you had not selected was invisible and the horizontal
//! axis was column index, which means nothing.
//!
//! It is now what it should always have been — the view you get looking **down**
//! at the scene. Horizontal is world **x**, matching the viewport above it, so a
//! body sits in the same left-right place in both. Vertical is **depth** into
//! the screen, front at the top. Every body is drawn, not just the selection, so
//! the panel reads as a view of the model rather than a form.
//!
//! A body extrudes as a prism, so from above it is a rectangle: its contour's
//! world **x** extent by its depth band. The extent comes from
//! [`geometry::polygonize`](gradiance_geometry::polygonize) through the host —
//! the single discretization point — because a rotated polygon's x extent is
//! not its width.
//!
//! # Editing
//!
//! - Drag a body's **front or back edge** to resize its band, its **middle** to
//!   move the whole band.
//! - Drag a **depth line** — the horizontal rules at each distinct band edge in
//!   the scene — to move *every* edge sitting on it at once. This is the bulk
//!   edit the old panel had no way to express: "push everything at this depth
//!   back", in one gesture and one undo step.
//!
//! Edges snap to the quarter-layer grid and the view auto-grows when dragged
//! past the bottom. Dragging previews locally (panel-scratch state, invariant 2)
//! and commits exactly **one** `PropertyEditIntent` on release — for a depth
//! line, one intent carrying every affected body's change.

use crate::widgets;
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_command::intent::PropertyEditIntent;
use gradiance_command::property::{PropertyChange, PropertyValue};
use gradiance_core::constants::LAYER_HEIGHT;
use gradiance_core::ids::StableId;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::layers::layer_hue;

/// Pixels of grab slop around an edge before it counts as the middle.
const EDGE_GRAB_PX: f32 = 6.0;
/// Default visible depth: 8 layers.
const DEFAULT_VIEW_DEPTH: f32 = 8.0 * LAYER_HEIGHT;
/// Depths closer together than this are one depth line. A quarter of the snap
/// step, so two edges the snapping made equal are always merged, and two the
/// user deliberately placed a snap step apart never are.
const LINE_EPSILON: f32 = 0.25 * 0.25 * LAYER_HEIGHT;

/// One body as the plan view sees it: a rectangle in (world x, depth).
#[derive(Debug, Clone, Copy)]
pub struct PlanRow {
    /// The body.
    pub id: StableId,
    /// World-x extent of the body's contour (`min`, `max`).
    pub x_extent: (f32, f32),
    /// Authored depth band.
    pub band: DepthBand,
    /// Fill colour, for the footprint.
    pub color: egui::Color32,
    /// Whether the body is in the current selection (drawn highlighted).
    pub selected: bool,
}

/// Which part of the view a drag grabbed.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Grab {
    /// One body's front edge — resizes `near`.
    Near,
    /// One body's back edge — resizes `far`.
    Far,
    /// One body's middle — moves the whole band.
    Whole,
    /// A scene-wide depth line — moves every band edge at that depth.
    Line,
}

/// One in-flight drag (panel-local preview; authored state is only written by
/// the commit intent on release).
#[derive(Debug, Clone, Copy)]
struct BandDrag {
    /// The body being dragged, or `None` for a depth line.
    id: Option<StableId>,
    grab: Grab,
    start: DepthBand,
    current: DepthBand,
    /// Pointer depth minus `near` at grab time (move mode keeps it).
    offset: f32,
    /// For a depth-line drag: the depth the line started at.
    line_depth: f32,
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

crate::impl_panel_toggle!(DepthPanel, open);

/// The band a body drag produces for a pointer at `depth`, snapped and sane.
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
        Grab::Whole | Grab::Line => {
            let near = (depth - drag.offset).max(0.0);
            DepthBand {
                near,
                far: near + drag.start.thickness(),
            }
        }
    };
    band.snapped()
}

/// The distinct depths at which some body's band starts or ends — the scene's
/// depth lines, in order. Two edges within [`LINE_EPSILON`] are one line, so a
/// row of bodies snapped to the same layer gets one draggable rule rather than
/// a dozen coincident ones.
fn depth_lines(rows: &[PlanRow]) -> Vec<f32> {
    let mut depths: Vec<f32> = rows
        .iter()
        .flat_map(|r| [r.band.near, r.band.far])
        .collect();
    depths.sort_by(f32::total_cmp);
    depths.dedup_by(|a, b| (*a - *b).abs() < LINE_EPSILON);
    depths
}

/// Every band edge sitting on `depth`, as `(id, band, which edge)` — what a
/// depth-line drag moves.
fn edges_at(rows: &[PlanRow], depth: f32) -> Vec<(StableId, DepthBand, Grab)> {
    rows.iter()
        .flat_map(|r| {
            [
                ((r.band.near - depth).abs() < LINE_EPSILON).then_some((r.id, r.band, Grab::Near)),
                ((r.band.far - depth).abs() < LINE_EPSILON).then_some((r.id, r.band, Grab::Far)),
            ]
        })
        .flatten()
        .collect()
}

/// The changes a depth-line drag from `from` to `to` produces: every edge on
/// the line moved, each clamped so its band keeps a legal thickness.
///
/// Pure, so the bulk edit — the thing that is genuinely hard to get right — is
/// testable without a window.
fn line_changes(rows: &[PlanRow], from: f32, to: f32) -> Vec<PropertyChange> {
    let mut moved: Vec<(StableId, DepthBand, DepthBand)> = Vec::new();
    for (id, band, edge) in edges_at(rows, from) {
        // A body whose *both* edges are on this line (a zero-thickness band
        // cannot happen, so this is only possible within epsilon) is moved
        // whole rather than pinched.
        let entry = moved.iter_mut().find(|(other, _, _)| *other == id);
        let current = entry.as_ref().map_or(band, |(_, _, b)| *b);
        let next = match edge {
            Grab::Near => DepthBand {
                near: to.min(current.far - DepthBand::MIN_THICKNESS).max(0.0),
                far: current.far,
            },
            _ => DepthBand {
                near: current.near,
                far: to.max(current.near + DepthBand::MIN_THICKNESS),
            },
        }
        .snapped();
        match entry {
            Some((_, _, slot)) => *slot = next,
            None => moved.push((id, band, next)),
        }
    }
    moved
        .into_iter()
        .filter(|(_, old, new)| old != new)
        .map(|(id, old, new)| PropertyChange {
            id,
            old: PropertyValue::Depth(old),
            new: PropertyValue::Depth(new),
        })
        .collect()
}

/// The world-x range the view spans, padded so nothing touches the edges.
/// Falls back to a unit span for an empty or degenerate scene, so the
/// projection never divides by zero.
fn x_range(rows: &[PlanRow]) -> (f32, f32) {
    let lo = rows
        .iter()
        .map(|r| r.x_extent.0)
        .fold(f32::INFINITY, f32::min);
    let hi = rows
        .iter()
        .map(|r| r.x_extent.1)
        .fold(f32::NEG_INFINITY, f32::max);
    if !lo.is_finite() || !hi.is_finite() || (hi - lo).abs() < 1e-3 {
        return (lo.clamp(-1.0, 0.0) - 1.0, hi.clamp(0.0, 1.0) + 1.0);
    }
    let pad = (hi - lo) * 0.05;
    (lo - pad, hi + pad)
}

/// Renders the plan view and turns completed drags into property intents.
/// `rows` is the whole scene projected to footprints; `max_height` bounds the
/// view so the dock can share space with the sections below.
pub fn depth_section(
    ui: &mut egui::Ui,
    panel: &mut DepthPanel,
    rows: &[PlanRow],
    edits: &mut MessageWriter<PropertyEditIntent>,
    max_height: f32,
) {
    // The axis grows to fit the content (and stays grown while dragging).
    let content_deepest = rows
        .iter()
        .map(|r| r.band.far)
        .chain(panel.drag.iter().map(|d| d.current.far))
        .fold(DEFAULT_VIEW_DEPTH, f32::max);
    panel.view_depth = panel
        .view_depth
        .max((content_deepest / LAYER_HEIGHT).ceil() * LAYER_HEIGHT);

    ui.horizontal(|ui| {
        widgets::section_header(ui, "Depth")
            .on_hover_text("looking down at the scene — front ↑, back ↓");
        if widgets::close_button(ui, "close the depth panel") {
            panel.open = false;
            panel.drag = None;
        }
    });
    if rows.is_empty() {
        widgets::empty_state(ui, "Empty scene — nothing to look down on yet.");
        return;
    }

    let height = ui.available_height().min(max_height).max(120.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click_and_drag(),
    );
    let view = Projection::new(rect, panel.view_depth.max(LAYER_HEIGHT), x_range(rows));
    let painter = ui.painter_at(rect);

    draw_layer_grid(&painter, &view);
    let lines = depth_lines(rows);
    draw_depth_lines(&painter, &view, &lines);
    draw_footprints(&painter, &view, rows, panel.drag);

    let pointer = response.hover_pos().or(response.interact_pointer_pos());
    if response.drag_started() && panel.drag.is_none() {
        panel.drag = start_drag(&view, rows, &lines, response.interact_pointer_pos());
    }
    update_drag(panel, rows, &response, &view, edits);

    // Readout for whatever is under the pointer, so depths are legible without
    // dragging anything.
    if let Some(p) = pointer.filter(|p| rect.contains(*p)) {
        ui.label(
            egui::RichText::new(format!("{:.2} m deep", view.depth_of(p.y)))
                .weak()
                .small(),
        );
    }

    ui.label(
        egui::RichText::new(
            "drag an edge to resize · a body to move it · a rule to move everything at that depth",
        )
        .weak()
        .small(),
    );
}

/// The view's world→screen mapping. Bundled so the drawing helpers each take
/// one argument instead of four closures.
struct Projection {
    rect: egui::Rect,
    view_depth: f32,
    x_lo: f32,
    x_span: f32,
}

impl Projection {
    fn new(rect: egui::Rect, view_depth: f32, (x_lo, x_hi): (f32, f32)) -> Self {
        Self {
            rect,
            view_depth,
            x_lo,
            x_span: (x_hi - x_lo).max(1e-3),
        }
    }

    fn y_of(&self, depth: f32) -> f32 {
        self.rect.top() + depth / self.view_depth * self.rect.height()
    }

    fn depth_of(&self, y: f32) -> f32 {
        (y - self.rect.top()) / self.rect.height() * self.view_depth
    }

    fn x_of(&self, world_x: f32) -> f32 {
        self.rect.left() + (world_x - self.x_lo) / self.x_span * self.rect.width()
    }
}

/// Layer slabs as faint horizontal bands in the shared layer hues — the same
/// palette the debug colouring uses, so the panel and the scene agree.
fn draw_layer_grid(painter: &egui::Painter, view: &Projection) {
    let mut layer = 0u32;
    loop {
        let d = layer as f32 * LAYER_HEIGHT;
        if d > view.view_depth {
            break;
        }
        let y = view.y_of(d);
        let hue = layer_hue(layer.min(31));
        let c = gradiance_domain::appearance::Rgba::from_hsl(hue, 0.6, 0.5);
        let color = egui::Color32::from_rgba_unmultiplied(
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            60,
        );
        painter.line_segment(
            [
                egui::pos2(view.rect.left(), y),
                egui::pos2(view.rect.right(), y),
            ],
            egui::Stroke::new(1.0, color),
        );
        if layer < 32 {
            painter.text(
                egui::pos2(view.rect.left() + 2.0, y + 1.0),
                egui::Align2::LEFT_TOP,
                format!("{layer}"),
                egui::FontId::proportional(9.0),
                color,
            );
        }
        layer += 1;
    }
}

/// The draggable scene-wide rules, brighter than the layer grid so the two
/// read as different things: layers are the fixed lattice, depth lines are
/// where the content actually sits.
fn draw_depth_lines(painter: &egui::Painter, view: &Projection, lines: &[f32]) {
    for &depth in lines {
        let y = view.y_of(depth);
        painter.line_segment(
            [
                egui::pos2(view.rect.left(), y),
                egui::pos2(view.rect.right(), y),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
        );
    }
}

/// Every body's footprint: its world-x extent by its depth band. The dragged
/// body previews its in-flight band.
fn draw_footprints(
    painter: &egui::Painter,
    view: &Projection,
    rows: &[PlanRow],
    drag: Option<BandDrag>,
) {
    for row in rows {
        let shown = match drag {
            Some(d) if d.id == Some(row.id) => d.current,
            _ => row.band,
        };
        let footprint = egui::Rect::from_min_max(
            egui::pos2(view.x_of(row.x_extent.0), view.y_of(shown.near)),
            egui::pos2(view.x_of(row.x_extent.1), view.y_of(shown.far)),
        );
        painter.rect_filled(
            footprint,
            2.0,
            row.color
                .gamma_multiply(if row.selected { 1.0 } else { 0.6 }),
        );
        painter.rect_stroke(
            footprint,
            2.0,
            egui::Stroke::new(
                if row.selected { 2.0 } else { 1.0 },
                egui::Color32::from_gray(if row.selected { 240 } else { 120 }),
            ),
            egui::StrokeKind::Outside,
        );
    }
}

/// Classifies a press into a drag: a body edge, a body middle, or a depth line.
///
/// Bodies win over lines, because a line runs the width of the panel and would
/// otherwise swallow every press that lands on a footprint edge — and the edge
/// is the more specific intent.
fn start_drag(
    view: &Projection,
    rows: &[PlanRow],
    lines: &[f32],
    pointer: Option<egui::Pos2>,
) -> Option<BandDrag> {
    let p = pointer?;
    for row in rows {
        let footprint = egui::Rect::from_min_max(
            egui::pos2(view.x_of(row.x_extent.0), view.y_of(row.band.near)),
            egui::pos2(view.x_of(row.x_extent.1), view.y_of(row.band.far)),
        );
        if !footprint.expand(EDGE_GRAB_PX).contains(p) {
            continue;
        }
        let grab = if (p.y - footprint.top()).abs() <= EDGE_GRAB_PX {
            Grab::Near
        } else if (p.y - footprint.bottom()).abs() <= EDGE_GRAB_PX {
            Grab::Far
        } else {
            Grab::Whole
        };
        return Some(BandDrag {
            id: Some(row.id),
            grab,
            start: row.band,
            current: row.band,
            offset: view.depth_of(p.y) - row.band.near,
            line_depth: 0.0,
        });
    }
    // No footprint under the pointer — try the depth lines.
    let depth = view.depth_of(p.y);
    let line = lines
        .iter()
        .copied()
        .find(|d| (view.y_of(*d) - p.y).abs() <= EDGE_GRAB_PX)?;
    Some(BandDrag {
        id: None,
        grab: Grab::Line,
        start: DepthBand {
            near: line,
            far: line,
        },
        current: DepthBand {
            near: line,
            far: line,
        },
        offset: depth - line,
        line_depth: line,
    })
}

/// Advances the in-flight drag and commits it on release.
fn update_drag(
    panel: &mut DepthPanel,
    rows: &[PlanRow],
    response: &egui::Response,
    view: &Projection,
    edits: &mut MessageWriter<PropertyEditIntent>,
) {
    let Some(mut drag) = panel.drag else {
        return;
    };
    if let Some(p) = response.interact_pointer_pos() {
        let depth = view.depth_of(p.y);
        if drag.grab == Grab::Line {
            let moved = ((depth - drag.offset).max(0.0) / (0.25 * LAYER_HEIGHT)).round()
                * (0.25 * LAYER_HEIGHT);
            drag.current = DepthBand {
                near: moved,
                far: moved,
            };
        } else {
            drag.current = dragged_band(&drag, depth);
        }
        // Auto-grow while dragging past the bottom edge.
        if p.y > view.rect.bottom() - 4.0 {
            panel.view_depth += LAYER_HEIGHT;
        }
    }
    if !response.drag_stopped() {
        panel.drag = Some(drag);
        return;
    }
    let changes = if drag.grab == Grab::Line {
        line_changes(rows, drag.line_depth, drag.current.near)
    } else if drag.current == drag.start {
        Vec::new()
    } else {
        drag.id
            .map(|id| {
                vec![PropertyChange {
                    id,
                    old: PropertyValue::Depth(drag.start),
                    new: PropertyValue::Depth(drag.current),
                }]
            })
            .unwrap_or_default()
    };
    if !changes.is_empty() {
        edits.write(PropertyEditIntent { changes });
    }
    panel.drag = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(near: f32, far: f32) -> PlanRow {
        PlanRow {
            id: StableId::new(),
            x_extent: (-1.0, 1.0),
            band: DepthBand { near, far },
            color: egui::Color32::WHITE,
            selected: false,
        }
    }

    #[test]
    fn edge_drags_resize_and_snap() {
        let drag = BandDrag {
            id: Some(StableId::new()),
            grab: Grab::Far,
            start: DepthBand::default(),
            current: DepthBand::default(),
            offset: 0.0,
            line_depth: 0.0,
        };
        let band = dragged_band(&drag, 0.234);
        assert!((band.far - 0.225).abs() < 1e-4, "snaps to quarter layers");
        assert!(band.near.abs() < 1e-6, "near end untouched");
    }

    #[test]
    fn near_drag_cannot_cross_the_far_edge() {
        let start = DepthBand {
            near: 0.0,
            far: 0.1,
        };
        let drag = BandDrag {
            id: Some(StableId::new()),
            grab: Grab::Near,
            start,
            current: start,
            offset: 0.0,
            line_depth: 0.0,
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
            id: Some(StableId::new()),
            grab: Grab::Whole,
            start,
            current: start,
            offset: 0.05, // grabbed 0.05 m below the front face
            line_depth: 0.0,
        };
        let moved = dragged_band(&drag, 0.425);
        assert!((moved.thickness() - start.thickness()).abs() < 1e-4);
        let clamped = dragged_band(&drag, -1.0);
        assert!(clamped.near.abs() < 1e-6, "cannot go in front of the plane");
    }

    /// Bodies snapped to the same layer share one rule; distinct depths keep
    /// their own. Without the merge, a row of ten aligned boxes would stack ten
    /// coincident lines and dragging would pick an arbitrary one.
    #[test]
    fn coincident_edges_collapse_into_one_depth_line() {
        let rows = [
            row(0.0, 0.1),
            row(0.0, 0.1),
            row(0.1, 0.3),
            row(0.0, 0.1 + LINE_EPSILON * 0.4),
        ];
        let lines = depth_lines(&rows);
        assert_eq!(lines.len(), 3, "0.0, 0.1, 0.3 — not seven: {lines:?}");
        assert!(lines.windows(2).all(|w| w[0] < w[1]), "sorted");
    }

    /// The bulk edit: one gesture moves every edge on the rule, and only those.
    #[test]
    fn a_depth_line_drag_moves_every_edge_on_it() {
        let rows = [row(0.0, 0.1), row(0.1, 0.3), row(0.5, 0.6)];
        let changes = line_changes(&rows, 0.1, 0.2);
        assert_eq!(changes.len(), 2, "the two bodies touching depth 0.1");
        // The first body's back edge moved back; the second's front edge did.
        let bands: Vec<DepthBand> = changes
            .iter()
            .map(|c| match c.new {
                PropertyValue::Depth(b) => b,
                _ => panic!("depth change"),
            })
            .collect();
        assert!((bands[0].far - 0.2).abs() < 1e-4);
        assert!((bands[0].near - 0.0).abs() < 1e-6, "other edge untouched");
        assert!((bands[1].near - 0.2).abs() < 1e-4);
        assert!((bands[1].far - 0.3).abs() < 1e-4);
    }

    /// Dragging a rule onto (or past) the opposite edge must not invert or
    /// flatten a band — the same clamp a single-edge drag gets.
    #[test]
    fn a_depth_line_drag_cannot_pinch_a_band_flat() {
        let rows = [row(0.0, 0.1)];
        let changes = line_changes(&rows, 0.1, 0.0);
        let PropertyValue::Depth(band) = changes[0].new else {
            panic!("depth change");
        };
        assert!(band.thickness() >= DepthBand::MIN_THICKNESS - 1e-4);
        // Dragging the front edge backwards past the back edge is clamped too.
        let changes = line_changes(&rows, 0.0, 0.9);
        let PropertyValue::Depth(band) = changes[0].new else {
            panic!("depth change");
        };
        assert!(band.thickness() >= DepthBand::MIN_THICKNESS - 1e-4);
    }

    #[test]
    fn a_line_drag_that_changes_nothing_produces_no_changes() {
        let rows = [row(0.0, 0.1), row(0.1, 0.3)];
        assert!(
            line_changes(&rows, 0.1, 0.1).is_empty(),
            "moving a rule to where it already is is not an edit"
        );
        assert!(
            line_changes(&rows, 0.7, 0.8).is_empty(),
            "a depth with no edges on it moves nothing"
        );
    }

    /// The horizontal axis must survive the shapes a real scene produces:
    /// nothing at all, and every body at the same x.
    #[test]
    fn the_x_range_is_never_degenerate() {
        let (lo, hi) = x_range(&[]);
        assert!(hi > lo, "an empty scene still has a span: {lo}..{hi}");
        let mut same = row(0.0, 0.1);
        same.x_extent = (2.0, 2.0);
        let (lo, hi) = x_range(&[same]);
        assert!(hi > lo, "a zero-width scene still has a span: {lo}..{hi}");
        // A normal scene is padded outward, never inward.
        let mut a = row(0.0, 0.1);
        a.x_extent = (-3.0, 5.0);
        let (lo, hi) = x_range(&[a]);
        assert!(lo < -3.0 && hi > 5.0);
    }
}
