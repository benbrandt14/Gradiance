//! The **Groups** panel (brief 4b-2): groups as editing layers, the way a
//! drawing tool lists layers. Every group is a row (swatch, name, member
//! count, nesting by indentation); clicking a row selects the group and makes
//! it the one the edit controls act on; double-clicking renames it.
//!
//! Below the rows, for the active group:
//! - **colour**: one swatch per palette entry (`group.set_color`);
//! - **set**: a property from `BODY_PROPS` and a value (`group.set`), a
//!   checkbox for a flag property;
//! - **distribute**: lo, hi, a sort key, linear or seeded random
//!   (`group.distribute` / `group.scatter`);
//! - **split**: by colour, kind or joint island, or dissolve
//!   (`group.decompose`).
//!
//! Then the **filter**: chips for the shape kinds and palette colours in the
//! scene, grouped / ungrouped, and property ranges. The chips build a
//! `body.where` clause list; the result becomes the selection, and "Group
//! selection" makes a group of it (the Ctrl+G invocation).
//!
//! # Seams
//!
//! The panel builds parameters and nothing else. Every edit is an
//! [`InvokeOp`] (the message the Edit menu writes), so the panel adds no
//! mutation path and holds no logic a new shell would need. Selection is the
//! one write that is not an operation (no `select` operation exists): a row
//! click or a chip change writes a [`GroupsFilter`], and
//! [`apply_groups_filters`] runs `body.where` against a fresh operation
//! context and applies the indices through [`SelectTransition`], the
//! sanctioned seam. A row click is the filter `group:<index>`, so both use
//! one path. The selection is exactly the query's result, **not** expanded
//! to outermost groups as a viewport click is: a chip on a mixed group
//! selects only the matching bodies, and a nested group's row selects that
//! group (brief A-1).
//!
//! The renderer ([`groups_section`]) is a leaf: it reads a prebuilt
//! [`GroupsModel`] and records what was asked into a [`GroupsOutput`], which
//! the dock writes as messages. Widgets are published to the widget
//! registry as `groups:<name>`, so `ui.click` can drive the panel by name.

use crate::widgets::{self, Commit};
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_command::body_prop::BODY_PROPS;
use gradiance_core::ids::StableId;
use gradiance_domain::Body;
use gradiance_domain::appearance::{PALETTE, Rgba, palette_slot};
use gradiance_domain::group::{GroupIndex, GroupNames, SelectionGroup};
use gradiance_interaction::input::{EditorAction, key_invocations};
use gradiance_interaction::selection::{SelectTransition, SelectedJoint, Selection};
use gradiance_op::{InvokeOp, OpContext, OpRegistry, json};

/// The decompose keys the split row offers, with their button labels.
const SPLIT_KEYS: [(&str, &str); 3] = [
    ("color", "By colour"),
    ("kind", "By kind"),
    ("islands", "By island"),
];

/// The distribute sort keys (`group.distribute`'s `by`).
const SORT_KEYS: [&str; 5] = ["order", "x", "y", "size", "value"];

/// The Groups panel: visibility plus the panel's own editor state (the
/// active group, the picked property, the filter chips). Editor view-state;
/// never persisted.
#[derive(Resource, Debug)]
pub struct GroupsPanel {
    open: bool,
    /// The group the edit controls act on, by id.
    active: Option<u32>,
    /// The row being renamed and the text typed so far.
    renaming: Option<(u32, String)>,
    /// The picked property, as a row of `BODY_PROPS`.
    prop: usize,
    /// The value `group.set` writes for a scalar property.
    value: f32,
    /// The value `group.set` writes for a flag property.
    flag: bool,
    /// Distribute bounds.
    lo: f32,
    hi: f32,
    /// Distribute sort key, as an index into [`SORT_KEYS`].
    by: usize,
    /// Seeded random (`group.scatter`) instead of linear.
    random: bool,
    seed: u32,
    /// The filter's clauses (`body.where`), in the order they were added.
    clauses: Vec<String>,
    /// The range-chip builder: property row and bounds.
    range_prop: usize,
    range_lo: f32,
    range_hi: f32,
}

