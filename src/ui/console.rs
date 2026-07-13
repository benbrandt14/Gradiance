//! The in-editor **script console**: a lisp REPL with MATLAB-style keys.
//!
//! A thin projection over the scripting seam (invariant #4): it never touches
//! authored state. The editor's text is submitted to [`ScriptInputs`] — the
//! same input queue a `--script` file or a test uses — and the exclusive
//! `run_scripts` system dispatches it through the intent bus. The output pane
//! just displays [`ScriptLog`] (which echoes each run's value, bound to `ans`).
//!
//! REPL keys: **Enter executes**, **Shift+Enter** inserts a newline, and
//! **Up/Down** walk the history filtered by the typed prefix (MATLAB style;
//! active while the buffer is a single line). Long scripts belong in a file:
//! `--script foo.scm` hot-reloads on save.
//!
//! Syntax highlighting and the reference panel are driven by the
//! [`OperationRegistry`] — the catalog the VM binds its builtins against — so
//! what highlights and is documented always tracks what the VM understands.

use crate::script::bridge::{OperationRegistry, ScriptInputs, ScriptLog};
use bevy_egui::egui;
use std::collections::BTreeSet;
use std::sync::Arc;

/// Scheme special forms / common bindings that are not scene verbs but should
/// still highlight.
const SCHEME_FORMS: &[&str] = &[
    "define", "lambda", "let", "let*", "letrec", "begin", "if", "cond", "case", "when", "unless",
    "and", "or", "not", "set!", "quote", "list", "map", "for-each", "do", "loop", "length",
    "string?", "number?", "ans",
];

/// Console state (visibility, the editor buffer, and the MATLAB-style
/// history walker).
#[derive(bevy::prelude::Resource, Default)]
pub struct ScriptConsole {
    /// Whether the dock section is shown.
    open: bool,
    /// Whether the reference panel is expanded.
    show_reference: bool,
    /// The editor buffer.
    input: String,
    /// Every submitted line, oldest first.
    history: Vec<String>,
    /// Current position while walking history (`None` = live buffer).
    hist_pos: Option<usize>,
    /// The live buffer stashed when the walk began — also the match prefix.
    stash: String,
}

impl ScriptConsole {
    /// Whether the console is currently shown (read by the transport button so
    /// it can reflect the open state).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Flips the console's visibility. Both the backquote shortcut and the
    /// transport button toggle through here.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Steps through history entries starting with the stashed prefix
    /// (`dir` −1 = older, +1 = newer). Returns the buffer to show.
    fn walk_history(&mut self, dir: i32) -> Option<String> {
        if self.hist_pos.is_none() {
            if dir > 0 {
                return None; // nothing newer than the live buffer
            }
            self.stash = self.input.clone();
        }
        let matches: Vec<usize> = (0..self.history.len())
            .filter(|&i| self.history[i].starts_with(&self.stash))
            .collect();
        let current = self.hist_pos.unwrap_or(self.history.len());
        if dir < 0 {
            let prev = matches.iter().rev().find(|&&i| i < current).copied()?;
            self.hist_pos = Some(prev);
            Some(self.history[prev].clone())
        } else if let Some(next) = matches.iter().find(|&&i| i > current).copied() {
            self.hist_pos = Some(next);
            Some(self.history[next].clone())
        } else {
            // Walked past the newest entry: back to the live buffer.
            self.hist_pos = None;
            Some(self.stash.clone())
        }
    }

    /// Records a submitted line (consecutive duplicates collapse) and resets
    /// the walker.
    fn remember(&mut self, line: &str) {
        if self.history.last().map(String::as_str) != Some(line) {
            self.history.push(line.to_owned());
        }
        self.hist_pos = None;
        self.stash.clear();
    }
}

