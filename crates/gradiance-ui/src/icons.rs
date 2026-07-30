//! The editor's image-icon registry.
//!
//! # Why images and not glyphs
//!
//! [`crate::fonts::glyph`] covers symbols that are *typographic* — arrows in a
//! sentence, a degree sign, a close mark. It cannot cover pictograms, because
//! the fonts egui bundles simply do not contain them: the coverage test there
//! showed that even `▶` is absent, which is why the Play button was a box.
//!
//! Anything that wants a picture rather than a character belongs here. The tool
//! palette already worked this way ([`crate::toolbar::ToolIcons`]); this module
//! generalises the same mechanism so any panel can use it, rather than each one
//! reinventing the load-and-register dance.
//!
//! # Fallback is not optional
//!
//! Textures need a render device, so headless — every integration test, and CI
//! — the registry is empty. [`icon_button`] therefore always takes a text label
//! and renders it when the image is missing. That is what keeps the panels
//! testable under `egui_kittest`, and it is why the label is a required
//! argument rather than an `Option`.

use bevy::prelude::*;
use bevy_egui::egui;

/// A named editor icon, backed by a PNG in `assets/icons/`.
///
/// The set is deliberately small and curated. `assets/icons/` holds 141 files
/// (an imported Algodoo-style set); listing one here is a statement that the
/// editor actually uses it, so the unused remainder stays visibly unused rather
/// than becoming an undifferentiated grab-bag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// The editor's image icons.
///
/// Deliberately **narrow**. This started with eight variants, chosen from the
/// asset set by guessing which actions wanted pictures; five of them never
/// found a call site, because on contact with the code their natural homes
/// turned out to be places `docs/ui-design.md` assigns to *glyphs* (a close ✕,
/// a zoom-to-fit) or to *words* (context-menu entries, which are read rather
/// than recognised). Decorating menu text to justify an enum variant is
/// backwards, so they were removed.
///
/// The rule, restated: an icon earns its place when the action is **prominent
/// and repeated** — the transport, the tool palette — where a user learns the
/// picture's position and stops reading. Everything else is a glyph or a word.
pub enum Icon {
    /// Start the simulation.
    Play,
    /// Pause the simulation.
    Pause,
    /// Return the camera to the straight-on 2D view.
    HomeView,
}

impl Icon {
    /// Every icon, for loading.
    pub const ALL: [Self; 3] = [Self::Play, Self::Pause, Self::HomeView];

    /// The asset path, relative to `assets/`.
    ///
    /// Several of these have spaces or numeric suffixes because they come from
    /// an imported set and were never renamed; the mapping lives here so no
    /// call site has to know that.
    pub fn asset_path(self) -> &'static str {
        match self {
            Self::Play => "icons/play.png",
            Self::Pause => "icons/pause.png",
            Self::HomeView => "icons/home (2).png",
        }
    }
}

/// Loaded icon textures, empty until [`load_icons`] runs — and always empty
/// headless, where there is no render device.
#[derive(Resource, Default)]
pub struct Icons {
    map: std::collections::HashMap<Icon, egui::TextureId>,
}

impl Icons {
    /// The texture for `icon`, if it loaded.
    pub fn get(&self, icon: Icon) -> Option<egui::TextureId> {
        self.map.get(&icon).copied()
    }

    /// Whether any icons are available (false headless).
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Loads every [`Icon`] and registers it with egui.
///
/// The `Strong` handle is owned by `EguiUserTextures`, which is what keeps the
/// image alive — a weak handle would let the asset be dropped and the button
/// would silently render nothing.
pub fn load_icons(
    asset_server: Res<AssetServer>,
    mut user_textures: ResMut<bevy_egui::EguiUserTextures>,
    mut icons: ResMut<Icons>,
) {
    for icon in Icon::ALL {
        let handle: Handle<Image> = asset_server.load(icon.asset_path());
        let id = user_textures.add_image(bevy_egui::EguiTextureHandle::Strong(handle));
        icons.map.insert(icon, id);
    }
}

/// Default on-screen size for an icon button, in points.
pub const ICON_SIZE: f32 = 16.0;

/// An icon button that degrades to a labelled text button.
///
/// `label` is required rather than optional: it is the headless fallback, and
/// it is also the hover text, so an icon-only button still says what it does.
pub fn icon_button(ui: &mut egui::Ui, icons: &Icons, icon: Icon, label: &str) -> egui::Response {
    match icons.get(icon) {
        Some(id) => ui
            .add(egui::Button::image(egui::load::SizedTexture::new(
                id,
                egui::vec2(ICON_SIZE, ICON_SIZE),
            )))
            .on_hover_text(label),
        None => ui.button(label),
    }
}

/// An icon button with the label always shown beside the image.
///
/// For prominent actions where the picture alone would be a guess — "Pack
/// selection" is not something anyone reads off a pictogram.
pub fn icon_text_button(
    ui: &mut egui::Ui,
    icons: &Icons,
    icon: Icon,
    label: &str,
) -> egui::Response {
    match icons.get(icon) {
        Some(id) => ui.add(
            egui::Button::image_and_text(
                egui::load::SizedTexture::new(id, egui::vec2(ICON_SIZE, ICON_SIZE)),
                label,
            )
            .wrap_mode(egui::TextWrapMode::Extend),
        ),
        None => ui.button(label),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every icon must name a file that exists. A typo'd path fails silently at
    /// runtime — the texture never loads and the button falls back to text,
    /// which looks exactly like running headless.
    #[test]
    fn every_icon_path_exists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .join("assets");
        let missing: Vec<&str> = Icon::ALL
            .iter()
            .map(|i| i.asset_path())
            .filter(|rel| !root.join(rel).exists())
            .collect();
        assert!(missing.is_empty(), "missing icon files: {missing:?}");
    }

    /// Every variant must have a real call site.
    ///
    /// This is the test that was missing. `every_icon_path_exists` iterates
    /// `Icon::ALL`, so a variant nobody draws still compiles and still passes —
    /// the suite was green while five of eight variants were dead. Scanning the
    /// sources is the only way to tell "declared" from "used", the same reason
    /// `fonts::no_source_file_uses_an_unlisted_glyph` exists.
    #[test]
    fn every_icon_variant_is_actually_drawn_somewhere() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut sources = String::new();
        for entry in std::fs::read_dir(&dir)
            .expect("the crate has a src dir")
            .flatten()
        {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            // This file declares the variants; a mention here is not a use.
            if path.file_name().is_some_and(|n| n == "icons.rs") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                sources.push_str(&text);
            }
        }

        let unused: Vec<String> = Icon::ALL
            .iter()
            .map(|icon| format!("{icon:?}"))
            .filter(|name| !sources.contains(&format!("Icon::{name}")))
            .collect();
        assert!(
            unused.is_empty(),
            "these Icon variants are declared but never drawn: {unused:?}\n\
             An icon earns its place only where the action is prominent and \
             repeated — otherwise it is a glyph or a word (docs/ui-design.md). \
             Remove the variant rather than decorating a menu to justify it."
        );
    }

    /// The registry is empty headless, and `icon_button` must cope rather than
    /// panic — this is the path every integration test takes.
    #[test]
    fn an_empty_registry_falls_back_to_text() {
        let icons = Icons::default();
        assert!(icons.is_empty());
        assert!(icons.get(Icon::Play).is_none());
    }
}
