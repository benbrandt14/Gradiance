//! Chrome theme tokens (brief U-7, `docs/unified-surface-plan.md` § 7.1).
//!
//! The editor's panels draw with **semantic tokens**, never with colours:
//! a panel background, a header, a widget's inner / hovered / outline, text
//! strong / normal / weak, the accent, the selection highlight, warning and
//! error, and the few content inks panels paint themselves (plot series,
//! console syntax, node canvas, the view cube, the on-canvas label plates).
//! [`ChromeTheme::visuals`] turns one token set into `egui::Visuals`;
//! [`tokens`] hands the active set to any panel. This file is the only place
//! in `gradiance-ui` a colour is written as numbers
//! (`tests/boundaries.rs`, `scene_colours_have_one_source`), as
//! `ui::fonts::glyph` is the only place a non-ASCII symbol is.
//!
//! # The default leans to Blender
//!
//! The greys are Blender's default dark theme (4.x, `userdef_default_theme.c`
//! as the U-7 worker knew it; no Blender ran in the container, so these are
//! the shipped values recalled, not sampled from a screen):
//!
//! | Token | Blender source | Value |
//! |---|---|---|
//! | `panel` | region back (Properties) | `#303030` |
//! | `header` | tab strip / top bar | `#1d1d1d` |
//! | `window` | box / tool inner (floating windows, menus) | `#282828` |
//! | `field` | text-field inner (`wcol_text`) | `#1d1d1d` |
//! | `widget` | button inner (`wcol_regular`) | `#545454` |
//! | `widget_hovered` | the hovered inner (Blender shades `widget` up) | `#656565` |
//! | `outline` | button outline (`wcol_regular`) | `#3d3d3d` |
//! | `border` | editor border | `#161616` |
//! | `text` / `text_strong` | widget text / selected text | `#e6e6e6` / `#ffffff` |
//! | selection (`blender-blue` only) | `wcol_regular.inner_sel` | `#4772b3` |
//!
//! Adjusted where Gradiance needs more: `text_weak` (`#9e9e9e`) is the
//! dimmest grey that still reads at 4.5:1 on `panel`; Blender's report
//! colours (warning `#b36a00`, error `#b34040`) are text here, so they keep
//! their hue and are lightened to `#f0a030` / `#ff7070` until they read on
//! `panel`.
//!
//! # The accent is the scene's
//!
//! Gradiance has one accent: the active scene look's
//! [`Ink::accent`](gradiance_domain::theme::Ink), the selection colour on
//! the canvas (brief 4b-3). The chrome reads it from there and never copies
//! it, so a selected row and a selected body are one hue in every look. The
//! selection fill behind white text is that accent darkened just enough to
//! hold 4.5:1 ([`ChromeTheme::new`]); hyperlinks use it lightened until they
//! read on `panel`.
//!
//! # Presets
//!
//! [`Chrome`] names the token sets: `blender` (the default), `blender-blue`
//! (Blender's own blue selection, for comparison) and `stock` (egui's stock
//! dark visuals and the colours the panels used before U-7, so the "before"
//! stays one click away). The preset is workstation config
//! ([`ChromeSettings`]).

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui::Color32;
use gradiance_domain::appearance::Rgba;
use gradiance_domain::settings::{Chrome, ChromeSettings, RenderSettings};
use gradiance_domain::theme::{Look, SceneTheme, contrast_ratio};

/// The contrast floor for normal text (WCAG 2 AA): every text token against
/// every surface it is drawn on.
pub const TEXT_CONTRAST: f32 = 4.5;

/// The contrast floor for marks that are not text (WCAG 2 non-text): plot
/// series against the plot background.
pub const MARK_CONTRAST: f32 = 3.0;

/// The fixed half of a token set: everything but the accent, which comes
/// from the scene look.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Greys {
    panel: Color32,
    header: Color32,
    window: Color32,
    field: Color32,
    widget: Color32,
    widget_hovered: Color32,
    outline: Color32,
    border: Color32,
    text: Color32,
    text_strong: Color32,
    text_weak: Color32,
    /// `None`: the look's accent, darkened behind white text.
    selected: Option<Color32>,
    warning: Color32,
    error: Color32,
    ok: Color32,
    muted: Color32,
    series: [Color32; 4],
    handle: Color32,
    chart_grid: Color32,
    syntax_verb: Color32,
    syntax_number: Color32,
    syntax_comment: Color32,
    canvas: Color32,
    canvas_dot: Color32,
    node: Color32,
    node_header: Color32,
    node_outline: Color32,
    pin_in: Color32,
    pin_out: Color32,
    overlay: Color32,
    overlay_text: Color32,
    cube_label: Color32,
    cube_edge: Color32,
    cube_edge_hovered: Color32,
    /// `None`: the accent, nearly opaque.
    cube_hovered: Option<Color32>,
    dial: Color32,
}