impl Default for GroupsPanel {
    fn default() -> Self {
        Self {
            open: false,
            active: None,
            renaming: None,
            prop: 0,
            value: 0.5,
            flag: false,
            lo: 0.5,
            hi: 5.0,
            by: 1,
            random: false,
            seed: 1,
            clauses: Vec::new(),
            range_prop: 0,
            range_lo: 0.0,
            range_hi: 1.0,
        }
    }
}

crate::impl_panel_toggle!(GroupsPanel, open);

impl GroupsPanel {
    /// The active group's id, if a row was clicked.
    pub fn active(&self) -> Option<u32> {
        self.active
    }

    /// The filter's current clauses.
    pub fn clauses(&self) -> &[String] {
        &self.clauses
    }

    /// Makes group `id` the one the edit controls act on (what a row click
    /// does, without selecting it).
    pub fn set_active(&mut self, id: Option<u32>) {
        self.active = id;
    }

    /// Replaces the filter's clauses (the chips' state), without applying
    /// them.
    pub fn set_clauses(&mut self, clauses: Vec<String>) {
        self.clauses = clauses;
    }
}

/// A selection the panel asked for: the bodies matching every clause
/// (`body.where`), or none when the list is empty. Applied by
/// [`apply_groups_filters`].
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct GroupsFilter {
    /// `body.where` clauses; empty clears the selection.
    pub clauses: Vec<String>,
}

/// What the renderer recorded this frame, for the host to write.
#[derive(Debug, Default)]
pub struct GroupsOutput {
    /// Operations to invoke, in order.
    pub invoke: Vec<InvokeOp>,
    /// A selection to apply, if one was asked for.
    pub filter: Option<GroupsFilter>,
}

/// What the panel needs to know about one body.
#[derive(Debug, Clone)]
pub struct BodyFacts {
    /// The body.
    pub id: StableId,
    /// Its group stack, innermost first.
    pub groups: Vec<u32>,
    /// Its fill.
    pub fill: Rgba,
    /// Its shape kind (`ShapeDef::kind_label`).
    pub kind: &'static str,
}

/// One group row.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupRow {
    /// The group's id.
    pub id: u32,
    /// Its position in id order: the index the `group.*` operations take.
    pub index: usize,
    /// Its name, or `""`.
    pub name: String,
    /// Members at any depth.
    pub count: usize,
    /// Nesting depth (0 = outermost).
    pub depth: usize,
    /// The first member's fill, for the swatch.
    pub fill: Rgba,
}

impl GroupRow {
    /// The text a row shows: the name, else `Group <index>`.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            format!("Group {}", self.index)
        } else {
            self.name.clone()
        }
    }
}

/// The panel's display snapshot, built by the host each frame so the
/// renderer holds no queries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupsModel {
    /// The groups, in tree order (a parent before its children, siblings
    /// in id order).
    pub rows: Vec<GroupRow>,
    /// The shape kinds present, sorted.
    pub kinds: Vec<&'static str>,
    /// The palette slots some body's fill is, ascending.
    pub palette: Vec<usize>,
    /// How many bodies are selected.
    pub selected: usize,
}

impl GroupsModel {
    /// Builds the model from every body and the group names. Pure.
    pub fn build(mut bodies: Vec<BodyFacts>, names: &GroupNames, selected: usize) -> Self {
        // Id order, as the operations' snapshot has it.
        bodies.sort_by_key(|b| b.id.0);
        let index = GroupIndex::build(bodies.iter().map(|b| (b.id, b.groups.as_slice())));
        let fill_of = |id: StableId| {
            bodies
                .iter()
                .find(|b| b.id == id)
                .map_or_else(unpainted, |b| b.fill)
        };
        let row = |i: usize, depth: usize| {
            let g = &index.groups[i];
            GroupRow {
                id: g.id,
                index: i,
                name: names.get(g.id).unwrap_or_default().to_owned(),
                count: g.members.len(),
                depth,
                fill: g.members.first().map_or_else(unpainted, |m| fill_of(*m)),
            }
        };
        // Depth-first from the roots; `seen` guards against a malformed
        // stack naming a cycle.
        let mut rows = Vec::with_capacity(index.len());
        let mut seen = vec![false; index.len()];
        let mut stack: Vec<(usize, usize)> = index
            .groups
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, g)| g.parent.is_none_or(|p| index.get(p).is_none()))
            .map(|(i, _)| (i, 0))
            .collect();
        while let Some((i, depth)) = stack.pop() {
            if std::mem::replace(&mut seen[i], true) {
                continue;
            }
            rows.push(row(i, depth));
            let id = index.groups[i].id;
            for (child, g) in index.groups.iter().enumerate().rev() {
                if g.parent == Some(id) {
                    stack.push((child, depth + 1));
                }
            }
        }
        // Anything a cycle hid still gets a row.
        for (i, done) in seen.iter().enumerate() {
            if !done {
                rows.push(row(i, 0));
            }
        }
        let mut kinds: Vec<&'static str> = bodies.iter().map(|b| b.kind).collect();
        kinds.sort_unstable();
        kinds.dedup();
        let mut palette: Vec<usize> = bodies.iter().filter_map(|b| palette_slot(b.fill)).collect();
        palette.sort_unstable();
        palette.dedup();
        Self {
            rows,
            kinds,
            palette,
            selected,
        }
    }

    fn row(&self, id: u32) -> Option<&GroupRow> {
        self.rows.iter().find(|r| r.id == id)
    }
}

