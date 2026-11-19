//! The status bar (brief U-9): a strip along the window's bottom edge with
//! the active tool, its mouse and key hints, the keys of the two U-9
//! surfaces, and the simulation state and selection count at the right.
//!
//! Text only, in theme tokens. It is a projection: the tool's name and key
//! come from its operation's metadata in the registry (`tool.box` declares
//! menu `Tools/Box` and chord `B`), the gestures from the tool's own hint
//! table (`interaction::tool_ops::hints`). A panel (`status` in
//! [`Panels::named`](crate::menu::Panels::named)), so the View menu and the
//! `panel.*` operations show and hide it; it is on by default.

use crate::fonts::glyph;
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_core::script_tools::ScriptTools;
use gradiance_core::states::{GameState, ToolState};
use gradiance_interaction::selection::{SelectedJoint, Selection};
use gradiance_interaction::tool_ops::{hints, tool_op_id};
use gradiance_op::OpRegistry;

/// Whether the status bar shows. On by default.
#[derive(Resource, Debug, Clone, Copy)]
pub struct StatusBar {
    /// Showing.
    pub open: bool,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self { open: true }
    }
}

crate::impl_panel_toggle!(StatusBar, open);

/// What the bar says this frame: a pure projection, unit-tested.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusLine {
    /// The active tool and its key: `Box (B)`.
    pub tool: String,
    /// The tool's gestures, input then effect.
    pub hints: Vec<(String, String)>,
    /// The right side: sim state, selection.
    pub state: Vec<String>,
}

/// The keys of the surfaces this brief adds, shown after the tool's hints.
const SURFACE_HINTS: &[(&str, &str)] = &[("F3", "search"), ("Q", "tools pie")];

/// Builds the line for the active `tool`.
pub fn status_line(
    registry: &OpRegistry,
    tool: ToolState,
    script_tool: Option<(&str, usize)>,
    playing: bool,
    selected: usize,
    joint_selected: bool,
) -> StatusLine {
    let meta = tool_op_id(tool)
        .and_then(|id| registry.get(id))
        .map(|e| e.meta);
    let name = match (meta, script_tool) {
        (Some(meta), _) => meta
            .menu
            .and_then(|path| path.rsplit('/').next())
            .unwrap_or(meta.lisp)
            .to_owned(),
        (None, Some((name, _))) => name.to_owned(),
        (None, None) => format!("{tool:?}"),
    };
    let tool_text = match meta.and_then(|m| m.chord) {
        Some(chord) => format!("{name} ({chord})"),
        None => name,
    };
    let mut hint_rows: Vec<(String, String)> = hints(tool)
        .iter()
        .map(|(input, effect)| ((*input).to_owned(), (*effect).to_owned()))
        .collect();
    if let Some((_, points)) = script_tool {
        hint_rows.insert(0, ("Points".to_owned(), points.to_string()));
    }
    hint_rows.extend(
        SURFACE_HINTS
            .iter()
            .map(|(input, effect)| ((*input).to_owned(), (*effect).to_owned())),
    );
    let selection = match (selected, joint_selected) {
        (0, true) => "joint selected".to_owned(),
        (0, false) => "nothing selected".to_owned(),
        (1, _) => "1 selected".to_owned(),
        (n, _) => format!("{n} selected"),
    };
    StatusLine {
        tool: tool_text,
        hints: hint_rows,
        state: vec![
            selection,
            if playing { "Playing" } else { "Paused" }.to_owned(),
        ],
    }
}

/// The bar's height in points (one line of small text and its margin).
pub const HEIGHT: f32 = 22.0;

