//! F3 search over the operation registry (brief U-9).
//!
//! F3 opens a box over **every operation a person can invoke with a click**:
//! an operation that is not a query and whose params a click can supply
//! ([`OpEntry::selection_params`]: no params, or only object targets, which
//! become `"selection"` as the Edit and context menus write them). Typing
//! ranks the rows by name, menu path, Lisp name, id and doc
//! ([`gradiance_op::search`]); arrows move, Enter or a click invokes through
//! [`InvokeOp`] (the path the Edit menu takes), Esc or F3 closes. The search
//! never writes an intent.
//!
//! **Enabled** is a dry run: each row is lowered against the operation
//! context the invocation would read (`OpContext::build`). A row whose
//! lowering fails shows the error as its reason; one that would write
//! nothing (Delete with nothing selected) says so. The registry has no
//! enable predicate of its own; lowering is the one place that knows.
//!
//! It is a pure projection: a new operation appears here with no edit. The
//! rows are built in [`refresh_search`] (exclusive: it builds a context),
//! drawn by [`search_ui`] (a leaf renderer, `egui_kittest`-tested), and the
//! same ranking answers the `ui.search` operation ([`UiSearch`]).

use crate::fonts::glyph;
use crate::widget_registry;
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_op::search::{Field, rank};
use gradiance_op::{
    InvokeOp, OpContext, OpEntry, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Value,
};
use serde::Deserialize;
use std::borrow::Cow;

/// The F3 box's state: editor state, never saved.
#[derive(Resource, Debug, Default, Clone)]
pub struct SearchPanel {
    /// Whether the box is showing.
    pub open: bool,
    /// The text typed so far.
    pub query: String,
    /// The highlighted row (Enter invokes it).
    pub highlight: usize,
    /// Give the text field the keyboard on the next draw.
    focus: bool,
}

impl crate::panels::PanelToggle for SearchPanel {
    fn is_open(&self) -> bool {
        self.open
    }

    fn set_open(&mut self, open: bool) {
        if open && !self.open {
            self.query.clear();
            self.highlight = 0;
            self.focus = true;
        }
        self.open = open;
    }
}

/// One row of the search: an operation, the params a click supplies, and
/// why it cannot run now (if it cannot).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchRow {
    /// The operation's id.
    pub id: &'static str,
    /// What the row reads as: the menu path's last part, else the Lisp name.
    pub label: &'static str,
    /// The menu path, if the operation has one.
    pub menu: Option<&'static str>,
    /// The Lisp name.
    pub lisp: &'static str,
    /// The one-line doc.
    pub doc: &'static str,
    /// The key chord, if the operation declares one.
    pub chord: Option<&'static str>,
    /// The params the invocation carries.
    pub params: Value,
    /// Why the row is greyed: `None` when it can run.
    pub disabled: Option<String>,
}

impl SearchRow {
    /// The invocation Enter or a click makes.
    pub fn invocation(&self) -> InvokeOp {
        InvokeOp {
            id: Cow::Borrowed(self.id),
            params: self.params.clone(),
        }
    }
}

/// The text a row is called: the last part of its menu path, else its Lisp
/// name.
fn label_of(meta: &OpMeta) -> &'static str {
    meta.menu
        .and_then(|path| path.rsplit('/').next())
        .unwrap_or(meta.lisp)
}

/// The operations F3 lists: not queries, and every param fillable by a
/// click. Each with the params it is invoked with.
pub fn candidates(entries: &[OpEntry]) -> Vec<(&OpEntry, Value)> {
    entries
        .iter()
        .filter(|entry| entry.meta.seam != Seam::Query)
        .filter_map(|entry| entry.selection_params().map(|params| (entry, params)))
        .collect()
}

/// Why `entry` cannot run with `params` against `ctx`, or `None` if it can:
/// a dry lowering (nothing is applied).
fn why_disabled(entry: &OpEntry, params: &Value, ctx: &OpContext) -> Option<String> {
    match entry.lower(params.clone(), ctx) {
        Err(err) => Some(err.to_string()),
        Ok(output) if output.writes_nothing() && output.value().is_none() => {
            let on_selection = params.as_object().is_some_and(|o| !o.is_empty());
            Some(if on_selection {
                "nothing selected it applies to".to_owned()
            } else {
                "nothing to do now".to_owned()
            })
        }
        Ok(_) => None,
    }
}