/// Publishes a widget under `groups:<name>` for `ui.inspect` / `ui.click`.
fn publish(name: &str, response: &egui::Response) {
    crate::widget_registry::publish_target(&format!("groups:{name}"), response.rect);
}

/// Renders the panel into `ui` and records what was asked into `out`.
pub fn groups_section(
    ui: &mut egui::Ui,
    model: &GroupsModel,
    panel: &mut GroupsPanel,
    out: &mut GroupsOutput,
) {
    rows_section(ui, model, panel, out);
    ui.separator();
    match panel.active.and_then(|id| model.row(id)) {
        Some(row) => edit_section(ui, row, panel, out),
        None => {
            widgets::hint(ui, "Click a group to edit it.");
        }
    }
    ui.separator();
    filter_section(ui, model, panel, out);
}

/// The group rows: click selects, double-click renames.
fn rows_section(
    ui: &mut egui::Ui,
    model: &GroupsModel,
    panel: &mut GroupsPanel,
    out: &mut GroupsOutput,
) {
    widgets::section_header(ui, &format!("Groups ({})", model.rows.len()));
    if model.rows.is_empty() {
        widgets::empty_state(
            ui,
            "No groups yet: select bodies (or filter below) and press Group selection.",
        );
        return;
    }
    for row in &model.rows {
        ui.horizontal(|ui| {
            ui.add_space(row.depth as f32 * 12.0);
            swatch(ui, row.fill, egui::vec2(10.0, 10.0));
            if let Some((id, text)) = panel.renaming.as_mut()
                && *id == row.id
            {
                let edit = ui.add(egui::TextEdit::singleline(text).desired_width(120.0));
                publish("rename", &edit);
                if !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                if edit.lost_focus() {
                    let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let text = text.trim().to_owned();
                    if !cancelled && text != row.name {
                        out.invoke.push(InvokeOp::with(
                            "group.rename",
                            json!({"group": row.index, "name": text}),
                        ));
                    }
                    panel.renaming = None;
                }
                return;
            }
            let label = row.label();
            let text = if row.name.is_empty() {
                egui::RichText::new(&label).italics()
            } else {
                egui::RichText::new(&label)
            };
            let response = ui
                .selectable_label(panel.active == Some(row.id), text)
                .on_hover_text("Click to select the group; double-click to rename it.");
            publish(&format!("row:{}", row.index), &response);
            if response.double_clicked() {
                panel.renaming = Some((row.id, row.name.clone()));
            } else if response.clicked() {
                panel.active = Some(row.id);
                panel.clauses.clear();
                out.filter = Some(GroupsFilter {
                    clauses: vec![format!("group:{}", row.index)],
                });
            }
            ui.label(egui::RichText::new(format!("({})", row.count)).weak());
        });
    }
}