/// A lightweight lisp highlighter over the live op catalog: verbs and forms
/// in blue, numbers in orange, comments dim green, parens dim.
fn highlight_lisp(
    ui: &egui::Ui,
    text: &str,
    wrap_width: f32,
    verbs: &BTreeSet<&str>,
) -> Arc<egui::Galley> {
    use egui::text::{LayoutJob, TextFormat};
    let font = egui::FontId::monospace(13.0);
    let color_default = ui.visuals().text_color();
    let color_verb = egui::Color32::from_rgb(120, 170, 240);
    let color_num = egui::Color32::from_rgb(230, 165, 100);
    let color_comment = egui::Color32::from_rgb(120, 150, 110);
    let color_paren = ui.visuals().weak_text_color();
    let mut job = LayoutJob::default();
    let fmt = |color: egui::Color32| TextFormat::simple(font.clone(), color);
    for line in text.split_inclusive('\n') {
        if let Some(at) = line.find(';') {
            let (code, comment) = line.split_at(at);
            highlight_code(
                &mut job,
                code,
                verbs,
                &fmt,
                color_default,
                color_verb,
                color_num,
                color_paren,
            );
            job.append(comment, 0.0, fmt(color_comment));
        } else {
            highlight_code(
                &mut job,
                line,
                verbs,
                &fmt,
                color_default,
                color_verb,
                color_num,
                color_paren,
            );
        }
    }
    job.wrap.max_width = wrap_width;
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// Appends one comment-free code span token by token.
#[expect(clippy::too_many_arguments)] // internal helper of the highlighter
fn highlight_code(
    job: &mut egui::text::LayoutJob,
    code: &str,
    verbs: &BTreeSet<&str>,
    fmt: &impl Fn(egui::Color32) -> egui::text::TextFormat,
    color_default: egui::Color32,
    color_verb: egui::Color32,
    color_num: egui::Color32,
    color_paren: egui::Color32,
) {
    let mut token = String::new();
    let flush = |job: &mut egui::text::LayoutJob, token: &mut String| {
        if token.is_empty() {
            return;
        }
        let color = if verbs.contains(token.as_str()) {
            color_verb
        } else if token.parse::<f64>().is_ok() {
            color_num
        } else {
            color_default
        };
        job.append(token, 0.0, fmt(color));
        token.clear();
    };
    for ch in code.chars() {
        if ch == '(' || ch == ')' {
            flush(job, &mut token);
            job.append(&ch.to_string(), 0.0, fmt(color_paren));
        } else if ch.is_whitespace() {
            flush(job, &mut token);
            job.append(&ch.to_string(), 0.0, fmt(color_default));
        } else {
            token.push(ch);
        }
    }
    flush(job, &mut token);
}

/// Renders the console section inside the right dock. Key handling happens
/// *before* the text editor sees input, so Enter can mean "run".
#[expect(clippy::too_many_lines)] // one section, drawn top to bottom
pub fn console_section(
    ui: &mut egui::Ui,
    console: &mut ScriptConsole,
    inputs: &mut ScriptInputs,
    registry: &OperationRegistry,
    log: &ScriptLog,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Script").strong())
            .on_hover_text("Enter runs · Shift+Enter newline · ↑/↓ history (by prefix)");
        ui.checkbox(&mut console.show_reference, "Reference");
        if ui.button("✕").clicked() {
            console.open = false;
        }
    });

    // Reference panel: every registered op, straight from the catalog.
    if console.show_reference {
        egui::ScrollArea::vertical()
            .id_salt("script_console_reference")
            .max_height(150.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for op in registry.0.ops() {
                    ui.horizontal(|ui| {
                        ui.monospace(op.signature);
                        ui.weak(format!("[{}]", op.category.label()));
                    });
                    ui.label(egui::RichText::new(op.doc).weak().small());
                }
            });
        ui.separator();
    }

    // REPL keys, consumed before the editor runs so it never sees them.
    let editor_id = egui::Id::new("script-repl-input");
    let focused = ui.ctx().memory(|m| m.has_focus(editor_id));
    let single_line = !console.input.contains('\n');
    let (mut submit, mut hist_dir) = (false, 0i32);
    if focused {
        ui.ctx().input_mut(|i| {
            submit = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
            if single_line {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    hist_dir = -1;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    hist_dir = 1;
                }
            }
        });
    }
    if hist_dir != 0
        && let Some(recalled) = console.walk_history(hist_dir)
    {
        console.input = recalled;
        // Cursor to the end of the recalled line.
        if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), editor_id) {
            let end = egui::text::CCursor::new(console.input.chars().count());
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(end)));
            state.store(ui.ctx(), editor_id);
        }
    }

    // Output log — newest at the bottom, echoing each run's value (`ans`).
    let editor_height = 84.0;
    let log_height = (ui.available_height() - editor_height).max(60.0);
    egui::ScrollArea::vertical()
        .id_salt("script_console_log")
        .max_height(log_height)
        .min_scrolled_height(log_height)
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for entry in &log.0 {
                ui.monospace(format!("\u{25b8} {}", entry.input));
                let color = if entry.ok {
                    egui::Color32::from_rgb(120, 200, 120)
                } else {
                    egui::Color32::from_rgb(230, 130, 130)
                };
                ui.colored_label(color, &entry.output);
            }
        });
    ui.separator();

    let verbs: BTreeSet<&str> = registry
        .0
        .names()
        .chain(SCHEME_FORMS.iter().copied())
        .collect();
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
        highlight_lisp(ui, text.as_str(), wrap, &verbs)
    };
    let response = ui.add(
        egui::TextEdit::multiline(&mut console.input)
            .id(editor_id)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .font(egui::FontId::monospace(13.0))
            .hint_text("(spawn-box 0 200 40 40)")
            .layouter(&mut layouter),
    );

    if submit && !console.input.trim().is_empty() {
        let line = console.input.trim_end().to_owned();
        console.remember(&line);
        inputs.submit(line);
        console.input.clear();
        response.request_focus();
    }
    ui.horizontal(|ui| {
        if ui.button("\u{25b6} Run").clicked() && !console.input.trim().is_empty() {
            let line = console.input.trim_end().to_owned();
            console.remember(&line);
            inputs.submit(line);
            console.input.clear();
        }
        if ui.button("Clear input").clicked() {
            console.input.clear();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console_with_history(entries: &[&str]) -> ScriptConsole {
        let mut c = ScriptConsole::default();
        for e in entries {
            c.remember(e);
        }
        c
    }

    #[test]
    fn up_walks_backward_filtered_by_prefix() {
        let mut c = console_with_history(&["(body-count)", "(spawn-box 0 0 1 1)", "(body-x 0)"]);
        c.input = "(body".into();
        assert_eq!(c.walk_history(-1).as_deref(), Some("(body-x 0)"));
        assert_eq!(c.walk_history(-1).as_deref(), Some("(body-count)"));
        assert_eq!(c.walk_history(-1), None, "no older match");
    }

    #[test]
    fn down_returns_to_the_stashed_live_buffer() {
        let mut c = console_with_history(&["(a)", "(b)"]);
        c.input = "(".into();
        assert_eq!(c.walk_history(-1).as_deref(), Some("(b)"));
        assert_eq!(c.walk_history(-1).as_deref(), Some("(a)"));
        assert_eq!(c.walk_history(1).as_deref(), Some("(b)"));
        assert_eq!(
            c.walk_history(1).as_deref(),
            Some("("),
            "walking past the newest restores what was being typed"
        );
        assert!(c.hist_pos.is_none());
    }

    #[test]
    fn consecutive_duplicates_collapse() {
        let c = console_with_history(&["(a)", "(a)", "(b)", "(a)"]);
        assert_eq!(c.history, vec!["(a)", "(b)", "(a)"]);
    }
}
