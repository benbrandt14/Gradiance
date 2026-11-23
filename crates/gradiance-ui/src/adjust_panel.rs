//! Adjust Last Operation (brief U-11): a small collapsible panel at the
//! viewport's bottom left that shows the params of the operation that made
//! the newest undo step, as fields. Changing a field invokes
//! `edit.adjust_last`, which undoes that step and runs the operation again
//! with the new value: still one undo step.
//!
//! The panel is a projection of the registry: its rows come from the
//! operation's params schema ([`OpEntry::schema`]) and the params of the
//! call ([`LastOp`]), so a new operation gets the panel with no edit here.
//! Numbers and quantities are `precise_drag` fields (the edit seam: one
//! commit per gesture, on release), flags are checkboxes, a fixed-length
//! list of numbers is a row of fields; anything else (an object target, a
//! text) is shown read-only.
//!
//! It shows only while there is something to adjust: the newest step was
//! made by one operation call ([`LastOp`]) whose params have at least one
//! editable field. It writes nothing but an [`InvokeOp`].

use crate::widgets::{Commit, precise_drag_int, precise_drag_unit};
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_op::{InvokeOp, LastOp, OpEntry, OpRegistry, Value, json};
use gradiance_units::Dimension;

/// One editable number inside the params: its dotted path, value and kind.
#[derive(Debug, Clone, PartialEq)]
pub struct NumberSlot {
    /// The path `edit.adjust_last` takes (`"w"`, `"step.0"`).
    pub path: String,
    /// The value now (base SI for a quantity).
    pub value: f32,
    /// What middle-click and Backspace reset to: the operation's example
    /// value for this slot, else the value now.
    pub default: f32,
    /// The quantity's dimension (`Dimensionless` for a plain number).
    pub dim: Dimension,
    /// A whole number (a count).
    pub integer: bool,
}

/// What one param shows as.
#[derive(Debug, Clone, PartialEq)]
pub enum RowKind {
    /// One number field.
    Number(NumberSlot),
    /// A fixed-length list of numbers, one field each (a vector).
    Vector(Vec<NumberSlot>),
    /// A checkbox.
    Flag {
        /// The param's path.
        path: String,
        /// Its value now.
        value: bool,
    },
    /// No widget: the value as text.
    ReadOnly(String),
}

/// One param row: its name and how it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The param's name.
    pub name: String,
    /// How it shows.
    pub kind: RowKind,
}

/// The panel's title for an operation: its menu label, else its Lisp name
/// in words (`spawn-box` reads "Spawn box").
pub fn title(entry: &OpEntry) -> String {
    if let Some(label) = entry.meta.menu.and_then(|path| path.rsplit('/').next()) {
        return label.to_owned();
    }
    let words = entry.meta.lisp.replace('-', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The number kind a field schema names: `(dimension, integer)`, or `None`
/// when it is not a number.
fn number_kind(schema: &Value) -> Option<(Dimension, bool)> {
    match schema.get("type").and_then(Value::as_str) {
        Some("number") => return Some((Dimension::Dimensionless, false)),
        Some("integer") => return Some((Dimension::Dimensionless, true)),
        _ => {}
    }
    schema
        .get("description")
        .and_then(Value::as_str)
        .and_then(Dimension::of_quantity)
        .map(|dim| (dim, false))
}

/// The rows for a call of `entry` with `params`: one per declared field,
/// in declared order. Pure, so it is unit-tested without a window.
pub fn rows(entry: &OpEntry, params: &Value) -> Vec<Row> {
    let schema = entry.schema();
    let example = entry.example();
    entry
        .fields
        .iter()
        .map(|name| {
            let field = &schema["properties"][*name];
            let value = &params[*name];
            let slot = |path: String, value: &Value, default: &Value, (dim, integer)| {
                value.as_f64().map(|v| NumberSlot {
                    path,
                    value: v as f32,
                    default: default.as_f64().unwrap_or(v) as f32,
                    dim,
                    integer,
                })
            };
            let kind = if let Some(kind) = number_kind(field) {
                slot((*name).to_owned(), value, &example[*name], kind).map(RowKind::Number)
            } else if field.get("type").and_then(Value::as_str) == Some("boolean") {
                value.as_bool().map(|value| RowKind::Flag {
                    path: (*name).to_owned(),
                    value,
                })
            } else if field.get("type").and_then(Value::as_str) == Some("array")
                && field.get("maxItems").is_some()
                && let Some(kind) = number_kind(&field["items"])
            {
                value.as_array().and_then(|items| {
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, item)| {
                            slot(format!("{name}.{i}"), item, &example[*name][i], kind)
                        })
                        .collect::<Option<Vec<_>>>()
                        .map(RowKind::Vector)
                })
            } else {
                None
            };
            Row {
                name: (*name).to_owned(),
                kind: kind.unwrap_or_else(|| RowKind::ReadOnly(short(value))),
            }
        })
        .collect()
}

/// Most items a read-only list shows before it is summarised as a count.
const SHORT_LIST: usize = 4;