/// The edit controls for the active group.
fn edit_section(
    ui: &mut egui::Ui,
    row: &GroupRow,
    panel: &mut GroupsPanel,
    out: &mut GroupsOutput,
) {
    let group = row.index;
    ui.horizontal(|ui| {
        widgets::section_header(ui, &format!("Edit {}", row.label()));
        // The same as a double-click on the row, for a pointer that cannot
        // double-click (an injected one, a slow frame).
        let rename = ui.small_button("Rename");
        publish("rename-button", &rename);
        if rename.clicked() {
            panel.renaming = Some((row.id, row.name.clone()));
        }
    });
    colour_row(ui, group, out);

    // Set one property on every member.
    egui::Grid::new("groups-edit-grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Property");
            prop_picker(ui, "groups-prop", &mut panel.prop, "prop");
            ui.end_row();

            let row_prop = &BODY_PROPS[panel.prop.min(BODY_PROPS.len() - 1)];
            ui.label("Set");
            ui.horizontal(|ui| {
                if row_prop.prop.is_flag() {
                    let response = ui.checkbox(&mut panel.flag, "on");
                    publish("flag", &response);
                    if response.changed() {
                        out.invoke
                            .push(set(group, row_prop.name, f64::from(u8::from(panel.flag))));
                    }
                } else {
                    let id = ui.id().with("groups-value");
                    if let Commit::Done(_, new) =
                        widgets::precise_drag(ui, id, &mut panel.value, 0.0, 0.01)
                    {
                        out.invoke.push(set(group, row_prop.name, f64::from(new)));
                    }
                    let apply = ui
                        .button("Apply")
                        .on_hover_text("Set this value on every member (one undo step).");
                    publish("apply", &apply);
                    if apply.clicked() {
                        out.invoke
                            .push(set(group, row_prop.name, f64::from(panel.value)));
                    }
                }
            });
            ui.end_row();

            // Distribute: lo..hi over the members.
            ui.label("From");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut panel.lo).speed(0.01));
                ui.label("to");
                ui.add(egui::DragValue::new(&mut panel.hi).speed(0.01));
            });
            ui.end_row();

            ui.label("Mode");
            ui.horizontal(|ui| {
                let linear = ui.radio_value(&mut panel.random, false, "Linear");
                publish("linear", &linear);
                let random = ui.radio_value(&mut panel.random, true, "Random");
                publish("random", &random);
            });
            ui.end_row();

            if panel.random {
                ui.label("Seed");
                ui.add(egui::DragValue::new(&mut panel.seed).speed(1.0));
            } else {
                ui.label("Order by");
                egui::ComboBox::from_id_salt("groups-by")
                    .selected_text(SORT_KEYS[panel.by.min(SORT_KEYS.len() - 1)])
                    .show_ui(ui, |ui| {
                        for (i, key) in SORT_KEYS.iter().enumerate() {
                            ui.selectable_value(&mut panel.by, i, *key);
                        }
                    });
            }
            ui.end_row();
        });
    let distribute = ui
        .button("Distribute")
        .on_hover_text("Spread the property from the first value to the second (one undo step).");
    publish("distribute", &distribute);
    if distribute.clicked() {
        out.invoke.push(spread(panel, group));
    }

    // Split or dissolve.
    ui.horizontal_wrapped(|ui| {
        ui.label("Split");
        for (key, label) in SPLIT_KEYS {
            let response = ui.button(label);
            publish(&format!("split:{key}"), &response);
            if response.clicked() {
                out.invoke.push(decompose(group, key));
            }
        }
        let response = ui
            .button("Dissolve")
            .on_hover_text("Remove the group; its members (and inner groups) stay.");
        publish("dissolve", &response);
        if response.clicked() {
            out.invoke.push(decompose(group, "dissolve"));
            panel.active = None;
        }
    });
}

/// Colour: one click per palette entry (`group.set_color`).
fn colour_row(ui: &mut egui::Ui, group: usize, out: &mut GroupsOutput) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for entry in &PALETTE {
            let c = entry.color;
            let response = ui
                .add(
                    egui::Button::new("")
                        .fill(crate::inspector::swatch_color32(ui.ctx(), c))
                        .min_size(egui::vec2(16.0, 16.0)),
                )
                .on_hover_text(entry.name);
            let label = format!("colour {}", entry.name);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
            });
            publish(&format!("colour:{}", entry.name), &response);
            if response.clicked() {
                out.invoke.push(InvokeOp::with(
                    "group.set_color",
                    json!({"group": group, "r": c.r, "g": c.g, "b": c.b}),
                ));
            }
        }
    });
}

