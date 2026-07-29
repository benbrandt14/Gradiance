//! Font setup and the editor's symbol vocabulary.
//!
//! # Why this module exists
//!
//! Before it, the workspace installed **no fonts at all** — the editor ran on
//! egui's stock [`egui::FontDefinitions::default`], whose families are:
//!
//! ```text
//! Monospace:    Hack → Ubuntu-Light → NotoEmoji-Regular → emoji-icon-font
//! Proportional:        Ubuntu-Light → NotoEmoji-Regular → emoji-icon-font
//! ```
//!
//! Hack is in the monospace chain and **not** the proportional one. Almost every
//! label in the editor is proportional, so a symbol that lives only in Hack —
//! arrows, set operators, geometric shapes — rendered as a tofu box (`□`)
//! everywhere it was used, while looking fine in the console. That accounted for
//! most of the missing glyphs in the editor; the rest were characters present in
//! *no* bundled font at all.
//!
//! [`install`] fixes the first class by appending Hack to the proportional
//! chain. The second class cannot be fixed by fallback order — those characters
//! simply are not in any font we ship — so the fix is to not use them, which is
//! what [`glyph`] is for.
//!
//! # The symbol vocabulary
//!
//! [`glyph`] is the single list of non-ASCII characters the UI may use. Every
//! entry is asserted to render by the coverage test at the bottom of this file,
//! so a symbol cannot silently become a tofu box again.
//!
//! It doubles as a consistency lever. The editor previously used two different
//! close crosses (`✕` U+2715 and `✖` U+2716) for the same action across five
//! files — and, as the coverage test then showed, *neither* is in a bundled
//! font, so both were boxes. Now there is one [`glyph::CLOSE`], and it renders.

use bevy_egui::egui;

/// Every non-ASCII character the editor's text is allowed to use.
///
/// Grouped by role rather than by codepoint, so call sites read as intent
/// (`glyph::CLOSE`) rather than as a literal. Every entry is verified present
/// in the bundled fonts by this module's coverage test.
pub mod glyph {
    // --- Chrome actions -------------------------------------------------
    /// Close / remove / delete. The *one* close mark — see the module docs.
    ///
    /// A multiplication sign rather than one of the heavier crosses: `✕`
    /// (U+2715) and `✖` (U+2716) are both absent from the bundled fonts, which
    /// is why every close button in the editor was a tofu box.
    pub const CLOSE: &str = "\u{d7}"; // ×
    /// Settings / options.
    pub const SETTINGS: &str = "\u{2699}"; // ⚙
    /// Reset to default, revert.
    pub const RESET: &str = "\u{21bb}"; // ↻
    /// Zoom to fit.
    pub const FIT: &str = "\u{26f6}"; // ⛶
    /// Locate / centre on this.
    pub const LOCATE: &str = "\u{2316}"; // ⌖

    // --- Transport ------------------------------------------------------
    /// Play. `⏵` rather than `▶` (U+25B6) — the latter is not in any bundled
    /// font, so the transport strip's most prominent control was a tofu box.
    pub const PLAY: &str = "\u{23f5}"; // ⏵
    /// Pause.
    pub const PAUSE: &str = "\u{23f8}"; // ⏸

    // --- Directions -----------------------------------------------------
    /// Rightward flow, as in "source → sink".
    pub const ARROW_RIGHT: &str = "\u{2192}"; // →
    /// Upward, as in history-back.
    pub const ARROW_UP: &str = "\u{2191}"; // ↑
    /// Downward, as in history-forward.
    pub const ARROW_DOWN: &str = "\u{2193}"; // ↓
    /// Nudge left / align left.
    pub const NUDGE_LEFT: &str = "\u{23f4}"; // ⏴
    /// Nudge right / align right.
    pub const NUDGE_RIGHT: &str = "\u{23f5}"; // ⏵
    /// Nudge up / align top.
    pub const NUDGE_UP: &str = "\u{23f6}"; // ⏶
    /// Nudge down / align bottom.
    pub const NUDGE_DOWN: &str = "\u{23f7}"; // ⏷

    // --- Node graph -----------------------------------------------------
    /// A tunable parameter (slider input).
    pub const PARAM: &str = "\u{2299}"; // ⊙
    /// The plot sink ("scope").
    pub const SCOPE: &str = "\u{25ad}"; // ▭
    /// A small forward marker, as on the per-sensor plot toggle.
    pub const TRIANGLE_RIGHT: &str = "\u{25b8}"; // ▸