/// A value as short text: an object id shows its first eight digits, a
/// number at most three decimals, and a list longer than [`SHORT_LIST`]
/// its length (brief Q-1: a drawn chain's path printed every coordinate at
/// full precision and stretched the panel across the window). An omitted
/// optional field reads `auto`: the operation chose its value (the body's
/// current one, or the default), and JSON `null` meant nothing to a reader.
fn short(value: &Value) -> String {
    match value {
        Value::Null => "auto".to_owned(),
        Value::String(text) if text.len() > 12 => format!("{}..", &text[..8]),
        Value::String(text) => text.clone(),
        Value::Number(n) => n.as_f64().map_or_else(|| n.to_string(), decimals),
        Value::Array(items) if items.len() > SHORT_LIST => format!("{} values", items.len()),
        Value::Array(items) => items.iter().map(short).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

/// `v` with at most three decimals and no trailing zeros.
fn decimals(v: f64) -> String {
    let text = format!("{v:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// Whether any row can be edited: the panel shows only then.
pub fn adjustable(rows: &[Row]) -> bool {
    rows.iter()
        .any(|row| !matches!(row.kind, RowKind::ReadOnly(_)))
}

/// Whether the panel shows expanded. Editor state (never saved).
#[derive(Resource, Debug, Clone, Copy)]
pub struct AdjustPanel {
    /// Expanded (the default) or collapsed to its title.
    pub expanded: bool,
}

impl Default for AdjustPanel {
    fn default() -> Self {
        Self { expanded: true }
    }
}

/// The drag speed (base SI per point) of a slot.
fn speed(slot: &NumberSlot) -> f32 {
    match (slot.integer, slot.dim) {
        (true, _) => 0.05,
        (false, Dimension::Angle) => 0.005,
        _ => 0.01,
    }
}

/// One number field; a finished gesture becomes one adjustment.
fn number(ui: &mut egui::Ui, slot: &NumberSlot, out: &mut Vec<InvokeOp>) {
    let id = egui::Id::new(("adjust-last", slot.path.as_str()));
    let mut value = slot.value;
    let shown = ui.scope(|ui| {
        if slot.integer {
            precise_drag_int(ui, id, &mut value, slot.default, speed(slot))
        } else {
            precise_drag_unit(ui, id, &mut value, slot.default, speed(slot), slot.dim)
        }
    });
    // Named for `ui.inspect` and the input operations: `adjust_last:w`.
    crate::widget_registry::publish_rect(
        &format!("adjust_last:{}", slot.path),
        shown.response.rect,
    );
    let commit = shown.inner;
    if let Commit::Done(_, new) = commit {
        // The shortest decimal that reads back as this `f32` (`1.95`, not
        // `1.9500000476837158`), so the params stay as a person wrote them.
        let value = new.to_string().parse::<f64>().unwrap_or(f64::from(new));
        out.push(adjust(&slot.path, json!(value)));
    }
}

/// The adjustment of one field.
fn adjust(path: &str, value: Value) -> InvokeOp {
    InvokeOp::with("edit.adjust_last", json!({"field": path, "value": value}))
}

/// Draws the panel's body (a leaf renderer) and returns the adjustments the
/// person made this frame.
pub fn adjust_ui(ui: &mut egui::Ui, rows: &[Row]) -> Vec<InvokeOp> {
    let mut out = Vec::new();
    egui::Grid::new("adjust-last-grid")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for row in rows {
                ui.label(&row.name);
                match &row.kind {
                    RowKind::Number(slot) => number(ui, slot, &mut out),
                    RowKind::Vector(slots) => {
                        ui.horizontal(|ui| {
                            for slot in slots {
                                number(ui, slot, &mut out);
                            }
                        });
                    }
                    RowKind::Flag { path, value } => {
                        let mut on = *value;
                        if ui.checkbox(&mut on, "").changed() {
                            out.push(adjust(path, json!(on)));
                        }
                    }
                    RowKind::ReadOnly(text) => {
                        let tokens = crate::theme::tokens(ui.ctx());
                        ui.label(egui::RichText::new(text).color(tokens.text_weak));
                    }
                }
                ui.end_row();
            }
        });
    out
}

/// Draws the panel at the viewport's bottom left, right of the tool
/// palette and above the bottom docks, while there is an operation to
/// adjust.
pub fn adjust_last_panel(
    mut contexts: bevy_egui::EguiContexts,
    last: Res<LastOp>,
    registry: Res<OpRegistry>,
    mut panel: ResMut<AdjustPanel>,
    panel_rects: Res<crate::PanelRects>,
    mut invoke: MessageWriter<InvokeOp>,
) -> Result {
    let Some(record) = last.0.as_ref() else {
        return Ok(());
    };
    let Some(entry) = registry.get(record.call.id) else {
        return Ok(());
    };
    let rows = rows(entry, &record.call.params);
    if !adjustable(&rows) {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let viewport = ctx.viewport_rect();
    let lift = viewport.bottom() - panel_rects.bottom_inset(viewport) + 8.0;
    let mut expanded = panel.expanded;
    let shown = egui::Window::new("adjust-last")
        .title_bar(false)
        .resizable(false)
        .movable(false)
        .auto_sized()
        .anchor(egui::Align2::LEFT_BOTTOM, [PALETTE_CLEARANCE, -lift])
        .show(ctx, |ui| {
            let tokens = crate::theme::tokens(ui.ctx());
            let arrow = if expanded {
                crate::fonts::glyph::TRIANGLE_DOWN
            } else {
                crate::fonts::glyph::TRIANGLE_RIGHT
            };
            let header = ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format!("{arrow} {}", title(entry)))
                            .color(tokens.text_strong),
                    )
                    .frame(false),
                )
                .on_hover_text(
                    "adjust the last operation: a change runs it again as one undo step",
                );
            crate::widget_registry::publish_rect("adjust_last:header", header.rect);
            if header.clicked() {
                expanded = !expanded;
            }
            if expanded {
                for request in adjust_ui(ui, &rows) {
                    invoke.write(request);
                }
            }
        });
    if let Some(shown) = shown {
        crate::widget_registry::publish_rect("adjust_last", shown.response.rect);
    }
    if panel.expanded != expanded {
        panel.expanded = expanded;
    }
    Ok(())
}