/// The content inks both Blender presets share: plot series and handles,
/// console syntax, node pins (hues kept from before U-7, the comment grey
/// lifted to read at 4.5:1).
const SERIES: [Color32; 4] = [
    Color32::from_rgb(120, 200, 255),
    Color32::from_rgb(255, 190, 120),
    Color32::from_rgb(150, 230, 150),
    Color32::from_rgb(230, 150, 230),
];

/// Blender's default dark theme (see the module docs for each source).
const BLENDER: Greys = Greys {
    panel: Color32::from_rgb(0x30, 0x30, 0x30),
    header: Color32::from_rgb(0x1d, 0x1d, 0x1d),
    window: Color32::from_rgb(0x28, 0x28, 0x28),
    field: Color32::from_rgb(0x1d, 0x1d, 0x1d),
    widget: Color32::from_rgb(0x54, 0x54, 0x54),
    widget_hovered: Color32::from_rgb(0x65, 0x65, 0x65),
    outline: Color32::from_rgb(0x3d, 0x3d, 0x3d),
    border: Color32::from_rgb(0x16, 0x16, 0x16),
    text: Color32::from_rgb(0xe6, 0xe6, 0xe6),
    text_strong: Color32::from_rgb(0xff, 0xff, 0xff),
    text_weak: Color32::from_rgb(0x9e, 0x9e, 0x9e),
    selected: None,
    warning: Color32::from_rgb(0xf0, 0xa0, 0x30),
    error: Color32::from_rgb(0xff, 0x70, 0x70),
    ok: Color32::from_rgb(0x78, 0xc8, 0x78),
    muted: Color32::from_rgb(0x5a, 0x5a, 0x5a),
    series: SERIES,
    handle: Color32::from_rgb(255, 220, 140),
    chart_grid: Color32::from_rgb(0x46, 0x46, 0x46),
    syntax_verb: Color32::from_rgb(120, 170, 240),
    syntax_number: Color32::from_rgb(230, 165, 100),
    syntax_comment: Color32::from_rgb(130, 170, 120),
    canvas: Color32::from_rgb(0x1d, 0x1d, 0x1d),
    canvas_dot: Color32::from_rgba_premultiplied(23, 23, 23, 46),
    node: Color32::from_rgb(0x28, 0x28, 0x28),
    node_header: Color32::from_rgb(0x3d, 0x3d, 0x3d),
    node_outline: Color32::from_rgb(0x16, 0x16, 0x16),
    pin_in: Color32::from_rgb(210, 170, 120),
    pin_out: Color32::from_rgb(140, 220, 150),
    overlay: Color32::from_black_alpha(170),
    overlay_text: Color32::from_rgb(0xe6, 0xe6, 0xe6),
    cube_label: Color32::from_rgb(0x1d, 0x1d, 0x1d),
    cube_edge: Color32::from_rgb(0x9e, 0x9e, 0x9e),
    cube_edge_hovered: Color32::from_rgb(0xe6, 0xe6, 0xe6),
    cube_hovered: None,
    dial: Color32::from_rgb(0x9e, 0x9e, 0x9e),
};

/// Blender's greys with Blender's blue selection (`wcol_regular.inner_sel`).
const BLENDER_BLUE: Greys = Greys {
    selected: Some(Color32::from_rgb(0x47, 0x72, 0xb3)),
    ..BLENDER
};