/// Draws `line` into `ui` (a leaf renderer).
pub fn status_bar_ui(ui: &mut egui::Ui, line: &StatusLine) {
    let tokens = crate::theme::tokens(ui.ctx());
    let sep = format!(" {} ", glyph::MIDDOT);
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(
            egui::RichText::new(&line.tool)
                .color(tokens.text_strong)
                .small(),
        );
        for (input, effect) in &line.hints {
            ui.label(egui::RichText::new(&sep).color(tokens.text_weak).small());
            ui.label(egui::RichText::new(input).color(tokens.text).small());
            ui.label(egui::RichText::new(effect).color(tokens.text_weak).small());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (i, text) in line.state.iter().rev().enumerate() {
                if i > 0 {
                    ui.label(egui::RichText::new(&sep).color(tokens.text_weak).small());
                }
                ui.label(egui::RichText::new(text).color(tokens.text).small());
            }
        });
    });
}

/// What the status bar reads, grouped to keep the system's params short.
#[derive(bevy::ecs::system::SystemParam)]
pub struct StatusInputs<'w> {
    bar: Res<'w, StatusBar>,
    registry: Res<'w, OpRegistry>,
    tool: Res<'w, State<ToolState>>,
    game: Res<'w, State<GameState>>,
    selection: Res<'w, Selection>,
    joint: Res<'w, SelectedJoint>,
    script_tools: Res<'w, ScriptTools>,
}

/// Draws the bar along the window's bottom edge and claims its rect, so the
/// docks drawn after it end above it.
pub fn status_bar(
    mut contexts: bevy_egui::EguiContexts,
    inputs: StatusInputs,
    mut panels: ResMut<crate::PanelRects>,
) -> Result {
    if !inputs.bar.open {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let tool = *inputs.tool.get();
    let script = match tool {
        ToolState::Script(i) => inputs
            .script_tools
            .0
            .get(usize::from(i))
            .map(|spec| (spec.name.as_str(), spec.points)),
        _ => None,
    };
    let line = status_line(
        &inputs.registry,
        tool,
        script,
        *inputs.game.get() == GameState::Playing,
        inputs.selection.len(),
        inputs.joint.0.is_some(),
    );
    let mut root = egui::Ui::new(
        ctx.clone(),
        "status-bar-root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    let frame = egui::Frame::side_top_panel(&ctx.global_style())
        .fill(crate::theme::tokens(ctx).header)
        .inner_margin(egui::Margin::symmetric(8, 2));
    let bar = egui::Panel::bottom("status-bar")
        .frame(frame)
        .exact_size(HEIGHT)
        .resizable(false)
        .show(&mut root, |ui| status_bar_ui(ui, &line));
    panels.push(bar.response.rect);
    crate::widget_registry::publish_rect("status", bar.response.rect);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_op::OpPlugin;

    fn registry() -> OpRegistry {
        let mut app = App::new();
        app.add_plugins((bevy::state::app::StatesPlugin, OpPlugin));
        app.init_state::<ToolState>();
        // The tool operations, as the interaction layer registers them.
        gradiance_interaction::tool_ops::install(&mut app);
        app.world().resource::<OpRegistry>().clone()
    }

    #[test]
    fn the_tool_name_and_key_come_from_its_operation() {
        let line = status_line(&registry(), ToolState::Box, None, false, 0, false);
        assert_eq!(line.tool, "Box (B)");
        assert_eq!(line.hints[0], ("Drag".to_owned(), "draw a box".to_owned()));
        assert!(line.hints.iter().any(|(k, _)| k == "F3"));
        assert_eq!(line.state, ["nothing selected", "Paused"]);
        let prismatic = status_line(&registry(), ToolState::Slider, None, true, 3, false);
        assert_eq!(prismatic.tool, "Prismatic (R)");
        assert_eq!(prismatic.state, ["3 selected", "Playing"]);
    }

    #[test]
    fn a_script_tool_shows_its_name_and_points() {
        let line = status_line(
            &registry(),
            ToolState::Script(0),
            Some(("Fence", 3)),
            false,
            0,
            true,
        );
        assert_eq!(line.tool, "Fence");
        assert_eq!(line.hints[0], ("Points".to_owned(), "3".to_owned()));
        assert_eq!(line.state[0], "joint selected");
    }
}