/// The left offset that clears the tool palette (its strip is about 60
/// points wide at 8 from the edge).
const PALETTE_CLEARANCE: f32 = 80.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(app: &App, id: &str) -> OpEntry {
        app.world()
            .resource::<OpRegistry>()
            .get(id)
            .cloned()
            .unwrap_or_else(|| panic!("{id} is registered"))
    }

    fn app() -> App {
        let mut app = App::new();
        gradiance_command::ops::install(&mut app);
        app
    }

    #[test]
    fn a_spawn_shows_four_length_fields_titled_from_its_name() {
        let app = app();
        let spawn = entry(&app, "body.spawn_box");
        assert_eq!(title(&spawn), "Spawn box");
        let rows = rows(&spawn, &json!({"x": 0.0, "y": 1.0, "w": 2.0, "h": 0.5}));
        assert_eq!(rows.len(), 4);
        let RowKind::Number(w) = &rows[2].kind else {
            panic!("w is a number: {rows:?}");
        };
        assert_eq!(
            (w.path.as_str(), w.value, w.dim),
            ("w", 2.0, Dimension::Length)
        );
        // The reset value is the operation's example.
        assert!((w.default - 1.6).abs() < 1e-6);
        assert!(adjustable(&rows));
    }

    #[test]
    fn an_array_shows_counts_vectors_and_its_targets_read_only() {
        let app = app();
        let array = entry(&app, "body.array");
        let rows = rows(
            &array,
            &json!({"targets": ["selection"], "count": 3, "step": [1.5, 0.0], "rows": 0, "cross": [0.0, 0.0]}),
        );
        assert_eq!(rows[0].kind, RowKind::ReadOnly("selection".to_owned()));
        assert!(
            matches!(&rows[1].kind, RowKind::Number(n) if n.integer && (n.value - 3.0).abs() < 1e-6)
        );
        let RowKind::Vector(step) = &rows[2].kind else {
            panic!("step is a vector: {rows:?}");
        };
        assert_eq!(step.len(), 2);
        assert_eq!(step[1].path, "step.1");
        assert_eq!(step[0].dim, Dimension::Length);
    }

    #[test]
    fn a_long_path_reads_as_a_count_and_numbers_are_rounded() {
        let app = app();
        let chain = entry(&app, "body.chain");
        let rows = rows(
            &chain,
            &json!({"path": [-2.399_999_618_530_273_4, 0.6, -2.35, 0.6, 2.0, 0.6], "link": 0.25,
                    "width": 0.06, "a": -1, "b": -1, "pins": "bodies"}),
        );
        assert_eq!(rows[0].kind, RowKind::ReadOnly("6 values".to_owned()));
        assert_eq!(short(&json!([-2.399_999_618_530_273_4, 0.5])), "-2.4, 0.5");
        assert_eq!(short(&json!(-0.000_1)), "0");
    }

    #[test]
    fn an_operation_with_only_targets_is_not_adjustable() {
        let app = app();
        let delete = entry(&app, "body.delete");
        assert!(!adjustable(&rows(&delete, &json!({"i": "selection"}))));
        let merge = entry(&app, "body.fillet_merge");
        let rows = rows(
            &merge,
            &json!({"a": "selection", "b": "selection", "radius": 0.2}),
        );
        assert!(adjustable(&rows));
        assert_eq!(title(&merge), "Merge with fillet");
    }

    #[test]
    fn an_omitted_optional_number_reads_auto_not_null() {
        let app = app();
        let fluid = entry(&app, "body.set_fluid");
        let rows = rows(&fluid, &json!({"i": 2, "density": 2.0}));
        assert_eq!(rows[2].name, "drag");
        assert_eq!(rows[2].kind, RowKind::ReadOnly("auto".to_owned()));
        assert!(adjustable(&rows), "density stays editable");
    }
}