/// egui 0.36's stock dark visuals (`Visuals::dark`), and the colours the
/// panels wrote in place before U-7. Not held to the contrast floors: it is
/// the comparison, not a design (its weak text is 2.7:1).
const EGUI: Greys = Greys {
    panel: Color32::from_gray(27),
    header: Color32::from_gray(27),
    window: Color32::from_gray(27),
    field: Color32::from_gray(10),
    widget: Color32::from_gray(60),
    widget_hovered: Color32::from_gray(70),
    outline: Color32::from_gray(60),
    border: Color32::from_gray(60),
    text: Color32::from_gray(140),
    text_strong: Color32::from_gray(255),
    text_weak: Color32::from_gray(95),
    selected: Some(Color32::from_rgb(0, 92, 128)),
    warning: Color32::from_rgb(255, 143, 0),
    error: Color32::from_rgb(220, 120, 120),
    ok: Color32::from_rgb(120, 200, 120),
    muted: Color32::from_gray(90),
    series: SERIES,
    handle: Color32::from_rgb(255, 220, 140),
    chart_grid: Color32::from_gray(70),
    syntax_verb: Color32::from_rgb(120, 170, 240),
    syntax_number: Color32::from_rgb(230, 165, 100),
    syntax_comment: Color32::from_rgb(120, 150, 110),
    canvas: Color32::from_gray(18),
    canvas_dot: Color32::from_rgba_premultiplied(23, 23, 23, 46),
    node: Color32::from_gray(30),
    node_header: Color32::from_gray(40),
    node_outline: Color32::from_gray(72),
    pin_in: Color32::from_rgb(210, 170, 120),
    pin_out: Color32::from_rgb(140, 220, 150),
    overlay: Color32::from_black_alpha(170),
    overlay_text: Color32::from_rgb(235, 235, 220),
    cube_label: Color32::from_gray(30),
    cube_edge: Color32::from_gray(130),
    cube_edge_hovered: Color32::from_gray(220),
    cube_hovered: Some(Color32::from_rgba_premultiplied(175, 161, 83, 235)),
    dial: Color32::from_gray(160),
};

/// One resolved token set: what every panel draws with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromeTheme {
    /// The preset these tokens came from.
    pub chrome: Chrome,
    /// The scene look the accent came from (also how a palette swatch is
    /// shown: [`SceneTheme::display`]).
    pub look: &'static SceneTheme,
    /// Dock and side-panel background.
    pub panel: Color32,
    /// The menu bar and the dock tab strips.
    pub header: Color32,
    /// Floating windows, menus and popups.
    pub window: Color32,
    /// Text and number entry, plot and console backgrounds.
    pub field: Color32,
    /// A button's or checkbox's inner.
    pub widget: Color32,
    /// The inner under the pointer.
    pub widget_hovered: Color32,
    /// A widget's outline and separators.
    pub outline: Color32,
    /// The edge of a window.
    pub border: Color32,
    /// Normal text.
    pub text: Color32,
    /// Hovered, pressed and selected text; headings.
    pub text_strong: Color32,
    /// Hints and secondary labels.
    pub text_weak: Color32,
    /// The scene look's accent, as authored (marks, outlines).
    pub accent: Color32,
    /// The accent as text: lightened until it reads on `panel`.
    pub accent_text: Color32,
    /// The selection highlight behind `text_strong`.
    pub selected: Color32,
    /// Warning text.
    pub warning: Color32,
    /// Error text.
    pub error: Color32,
    /// Success text (a console result).
    pub ok: Color32,
    /// An inactive mark (a weightless gravity dial).
    pub muted: Color32,
    /// Plot series, in order; the first is also the curve editor's line.
    pub series: [Color32; 4],
    /// A draggable point on a chart.
    pub handle: Color32,
    /// Chart grid lines.
    pub chart_grid: Color32,
    /// Console: a verb at the head of a form.
    pub syntax_verb: Color32,
    /// Console: a number.
    pub syntax_number: Color32,
    /// Console: a comment.
    pub syntax_comment: Color32,
    /// The node canvas.
    pub canvas: Color32,
    /// The node canvas's dot grid.
    pub canvas_dot: Color32,
    /// A node's body.
    pub node: Color32,
    /// A node's header.
    pub node_header: Color32,
    /// A node's outline.
    pub node_outline: Color32,
    /// An input pin.
    pub pin_in: Color32,
    /// An output pin.
    pub pin_out: Color32,
    /// The plate behind text drawn over the scene (labels, review caption).
    pub overlay: Color32,
    /// Text on [`overlay`](Self::overlay).
    pub overlay_text: Color32,
    /// A view-cube face label.
    pub cube_label: Color32,
    /// A view-cube face edge.
    pub cube_edge: Color32,
    /// The hovered view-cube face's edge.
    pub cube_edge_hovered: Color32,
    /// The hovered view-cube face.
    pub cube_hovered: Color32,
    /// A dial's ring and needle (gravity, the sun gadget).
    pub dial: Color32,
}

impl Default for ChromeTheme {
    fn default() -> Self {
        Self::new(Chrome::default(), Look::default().theme())
    }
}