/// The filter chips and the range builder.
fn filter_section(
    ui: &mut egui::Ui,
    model: &GroupsModel,
    panel: &mut GroupsPanel,
    out: &mut GroupsOutput,
) {
    widgets::section_header(ui, "Filter");
    let before = panel.clauses.clone();
    ui.horizontal_wrapped(|ui| {
        for kind in &model.kinds {
            chip(ui, panel, &format!("kind:{kind}"), kind, None);
        }
        for &slot in &model.palette {
            if let Some(entry) = PALETTE.get(slot) {
                chip(
                    ui,
                    panel,
                    &format!("palette:{slot}"),
                    entry.name,
                    Some(entry.color),
                );
            }
        }
        chip(ui, panel, "grouped", "grouped", None);
        chip(ui, panel, "ungrouped", "ungrouped", None);
    });
    // The range chips added so far, each removable.
    let ranges: Vec<String> = panel
        .clauses
        .iter()
        .filter(|c| c.contains(".."))
        .cloned()
        .collect();
    for clause in ranges {
        ui.horizontal(|ui| {
            ui.label(&clause);
            if widgets::close_button(ui, "Remove this range") {
                panel.clauses.retain(|c| *c != clause);
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        prop_picker(ui, "groups-range-prop", &mut panel.range_prop, "range-prop");
        ui.add(egui::DragValue::new(&mut panel.range_lo).speed(0.01));
        ui.label("..");
        ui.add(egui::DragValue::new(&mut panel.range_hi).speed(0.01));
        let add = ui
            .button("Add range")
            .on_hover_text("Keep bodies whose property is in this range.");
        publish("add-range", &add);
        if add.clicked() {
            let name = BODY_PROPS[panel.range_prop.min(BODY_PROPS.len() - 1)].name;
            let clause = format!("{name}:{}..{}", panel.range_lo, panel.range_hi);
            if !panel.clauses.contains(&clause) {
                panel.clauses.push(clause);
            }
        }
    });
    if panel.clauses != before {
        out.filter = Some(GroupsFilter {
            clauses: panel.clauses.clone(),
        });
    }
    ui.horizontal(|ui| {
        let clear = ui.add_enabled(
            !panel.clauses.is_empty() || model.selected > 0,
            egui::Button::new("Clear"),
        );
        publish("clear", &clear);
        if clear.clicked() {
            panel.clauses.clear();
            out.filter = Some(GroupsFilter {
                clauses: Vec::new(),
            });
        }
        let group = ui
            .add_enabled(model.selected > 1, egui::Button::new("Group selection"))
            .on_hover_text("Group the selected bodies (Ctrl+G).");
        publish("group-selection", &group);
        if group.clicked() {
            out.invoke.extend(key_invocations(EditorAction::Group));
        }
    });
}

/// One toggle chip for `clause`: on while the clause is in the filter.
fn chip(ui: &mut egui::Ui, panel: &mut GroupsPanel, clause: &str, label: &str, fill: Option<Rgba>) {
    let on = panel.clauses.iter().any(|c| c == clause);
    let mut button = egui::Button::new(label).selected(on);
    if let Some(c) = fill {
        // A colour chip keeps its colour either way: dim when off, full and
        // outlined when on.
        let colour = crate::inspector::swatch_color32(ui.ctx(), c);
        button = if on {
            button
                .fill(colour)
                .stroke(egui::Stroke::new(1.5, ui.visuals().strong_text_color()))
        } else {
            button.fill(colour.gamma_multiply(0.45))
        };
    }
    let response = ui.add(button);
    publish(&format!("chip:{label}"), &response);
    if response.clicked() {
        if on {
            panel.clauses.retain(|c| c != clause);
        } else {
            panel.clauses.push(clause.to_owned());
        }
    }
}

/// The property picker over `BODY_PROPS` (name, doc as tooltip).
fn prop_picker(ui: &mut egui::Ui, salt: &str, prop: &mut usize, name: &str) {
    let current = &BODY_PROPS[(*prop).min(BODY_PROPS.len() - 1)];
    let response = egui::ComboBox::from_id_salt(salt)
        .selected_text(current.name)
        .show_ui(ui, |ui| {
            for (i, row) in BODY_PROPS.iter().enumerate() {
                let item = ui
                    .selectable_value(prop, i, row.name)
                    .on_hover_text(row.doc);
                publish(&format!("{name}:{}", row.name), &item);
            }
        })
        .response
        .on_hover_text(current.doc);
    publish(name, &response);
}

/// The fill of a body with no `Appearance` (as the canvas draws it).
fn unpainted() -> Rgba {
    gradiance_domain::appearance::Appearance::default().fill
}

/// A small filled square.
fn swatch(ui: &mut egui::Ui, fill: Rgba, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 2.0, crate::inspector::swatch_color32(ui.ctx(), fill));
}

fn set(group: usize, prop: &str, v: f64) -> InvokeOp {
    InvokeOp::with("group.set", json!({"group": group, "prop": prop, "v": v}))
}

fn decompose(group: usize, key: &str) -> InvokeOp {
    InvokeOp::with("group.decompose", json!({"group": group, "key": key}))
}

fn spread(panel: &GroupsPanel, group: usize) -> InvokeOp {
    let prop = BODY_PROPS[panel.prop.min(BODY_PROPS.len() - 1)].name;
    let (lo, hi) = (f64::from(panel.lo), f64::from(panel.hi));
    if panel.random {
        InvokeOp::with(
            "group.scatter",
            json!({"group": group, "prop": prop, "lo": lo, "hi": hi, "seed": panel.seed}),
        )
    } else {
        let by = SORT_KEYS[panel.by.min(SORT_KEYS.len() - 1)];
        InvokeOp::with(
            "group.distribute",
            json!({"group": group, "prop": prop, "lo": lo, "hi": hi, "by": by}),
        )
    }
}

/// Builds the model from the scene (the dock's read).
pub(crate) fn build_model(
    bodies: &Query<
        (
            &StableId,
            &gradiance_domain::shape::ShapeDef,
            Option<&SelectionGroup>,
            Option<&gradiance_domain::appearance::Appearance>,
        ),
        With<Body>,
    >,
    names: &GroupNames,
    selected: usize,
) -> GroupsModel {
    let facts = bodies
        .iter()
        .map(|(id, shape, groups, appearance)| BodyFacts {
            id: *id,
            groups: groups.map_or_default(|g| g.0.clone()),
            fill: appearance.map_or_else(unpainted, |a| a.fill),
            kind: shape.kind_label(),
        })
        .collect();
    GroupsModel::build(facts, names, selected)
}

/// Installs the panel's headless half: the filter message and the system
/// that turns it into a selection.
pub(crate) fn install(app: &mut App) {
    app.add_message::<GroupsFilter>();
    app.add_systems(Update, apply_groups_filters);
}

/// Applies the latest [`GroupsFilter`]: runs `body.where` against a fresh
/// operation context (committed state) and sets the selection to the
/// matching bodies through [`SelectTransition`]. An empty clause list clears
/// the selection. A bad clause is logged and changes nothing.
pub fn apply_groups_filters(world: &mut World) {
    let Some(request) = gradiance_core::messages::drain::<GroupsFilter>(world).pop() else {
        return;
    };
    let ids = if request.clauses.is_empty() {
        Vec::new()
    } else {
        match matching_ids(world, &request.clauses) {
            Ok(ids) => ids,
            Err(err) => {
                warn!("groups filter: {err}");
                return;
            }
        }
    };
    if let Err(err) = world.run_system_cached_with(select_ids, ids) {
        warn!("groups filter: {err}");
    }
}

/// The ids of the bodies `body.where` returns for `clauses`.
fn matching_ids(world: &mut World, clauses: &[String]) -> Result<Vec<StableId>, String> {
    let entry = world
        .get_resource::<OpRegistry>()
        .and_then(|r| r.get("body.where").cloned())
        .ok_or("body.where is not registered")?;
    let ctx = OpContext::build(world);
    let output = entry
        .lower(json!({ "clauses": clauses }), &ctx)
        .map_err(|e| e.to_string())?;
    let scene = ctx
        .get::<gradiance_command::snapshot::SceneSnapshot>()
        .map_err(|e| e.to_string())?;
    let indices = output
        .value()
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(indices
        .iter()
        .filter_map(gradiance_op::Value::as_u64)
        .filter_map(|i| usize::try_from(i).ok())
        .filter_map(|i| scene.bodies.get(i).map(|b| b.id))
        .collect())
}

/// Sets the body selection to exactly `ids` (a query result, not
/// group-expanded), or clears it.
fn select_ids(
    In(ids): In<Vec<StableId>>,
    mut selection: ResMut<Selection>,
    mut joint: ResMut<SelectedJoint>,
    bodies: Query<(Entity, &StableId), With<Body>>,
    groups: Query<(Entity, &SelectionGroup), With<Body>>,
) {
    let hits: Vec<Entity> = bodies
        .iter()
        .filter(|(_, id)| ids.contains(id))
        .map(|(e, _)| e)
        .collect();
    let transition = if hits.is_empty() {
        SelectTransition::Clear
    } else {
        SelectTransition::SetExactly(hits)
    };
    transition.apply(&mut selection, &mut joint, &groups);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;
    use gradiance_command::body_prop::BodyProp;

    /// The `n`-th of ten ids, in id order (1-based).
    fn id(n: usize) -> StableId {
        thread_local! {
            static IDS: Vec<StableId> = {
                let mut ids: Vec<StableId> = (0..10).map(|_| StableId::new()).collect();
                ids.sort_by_key(|i| i.0);
                ids
            };
        }
        IDS.with(|ids| ids[n - 1])
    }

    fn body(n: usize, groups: &[u32], slot: usize, kind: &'static str) -> BodyFacts {
        BodyFacts {
            id: id(n),
            groups: groups.to_vec(),
            fill: PALETTE[slot].color,
            kind,
        }
    }

    #[test]
    fn rows_come_in_tree_order_with_depth_names_and_counts() {
        // Group 1 is inside group 2; group 3 stands alone.
        let bodies = vec![
            body(1, &[1, 2], 0, "box"),
            body(2, &[1, 2], 0, "box"),
            body(3, &[2], 4, "circle"),
            body(4, &[3], 7, "box"),
            body(5, &[], 7, "polygon"),
        ];
        let mut names = GroupNames::default();
        names.0.insert(2, "cart".into());
        let model = GroupsModel::build(bodies, &names, 0);
        let order: Vec<(u32, usize, usize)> = model
            .rows
            .iter()
            .map(|r| (r.id, r.depth, r.count))
            .collect();
        assert_eq!(order, [(2, 0, 3), (1, 1, 2), (3, 0, 1)]);
        assert_eq!(model.rows[0].label(), "cart");
        assert_eq!(model.rows[1].label(), "Group 0", "unnamed: by index");
        assert_eq!(model.rows[0].index, 1, "id order index");
        assert_eq!(model.kinds, ["box", "circle", "polygon"]);
        assert_eq!(model.palette, [0, 4, 7]);
    }

    #[test]
    fn a_rename_commits_once_on_enter_with_the_group_index() {
        let model = GroupsModel::build(
            vec![body(1, &[5], 0, "box"), body(2, &[5], 0, "box")],
            &GroupNames::default(),
            0,
        );
        let mut panel = GroupsPanel {
            renaming: Some((5, String::new())),
            ..GroupsPanel::default()
        };
        let mut out = GroupsOutput::default();
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            groups_section(ui, &model, &mut panel, &mut out);
        });
        harness.run();
        harness
            .get_by_role(egui::accesskit::Role::TextInput)
            .type_text("wheels");
        harness.key_press(egui::Key::Enter);
        harness.run();
        drop(harness);
        let renames: Vec<_> = out
            .invoke
            .iter()
            .filter(|op| op.id == "group.rename")
            .map(|op| op.params.clone())
            .collect();
        assert_eq!(renames, [json!({"group": 0, "name": "wheels"})]);
        assert!(panel.renaming.is_none());
    }

    #[test]
    fn a_flag_property_is_a_checkbox_and_a_scalar_is_a_number() {
        assert!(BodyProp::named("static").is_some_and(BodyProp::is_flag));
        assert!(!BodyProp::named("density").is_some_and(BodyProp::is_flag));
    }
}