/// The order `query` ranks `candidates` in, best first: by name (4),
/// menu path and Lisp name (3), id (2) and doc (1).
fn rank_candidates(query: &str, candidates: &[(&OpEntry, Value)]) -> Vec<usize> {
    rank(query, candidates, |(entry, _)| {
        let meta = &entry.meta;
        vec![
            Field::new(label_of(meta), 4),
            Field::new(meta.menu.unwrap_or_default(), 3),
            Field::new(meta.lisp, 3),
            Field::new(meta.id, 2),
            Field::new(meta.doc, 1),
        ]
    })
}

/// The rows matching `query`, best first, each checked against `ctx`.
pub fn search_rows(entries: &[OpEntry], ctx: &OpContext, query: &str) -> Vec<SearchRow> {
    let candidates = candidates(entries);
    rank_candidates(query, &candidates)
        .into_iter()
        .filter_map(|i| candidates.get(i))
        .map(|(entry, params)| SearchRow {
            id: entry.meta.id,
            label: label_of(&entry.meta),
            menu: entry.meta.menu,
            lisp: entry.meta.lisp,
            doc: entry.meta.doc,
            chord: entry.meta.chord,
            disabled: why_disabled(entry, params, ctx),
            params: params.clone(),
        })
        .collect()
}

/// The rows the box draws, refreshed while it is open.
#[derive(Resource, Debug, Default, Clone)]
pub struct SearchRows(pub Vec<SearchRow>);

/// Exclusive: rebuilds [`SearchRows`] for the typed query while the box is
/// open. It builds the operation context an invocation would read, so a
/// row's enabled state is the state its click would meet.
pub fn refresh_search(world: &mut World) {
    let Some(query) = world
        .get_resource::<SearchPanel>()
        .filter(|panel| panel.open)
        .map(|panel| panel.query.clone())
    else {
        return;
    };
    let Some(entries) = world
        .get_resource::<OpRegistry>()
        .map(|registry| registry.entries().to_vec())
    else {
        return;
    };
    let ctx = OpContext::build(world);
    let rows = search_rows(&entries, &ctx, &query);
    world.insert_resource(SearchRows(rows));
}

/// The most rows the box shows at once (it scrolls past that).
const MAX_ROWS: usize = 12;
/// The box's width in points.
const WIDTH: f32 = 460.0;

/// The F3 box as a leaf renderer: handles F3 (open or close), the arrows,
/// Enter and Esc, draws the field and the rows, and returns the invocation a
/// person chose. Host-agnostic, so `egui_kittest` drives it.
pub fn search_ui(
    ctx: &egui::Context,
    panel: &mut SearchPanel,
    rows: &[SearchRow],
) -> Option<InvokeOp> {
    use crate::panels::PanelToggle;
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F3)) {
        panel.toggle();
    }
    if !panel.open {
        return None;
    }
    // The keys the box owns, taken before the text field sees them.
    let (escape, down, up, enter) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
        )
    });
    if escape {
        panel.set_open(false);
        return None;
    }
    let last = rows.len().saturating_sub(1);
    if down {
        panel.highlight = (panel.highlight + 1).min(last);
    }
    if up {
        panel.highlight = panel.highlight.saturating_sub(1);
    }
    panel.highlight = panel.highlight.min(last);
    let mut chosen = None;
    if enter
        && let Some(row) = rows.get(panel.highlight)
        && row.disabled.is_none()
    {
        chosen = Some(row.invocation());
    }

    let tokens = crate::theme::tokens(ctx);
    let shown = egui::Area::new(egui::Id::new("f3-search"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 90.0])
        .show(ctx, |ui| {
            egui::Frame::popup(&ctx.global_style()).show(ui, |ui| {
                ui.set_width(WIDTH);
                let hint = format!("Search operations{}", glyph::ELLIPSIS);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut panel.query)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY),
                );
                widget_registry::publish_target("search:field", field.rect);
                if panel.focus {
                    field.request_focus();
                    panel.focus = false;
                }
                if field.changed() {
                    panel.highlight = 0;
                }
                ui.separator();
                if rows.is_empty() {
                    crate::widgets::empty_state(ui, "No operation matches: try fewer letters");
                    return;
                }
                egui::ScrollArea::vertical()
                    .max_height(MAX_ROWS as f32 * 34.0)
                    .show(ui, |ui| {
                        for (i, row) in rows.iter().enumerate() {
                            if let Some(invocation) = row_ui(ui, row, i == panel.highlight, &tokens)
                            {
                                chosen = Some(invocation);
                            }
                        }
                    });
            });
        });
    widget_registry::publish_rect("search", shown.response.rect);
    if chosen.is_some() {
        panel.set_open(false);
    }
    chosen
}