impl ChromeTheme {
    /// The tokens of `chrome` over the scene look `look`.
    #[must_use]
    pub fn new(chrome: Chrome, look: &'static SceneTheme) -> Self {
        let g = match chrome {
            Chrome::Blender => BLENDER,
            Chrome::BlenderBlue => BLENDER_BLUE,
            Chrome::Stock => EGUI,
        };
        let accent = color32(look.ink.accent);
        let (selected, accent_text) = if chrome == Chrome::Stock {
            // egui's own selection blue and hyperlink colour.
            (
                g.selected.unwrap_or(accent),
                Color32::from_rgb(90, 170, 255),
            )
        } else {
            (
                g.selected
                    .unwrap_or_else(|| legible(accent, g.text_strong, Color32::BLACK)),
                legible(accent, g.panel, Color32::WHITE),
            )
        };
        Self {
            chrome,
            look,
            panel: g.panel,
            header: g.header,
            window: g.window,
            field: g.field,
            widget: g.widget,
            widget_hovered: g.widget_hovered,
            outline: g.outline,
            border: g.border,
            text: g.text,
            text_strong: g.text_strong,
            text_weak: g.text_weak,
            accent,
            accent_text,
            selected,
            warning: g.warning,
            error: g.error,
            ok: g.ok,
            muted: g.muted,
            series: g.series,
            handle: g.handle,
            chart_grid: g.chart_grid,
            syntax_verb: g.syntax_verb,
            syntax_number: g.syntax_number,
            syntax_comment: g.syntax_comment,
            canvas: g.canvas,
            canvas_dot: g.canvas_dot,
            node: g.node,
            node_header: g.node_header,
            node_outline: g.node_outline,
            pin_in: g.pin_in,
            pin_out: g.pin_out,
            overlay: g.overlay,
            overlay_text: g.overlay_text,
            cube_label: g.cube_label,
            cube_edge: g.cube_edge,
            cube_edge_hovered: g.cube_edge_hovered,
            cube_hovered: g.cube_hovered.unwrap_or_else(|| {
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 235)
            }),
            dial: g.dial,
        }
    }

    /// The egui visuals these tokens describe. The `stock` preset is egui's
    /// stock dark visuals, untouched.
    #[must_use]
    pub fn visuals(&self) -> egui::Visuals {
        let mut v = egui::Visuals::dark();
        if self.chrome == Chrome::Stock {
            return v;
        }
        let stroke = |width: f32, color: Color32| egui::Stroke::new(width, color);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = self.panel;
        w.noninteractive.weak_bg_fill = self.panel;
        w.noninteractive.bg_stroke = stroke(1.0, self.outline);
        w.noninteractive.fg_stroke = stroke(1.0, self.text);
        w.inactive.bg_fill = self.widget;
        w.inactive.weak_bg_fill = self.widget;
        w.inactive.bg_stroke = stroke(1.0, self.outline);
        w.inactive.fg_stroke = stroke(1.0, self.text);
        w.hovered.bg_fill = self.widget_hovered;
        w.hovered.weak_bg_fill = self.widget_hovered;
        w.hovered.bg_stroke = stroke(1.0, self.outline);
        w.hovered.fg_stroke = stroke(1.5, self.text_strong);
        w.active.bg_fill = self.selected;
        w.active.weak_bg_fill = self.selected;
        w.active.bg_stroke = stroke(1.0, self.selected);
        w.active.fg_stroke = stroke(2.0, self.text_strong);
        w.open.bg_fill = self.panel;
        w.open.weak_bg_fill = self.widget_hovered;
        w.open.bg_stroke = stroke(1.0, self.outline);
        w.open.fg_stroke = stroke(1.0, self.text);
        v.selection.bg_fill = self.selected;
        v.selection.stroke = stroke(1.0, self.text_strong);
        v.weak_text_color = Some(self.text_weak);
        v.hyperlink_color = self.accent_text;
        v.faint_bg_color = mix(self.panel, self.text_strong, 0.03);
        v.extreme_bg_color = self.field;
        v.text_edit_bg_color = Some(self.field);
        v.code_bg_color = self.field;
        v.warn_fg_color = self.warning;
        v.error_fg_color = self.error;
        v.window_fill = self.window;
        v.window_stroke = stroke(1.0, self.border);
        v.panel_fill = self.panel;
        v
    }
}

/// The token set the editor is drawn with now, as [`install`] left it
/// (the default set before the first install: leaf renderers in tests).
#[must_use]
pub fn tokens(ctx: &egui::Context) -> ChromeTheme {
    ctx.data(|d| d.get_temp::<ChromeTheme>(key()))
        .unwrap_or_default()
}

