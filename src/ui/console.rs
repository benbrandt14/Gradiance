//! The in-editor **script console**: a lisp REPL / mini code-editor panel.
//!
//! A thin projection over the scripting seam (invariant #4): it never touches
//! authored state. The editor's text is submitted to
//! [`ScriptInputs`](crate::script::bridge::ScriptInputs) — the same input queue
//! a `--script` file or a test uses — and the exclusive `run_scripts` system
//! dispatches it through the intent bus. The output pane just displays
//! [`ScriptLog`](crate::script::bridge::ScriptLog).
//!
//! Syntax highlighting and completion come from `egui_code_editor`, driven by a
//! lisp [`Syntax`] whose keyword set is the registered scene verbs plus the
//! Scheme special forms — so what highlights and completes tracks what the VM
//! actually understands.

use crate::script::bridge::{ScriptInputs, ScriptLog};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui_code_editor::{CodeEditor, ColorTheme, Completer, Syntax};

/// Console panel state (visibility, the editor buffer, and the completion
/// dictionary, which must persist across frames).
#[derive(Resource)]
pub struct ScriptConsole {
    /// Whether the panel is shown.
    open: bool,
    /// The editor buffer.
    input: String,
    /// Auto-completion state, seeded from the lisp syntax + prior words.
    completer: Completer,
}

impl Default for ScriptConsole {
    fn default() -> Self {
        Self {
            open: false,
            input: String::new(),
            completer: Completer::new_with_syntax(&lisp_syntax()).with_user_words(),
        }
    }
}

/// The lisp syntax: scene verbs (our builtins) plus Scheme special forms.
/// Highlighting *and* completion read this, so both track the real VM surface.
fn lisp_syntax() -> Syntax {
    Syntax::new("lisp")
        .with_case_sensitive(true)
        .with_comment(";")
        .with_keywords([
            // Gradiance scene verbs (the registered builtins).
            "cut",
            "spawn-box",
            "spawn-circle",
            // Scheme special forms / common bindings.
            "define",
            "lambda",
            "let",
            "let*",
            "letrec",
            "begin",
            "if",
            "cond",
            "case",
            "when",
            "unless",
            "and",
            "or",
            "not",
            "set!",
            "quote",
            "list",
            "map",
            "for-each",
            "do",
            "loop",
        ])
        .with_special(["+", "-", "*", "/", "<", ">", "=", "<=", ">="])
}

/// Renders the script console panel (toggle with the backquote key). Runs only
/// with rendering present (installed by the UI plugin).
pub fn script_console(
    mut contexts: EguiContexts,
    mut console: ResMut<ScriptConsole>,
    mut inputs: ResMut<ScriptInputs>,
    log: Res<ScriptLog>,
    keys: Res<ButtonInput<KeyCode>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    // Toggle with `\`` — but not while typing (so the key reaches the editor).
    if keys.just_pressed(KeyCode::Backquote) && !ctx.egui_wants_keyboard_input() {
        console.open = !console.open;
    }
    if !console.open {
        return Ok(());
    }

    let syntax = lisp_syntax();
    let mut window_open = true;
    let mut submit = false;
    egui::Window::new("Script Console")
        .open(&mut window_open)
        .default_width(460.0)
        .show(ctx, |ui| {
            // Output log — newest at the bottom.
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .auto_shrink([false, true])
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

            // Lisp editor: syntax highlighting + completion from the op registry.
            let console = &mut *console;
            CodeEditor::default()
                .id_source("script_console_editor")
                .with_rows(6)
                .with_fontsize(13.0)
                .with_theme(ColorTheme::GRUVBOX)
                .with_numlines(true)
                .show_with_completer(ui, &mut console.input, &syntax, &mut console.completer);

            ui.horizontal(|ui| {
                submit = ui.button("\u{25b6} Run").clicked();
                if ui.button("Clear input").clicked() {
                    console.input.clear();
                }
            });
        });
    console.open = window_open;

    if submit && !console.input.trim().is_empty() {
        inputs.submit(console.input.clone());
    }
    Ok(())
}