/// One row: the label and menu path, the chord at the right, the doc (or
/// the reason it is greyed) under it. Returns its invocation when clicked.
fn row_ui(
    ui: &mut egui::Ui,
    row: &SearchRow,
    highlighted: bool,
    tokens: &crate::theme::ChromeTheme,
) -> Option<InvokeOp> {
    let enabled = row.disabled.is_none();
    let (text, weak) = if enabled {
        (tokens.text, tokens.text_weak)
    } else {
        (tokens.text_weak, tokens.text_weak)
    };
    let path = row.menu.map_or_else(
        || row.lisp.to_owned(),
        |m| m.replace('/', &format!(" {} ", glyph::TRIANGLE_RIGHT)),
    );
    let detail = row.disabled.as_deref().unwrap_or(row.doc);
    let response = ui
        .scope(|ui| {
            let frame = egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(6, 3))
                .corner_radius(3.0)
                .fill(if highlighted {
                    tokens.selected
                } else {
                    egui::Color32::TRANSPARENT
                });
            frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let strong = if highlighted {
                            tokens.text_strong
                        } else {
                            text
                        };
                        ui.label(egui::RichText::new(row.label).color(strong));
                        ui.label(egui::RichText::new(path).color(weak).small());
                        if let Some(chord) = row.chord {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(egui::RichText::new(chord).color(weak).small());
                                },
                            );
                        }
                    });
                    let detail_color = if highlighted { tokens.text } else { weak };
                    ui.label(egui::RichText::new(detail).color(detail_color).small());
                })
                .response
        })
        .inner
        .interact(egui::Sense::click());
    widget_registry::publish_target(&format!("search:{}", row.label), response.rect);
    let response = response.on_hover_text(format!("{} ({})", row.id, row.lisp));
    (enabled && response.clicked()).then(|| row.invocation())
}

/// Draws the box in the app and writes what it chose.
pub fn search_window(
    mut contexts: bevy_egui::EguiContexts,
    mut panel: ResMut<SearchPanel>,
    rows: Res<SearchRows>,
    mut invoke: MessageWriter<InvokeOp>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if let Some(invocation) = search_ui(ctx, &mut panel, &rows.0) {
        invoke.write(invocation);
    }
    Ok(())
}

/// The registry entries, published into the operation context for
/// [`UiSearch`] (an operation reads only its context).
#[derive(Debug, Clone, Default)]
pub struct SearchCatalog(pub Vec<OpEntry>);

/// Publishes [`SearchCatalog`] into a new operation context.
pub(crate) fn fill_search_catalog(world: &mut World, ctx: &mut OpContext) {
    if let Some(registry) = world.get_resource::<OpRegistry>() {
        ctx.insert(SearchCatalog(registry.entries().to_vec()));
    }
}

/// Rank the operations F3 lists for a query.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct UiSearch {
    /// The search text (`""`: every row, in registry order).
    pub text: String,
}

impl Operation for UiSearch {
    const META: OpMeta = OpMeta {
        id: "ui.search",
        lisp: "ui-search",
        doc: "The ids of the operations F3 search lists for text, best first (a click-runnable operation: no params, or only objects, filled by the selection).",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"text": "undo"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let catalog = ctx.get::<SearchCatalog>()?;
        // Ids only: whether a row can run depends on the doorway's context
        // (a script run has no selection), the ranking does not.
        let candidates = candidates(&catalog.0);
        let rows: Vec<Value> = rank_candidates(&self.text, &candidates)
            .into_iter()
            .filter_map(|i| candidates.get(i))
            .map(|(entry, _)| Value::from(entry.meta.id))
            .collect();
        Ok(OpOutput::none().returning(Value::Array(rows)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::PanelToggle;

    #[test]
    fn opening_clears_the_last_query_and_asks_for_focus() {
        let mut panel = SearchPanel {
            query: "old".to_owned(),
            highlight: 3,
            ..SearchPanel::default()
        };
        panel.set_open(true);
        assert!(panel.open && panel.focus);
        assert_eq!((panel.query.as_str(), panel.highlight), ("", 0));
        // Showing an open box again keeps what was typed.
        panel.query.push('x');
        panel.set_open(true);
        assert_eq!(panel.query, "x");
    }

    #[test]
    fn a_label_is_the_menu_leaf_else_the_lisp_name() {
        let meta = OpMeta {
            id: "a.b",
            lisp: "a-b",
            doc: "",
            seam: Seam::Edit,
            menu: Some("Edit/Delete selection"),
            chord: None,
            undo: None,
            example: "{}",
        };
        assert_eq!(label_of(&meta), "Delete selection");
        assert_eq!(label_of(&OpMeta { menu: None, ..meta }), "a-b");
    }
}