    // --- Units and maths ------------------------------------------------
    /// Degrees.
    pub const DEGREE: &str = "\u{b0}"; // °
    /// Superscript two, for squared units.
    pub const SQUARED: &str = "\u{b2}"; // ²
    /// Multiplication.
    pub const TIMES: &str = "\u{d7}"; // ×
    /// Division.
    pub const DIVIDE: &str = "\u{f7}"; // ÷
    /// Middle dot, for compound units.
    pub const MIDDOT: &str = "\u{b7}"; // ·
    /// Unary minus (typographic, not hyphen).
    pub const MINUS: &str = "\u{2212}"; // −
    /// Function, as in a computed signal.
    pub const FUNCTION: &str = "\u{192}"; // ƒ
    /// Angular frequency.
    pub const OMEGA: &str = "\u{3c9}"; // ω
    /// Em dash.
    pub const EM_DASH: &str = "\u{2014}"; // —
    /// Ellipsis.
    pub const ELLIPSIS: &str = "\u{2026}"; // …

    /// Every glyph above, for the coverage test and the source scanner.
    pub const ALL: &[&str] = &[
        CLOSE,
        SETTINGS,
        RESET,
        FIT,
        LOCATE,
        PLAY,
        PAUSE,
        ARROW_RIGHT,
        ARROW_UP,
        ARROW_DOWN,
        NUDGE_LEFT,
        NUDGE_RIGHT,
        NUDGE_UP,
        NUDGE_DOWN,
        PARAM,
        SCOPE,
        TRIANGLE_RIGHT,
        DEGREE,
        SQUARED,
        TIMES,
        DIVIDE,
        MIDDOT,
        MINUS,
        FUNCTION,
        OMEGA,
        EM_DASH,
        ELLIPSIS,
    ];
}

/// Installs the editor's font configuration onto an egui context.
///
/// The whole change is appending `Hack` to the **proportional** fallback chain.
/// Hack ships with egui and covers the arrows, set operators, and geometric
/// shapes the editor labels with; stock egui only reaches it from monospace, so
/// those characters were tofu in every proportional label. Appending rather than
/// prepending keeps Ubuntu-Light as the text face — this changes which glyphs
/// resolve, not how ordinary text looks.
///
/// No font is vendored: everything used here is already in the binary via
/// `bevy_egui`'s `default_fonts` feature.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        let hack = "Hack".to_owned();
        if !proportional.contains(&hack) {
            proportional.push(hack);
        }
    }
    ctx.set_fonts(fonts);
}