/// Makes `theme` the context's look: dark mode, its visuals, and the tokens
/// [`tokens`] returns.
pub fn install(ctx: &egui::Context, theme: &ChromeTheme) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, theme.visuals());
    ctx.data_mut(|d| d.insert_temp(key(), *theme));
}

/// Where [`install`] keeps the tokens in the context's memory.
fn key() -> egui::Id {
    egui::Id::new("gradiance-chrome-theme")
}

/// Installs the chrome on the primary context when the preset or the scene
/// look changes (and on the first frame).
pub fn apply_chrome(
    mut contexts: EguiContexts,
    chrome: Option<Res<ChromeSettings>>,
    render: Option<Res<RenderSettings>>,
    mut applied: Local<Option<(Chrome, Look)>>,
) -> Result {
    let want = (
        chrome.map_or_default(|c| c.chrome),
        render.map_or_default(|r| r.look),
    );
    if *applied == Some(want) {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    install(ctx, &ChromeTheme::new(want.0, want.1.theme()));
    *applied = Some(want);
    Ok(())
}

/// A domain colour (sRGB, `[0, 1]`) as an egui colour.
#[must_use]
pub fn color32(c: Rgba) -> Color32 {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(byte(c.r), byte(c.g), byte(c.b), byte(c.a))
}

/// An egui colour as a domain colour (alpha dropped), for the colour maths.
fn rgba(c: Color32) -> Rgba {
    let unit = |v: u8| f32::from(v) / 255.0;
    Rgba::rgb(unit(c.r()), unit(c.g()), unit(c.b()))
}

/// `a` moved a fraction `t` of the way to `b`, per sRGB channel (opaque).
#[must_use]
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

/// The WCAG contrast ratio of two egui colours (alpha ignored).
#[must_use]
pub fn contrast(a: Color32, b: Color32) -> f32 {
    contrast_ratio(rgba(a), rgba(b))
}

/// `colour` moved toward `toward` (black or white) in small steps until it
/// holds [`TEXT_CONTRAST`] (with a little room for rounding) against
/// `against`; `colour` itself when it already does.
fn legible(colour: Color32, against: Color32, toward: Color32) -> Color32 {
    let floor = TEXT_CONTRAST + 0.1;
    (0..=50)
        .map(|step| mix(colour, toward, step as f32 / 50.0))
        .find(|c| contrast(*c, against) >= floor)
        .unwrap_or(toward)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every token set held to the floors: the two Blender presets, over
    /// every scene look (the accent changes with the look).
    fn designed() -> impl Iterator<Item = ChromeTheme> {
        [Chrome::Blender, Chrome::BlenderBlue]
            .into_iter()
            .flat_map(|c| Look::ALL.map(|l| ChromeTheme::new(c, l.theme())))
    }

    fn check(t: &ChromeTheme, pairs: &[(&str, Color32, &str, Color32)], floor: f32) {
        for (fg_name, fg, bg_name, bg) in pairs {
            let ratio = contrast(*fg, *bg);
            assert!(
                ratio >= floor,
                "{} over {}: {fg_name} on {bg_name} is {ratio:.2}:1, below {floor}:1",
                t.chrome.name(),
                t.look.name,
            );
        }
    }

    /// Normal text reads at 4.5:1 (WCAG AA) on every surface it sits on.
    #[test]
    fn text_meets_the_contrast_floor_on_every_surface() {
        for t in designed() {
            let surfaces = [
                ("panel", t.panel),
                ("window", t.window),
                ("field", t.field),
                ("header", t.header),
            ];
            let inks = [
                ("text", t.text),
                ("text_strong", t.text_strong),
                ("text_weak", t.text_weak),
                ("accent_text", t.accent_text),
                ("warning", t.warning),
                ("error", t.error),
                ("ok", t.ok),
                ("syntax_verb", t.syntax_verb),
                ("syntax_number", t.syntax_number),
                ("syntax_comment", t.syntax_comment),
            ];
            let mut pairs = Vec::new();
            for (f, fc) in inks {
                for (b, bc) in surfaces {
                    pairs.push((f, fc, b, bc));
                }
            }
            pairs.extend([
                ("text", t.text, "widget", t.widget),
                (
                    "text_strong",
                    t.text_strong,
                    "widget_hovered",
                    t.widget_hovered,
                ),
                ("text_strong", t.text_strong, "selected", t.selected),
                ("text", t.text, "node", t.node),
                ("text", t.text, "node_header", t.node_header),
            ]);
            check(&t, &pairs, TEXT_CONTRAST);
        }
    }

    /// Text drawn over the scene sits on a translucent plate: it reads over
    /// each look's backdrop, through the plate.
    #[test]
    fn overlay_text_reads_over_every_backdrop() {
        for t in designed() {
            let a = f32::from(t.overlay.a()) / 255.0;
            let plate = mix(color32(t.look.backdrop), Color32::BLACK, a);
            check(
                &t,
                &[("overlay_text", t.overlay_text, "overlay", plate)],
                TEXT_CONTRAST,
            );
        }
    }

    /// Plot series and the curve handle are marks: 3:1 on the plot field.
    #[test]
    fn chart_marks_meet_the_non_text_floor() {
        for t in designed() {
            let mut pairs: Vec<_> = t
                .series
                .iter()
                .map(|s| ("series", *s, "field", t.field))
                .collect();
            pairs.push(("handle", t.handle, "field", t.field));
            check(&t, &pairs, MARK_CONTRAST);
        }
    }

    /// The selection highlight is the scene accent's hue (brief U-7: the
    /// chrome reads it from the look) under `blender`, and Blender's blue
    /// under `blender-blue`.
    #[test]
    fn the_selection_follows_the_scene_accent() {
        for look in Look::ALL {
            let t = ChromeTheme::new(Chrome::Blender, look.theme());
            assert_eq!(t.accent, color32(look.theme().ink.accent));
            // Darkening toward black keeps the channel order (the hue).
            let order = |c: Color32| {
                let mut ch = [(c.r(), 0), (c.g(), 1), (c.b(), 2)];
                ch.sort_unstable();
                ch.map(|(_, i)| i)
            };
            assert_eq!(order(t.selected), order(t.accent), "{}", look.name());
            let blue = ChromeTheme::new(Chrome::BlenderBlue, look.theme());
            assert_eq!(blue.selected, Color32::from_rgb(0x47, 0x72, 0xb3));
        }
    }

    /// The default preset leans to Blender: neutral greys (r = g = b) for
    /// every surface, and low-contrast chrome (no surface pair above 2:1
    /// except a field against a widget).
    #[test]
    fn the_default_greys_are_neutral_and_low_contrast() {
        let t = ChromeTheme::default();
        assert_eq!(t.chrome, Chrome::Blender);
        let surfaces = [
            t.panel,
            t.header,
            t.window,
            t.field,
            t.widget,
            t.widget_hovered,
            t.outline,
            t.border,
        ];
        for s in surfaces {
            assert!(s.r() == s.g() && s.g() == s.b(), "{s:?} is not a grey");
        }
        assert!(contrast(t.panel, t.header) < 2.0);
        assert!(contrast(t.panel, t.window) < 2.0);
        assert!(contrast(t.panel, t.outline) < 2.0);
    }

    /// The visuals carry the tokens; the `stock` preset is egui's own.
    #[test]
    fn visuals_carry_the_tokens() {
        let t = ChromeTheme::default();
        let v = t.visuals();
        assert_eq!(v.panel_fill, t.panel);
        assert_eq!(v.window_fill, t.window);
        assert_eq!(v.selection.bg_fill, t.selected);
        assert_eq!(v.widgets.noninteractive.fg_stroke.color, t.text);
        assert_eq!(v.weak_text_color, Some(t.text_weak));
        assert_eq!(v.extreme_bg_color, t.field);
        let stock = ChromeTheme::new(Chrome::Stock, Look::default().theme());
        assert_eq!(stock.visuals(), egui::Visuals::dark());
    }

    /// `tokens` returns what `install` stored, and the default before.
    #[test]
    fn install_stores_the_tokens() {
        let ctx = egui::Context::default();
        assert_eq!(tokens(&ctx), ChromeTheme::default());
        let blue = ChromeTheme::new(Chrome::BlenderBlue, Look::Blueprint.theme());
        install(&ctx, &blue);
        assert_eq!(tokens(&ctx), blue);
        assert_eq!(ctx.global_style().visuals.panel_fill, blue.panel);
    }

    #[test]
    fn colour_helpers_round_trip() {
        let c = Color32::from_rgb(12, 200, 99);
        assert_eq!(color32(rgba(c)), c);
        assert_eq!(mix(Color32::BLACK, Color32::WHITE, 0.0), Color32::BLACK);
        assert_eq!(mix(Color32::BLACK, Color32::WHITE, 1.0), Color32::WHITE);
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
    }
}
