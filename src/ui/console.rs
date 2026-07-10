//! The in-editor **script console**: a lisp REPL / mini code-editor panel.
//!
//! A thin projection over the scripting seam (invariant #4): it never touches
//! authored state. The editor's text is submitted to
//! [`ScriptInputs`](crate::script::bridge::ScriptInputs) — the same input queue
//! a `--script` file or a test uses — and the exclusive `run_scripts` system
//! dispatches it through the intent bus. The output pane just displays
//! [`ScriptLog`](crate::script::bridge::ScriptLog).
//!
//! Syntax highlighting, completion, *and* the reference panel are all driven by
//! the [`OperationRegistry`](crate::script::bridge::OperationRegistry) — the
//! same catalog the VM binds its builtins against — so what highlights,
//! completes, and is documented always tracks what the VM actually understands.

use crate::script::bridge::{OperationRegistry, ScriptInputs, ScriptLog};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui_code_editor::{CodeEditor, ColorTheme, Completer, Syntax};
use std::collections::BTreeSet;

/// Scheme special forms / common bindings that are not scene verbs but should
/// still highlight and complete.
const SCHEME_FORMS: &[&str] = &[
    "define", "lambda", "let", "let*", "letrec", "begin", "if", "cond", "case", "when", "unless",
    "and", "or", "not", "set!", "quote", "list", "map", "for-each", "do", "loop", "length",
    "string?", "number?",
];

/// Operators highlighted as "special".
const OPERATORS: &[&str] = &["+", "-", "*", "/", "<", ">", "=", "<=", ">="];

/// Console panel state (visibility, the editor buffer, and the completion
/// dictionary, which must persist across frames).
#[derive(Resource, Default)]
pub struct ScriptConsole {
    /// Whether the panel is shown.
    open: bool,
    /// Whether the reference panel is expanded.
    show_reference: bool,
    /// The editor buffer.
    input: String,
    /// Auto-completion state, seeded from the registry op names on first show
    /// (then it accretes user words). `None` until the first frame so it can be
    /// built from the live [`OperationRegistry`].
    completer: Option<Completer>,
}

/// The lisp syntax for the editor: the registered scene verbs plus the Scheme
/// special forms. Highlighting *and* completion read this, so both track the
/// real VM surface.
fn lisp_syntax(registry: &OperationRegistry) -> Syntax {
    let keywords: BTreeSet<&str> = registry
        .0
        .names()
        .chain(SCHEME_FORMS.iter().copied())
        .collect();
    let special: BTreeSet<&str> = OPERATORS.iter().copied().collect();
    Syntax::new("lisp")
        .with_case_sensitive(true)
        .with_comment(";")
        .with_keywords(keywords)
        .with_special(special)
}

/// Renders the script console panel (toggle with the backquote key). Runs only
/// with rendering present (installed by the UI plugin).
pub fn script_console(
    mut contexts: EguiContexts,
    mut console: ResMut<ScriptConsole>,
    mut inputs: ResMut<ScriptInputs>,
    registry: Res<OperationRegistry>,
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

    let syntax = lisp_syntax(&registry);
    let mut window_open = true;
    let mut submit = false;
    egui::Window::new("Script Console")
        .open(&mut window_open)
        .default_width(480.0)
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

            // Lisp editor: syntax highlighting + completion from the registry.
            let console = &mut *console;
            let completer = console
                .completer
                .get_or_insert_with(|| Completer::new_with_syntax(&syntax).with_user_words());
            CodeEditor::default()
                .id_source("script_console_editor")
                .with_rows(6)
                .with_fontsize(13.0)
                .with_theme(ColorTheme::GRUVBOX)
                .with_numlines(true)
                .show_with_completer(ui, &mut console.input, &syntax, completer);

            ui.horizontal(|ui| {
                submit = ui.button("\u{25b6} Run").clicked();
                if ui.button("Clear input").clicked() {
                    console.input.clear();
                }
                ui.checkbox(&mut console.show_reference, "Reference");
            });

            // Reference panel: every registered op, straight from the catalog.
            if console.show_reference {
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("script_console_reference")
                    .max_height(180.0)
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
            }
        });
    console.open = window_open;

    if submit && !console.input.trim().is_empty() {
        inputs.submit(console.input.clone());
    }
    Ok(())
}