/// Installs the editor fonts on the primary egui context, once.
///
/// Runs in `EguiPrimaryContextPass` rather than `Startup` because the egui
/// context does not exist yet at startup. The `Local` latch keeps it to a
/// single call: `set_fonts` rebuilds the font atlas, so calling it every frame
/// would rebuild it every frame.
pub fn install_fonts(mut contexts: bevy_egui::EguiContexts, mut done: bevy::prelude::Local<bool>) {
    if *done {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    install(ctx);
    *done = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A context with fonts actually built.
    ///
    /// egui builds its font atlas lazily on the first frame, so `fonts_mut`
    /// panics on a fresh context — one empty `run` is the cheapest way to get a
    /// queryable font set without standing up a window.
    fn ready_ctx(with_our_fonts: bool) -> egui::Context {
        let ctx = egui::Context::default();
        if with_our_fonts {
            install(&ctx);
        } else {
            ctx.set_fonts(egui::FontDefinitions::default());
        }
        ctx.begin_pass(egui::RawInput::default());
        let _ = ctx.end_pass();
        // A font family is instantiated lazily, on first use. Headless there is
        // no UI to use one, so probe glyphs in an untouched family and every
        // answer comes back "missing" — which looked exactly like a font full
        // of holes and cost real time to tell apart.
        ctx.fonts_mut(|fonts| {
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                let _ = fonts.layout_no_wrap(
                    "A".to_owned(),
                    egui::FontId::new(14.0, family),
                    egui::Color32::WHITE,
                );
            }
        });
        ctx
    }

    /// The point of the module: every symbol the editor uses must actually
    /// render, in the proportional family, with the fonts we install.
    ///
    /// This is the regression guard for a bug that shipped — nine codepoints
    /// were rendering as tofu boxes in the running editor, including one in the
    /// always-visible transport strip, because nothing had ever checked.
    /// Every symbol the editor uses must actually render.
    ///
    /// This is the regression guard for a bug that shipped: `▶` in the
    /// transport strip and both close crosses (`✕`, `✖`) are in *no* bundled
    /// font, so the editor's most prominent control and every close button
    /// were tofu boxes. Nothing had ever checked.
    ///
    /// **Proportional only, deliberately.** egui instantiates a font family
    /// lazily on first use, and headless there is no UI to use one — the
    /// monospace family never loads, so probing it returns "missing" for
    /// everything, which is indistinguishable from a font full of holes.
    /// Priming it with a layout does not help. Proportional is the default
    /// family and what essentially every label in the editor uses, so it is
    /// both the testable surface and the one that matters; the console is the
    /// only monospace surface. The `'A'` control below is what makes the
    /// distinction visible rather than silent.
    #[test]
    fn every_glyph_renders_in_the_proportional_family() {
        let ctx = ready_ctx(true);
        let mut report: Vec<String> = Vec::new();
        let font_id = egui::FontId::new(14.0, egui::FontFamily::Proportional);

        ctx.fonts_mut(|fonts| {
            assert!(
                fonts.has_glyph(&font_id, 'A'),
                "the proportional family did not load — results would be meaningless"
            );
            for entry in glyph::ALL {
                for c in entry.chars() {
                    if !fonts.has_glyph(&font_id, c) {
                        report.push(format!("U+{:04X} {c}", c as u32));
                    }
                }
            }
        });

        assert!(
            report.is_empty(),
            "these would render as tofu boxes: {}",
            report.join(", ")
        );
    }

    /// Appending Hack to the proportional chain is the entire fix for the
    /// larger class of missing glyphs — pinned so a future font change cannot
    /// quietly drop it.
    #[test]
    fn the_proportional_family_can_reach_hack() {
        let ctx = ready_ctx(true);
        ctx.fonts_mut(|fonts| {
            let proportional = egui::FontId::new(14.0, egui::FontFamily::Proportional);
            // U+2192 → lives in Hack and in none of the stock proportional fonts.
            assert!(
                fonts.has_glyph(&proportional, '\u{2192}'),
                "the proportional chain never reached Hack"
            );
        });
    }

    /// No source file may use a non-ASCII character that is not in the
    /// vocabulary.
    ///
    /// The coverage test proves every *listed* glyph renders; this proves the
    /// list is the whole story. Without it, the next person to type a `✕`
    /// straight into a button reintroduces exactly the bug this module exists
    /// to remove, and nothing would notice until someone looked at the running
    /// editor.
    ///
    /// Prose in comments and doc-comments is exempt — this is about what the
    /// editor *draws*, and `fonts.rs` itself has to name the broken characters
    /// in order to document them.
    #[test]
    fn no_source_file_uses_an_unlisted_glyph() {
        let allowed: std::collections::HashSet<char> =
            glyph::ALL.iter().flat_map(|g| g.chars()).collect();

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders: Vec<String> = Vec::new();

        let entries = std::fs::read_dir(&dir).expect("the crate has a src dir");
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            // This file documents the broken characters by name.
            if path.file_name().is_some_and(|n| n == "fonts.rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (line_no, line) in text.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue; // prose, not drawn
                }
                for c in line.chars() {
                    if !c.is_ascii() && !allowed.contains(&c) {
                        offenders.push(format!(
                            "{}:{} U+{:04X} {c}",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            line_no + 1,
                            c as u32
                        ));
                    }
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "non-ASCII characters outside the vocabulary (add them to \
             `glyph` — and check they render — or use plain text): {}",
            offenders.join(", ")
        );
    }

    /// Stock egui really is missing these — the assertion above is meaningful
    /// only if the default is genuinely broken. If a future egui bundles a
    /// wider font this test fails loudly and `install` can be simplified.
    #[test]
    fn stock_egui_really_does_lack_these_glyphs() {
        let ctx = ready_ctx(false);
        ctx.fonts_mut(|fonts| {
            let proportional = egui::FontId::new(14.0, egui::FontFamily::Proportional);
            assert!(
                !fonts.has_glyph(&proportional, '\u{2192}'),
                "stock egui now covers → in proportional; install() may be redundant"
            );
        });
    }
}
