//! The Scenes panel (brief S-17): the scene files in a folder as a grid of
//! thumbnails with names.
//!
//! Click a tile to open its scene (`scene.load`, one undo step). With the
//! pointer on a tile, F2 renames it (`scene.rename`) and Delete asks, then
//! moves it to the trash folder beside it (`scene.trash`). "Folder" picks
//! the folder (`scene.browse_dir`). Every action is an operation invoked
//! by id, so the panel adds no mutation path; the disk work is the persist
//! layer's ([`gradiance_persist::browser`]).
//!
//! The listing is read from disk when the folder changes and once a second
//! while the panel shows. A thumbnail is decoded once per file change.

use crate::text::Strings;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::scene_folder::SceneFolder;
use gradiance_op::{InvokeOp, json};
use gradiance_persist::browser::{SceneEntry, ScenesFolder, list_scenes};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How often the open panel reads the folder again (seconds).
const REFRESH_S: f64 = 1.0;

/// A tile's thumbnail size in the panel (points, 16:10).
const TILE: egui::Vec2 = egui::vec2(128.0, 80.0);

/// The Scenes panel's state: visibility, the last listing, the decoded
/// thumbnails, and the rename or trash in progress.
#[derive(Resource, Default)]
pub struct ScenesPanel {
    open: bool,
    /// The folder the listing is of.
    listed: Option<PathBuf>,
    /// When the folder was last read (seconds of real time).
    read_at: f64,
    /// The scene files, by name.
    pub entries: Vec<SceneEntry>,
    /// Decoded thumbnails by sidecar path, with the file time they show.
    textures: HashMap<PathBuf, (Option<SystemTime>, egui::TextureHandle)>,
    /// The scene being renamed and the name typed so far.
    renaming: Option<(PathBuf, String)>,
    /// The scene waiting for the trash confirm.
    confirm: Option<PathBuf>,
}

crate::impl_panel_toggle!(ScenesPanel, open);

impl ScenesPanel {
    /// Reads `folder` again when it is not the listed one or the listing is
    /// a second old at `now`.
    pub fn refresh(&mut self, folder: &Path, now: f64) {
        if self.listed.as_deref() != Some(folder) || now - self.read_at >= REFRESH_S {
            self.entries = list_scenes(folder);
            self.listed = Some(folder.to_path_buf());
            self.read_at = now;
        }
    }

    /// The decoded thumbnail at `path`, decoded again when the file changed.
    fn texture(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureId> {
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if let Some((at, handle)) = self.textures.get(path)
            && *at == modified
        {
            return Some(handle.id());
        }
        let image = decode(path)?;
        let handle = ctx.load_texture(path.to_string_lossy(), image, egui::TextureOptions::LINEAR);
        let id = handle.id();
        self.textures.insert(path.to_path_buf(), (modified, handle));
        Some(id)
    }
}

/// A PNG file as an egui image, or `None` when it does not decode.
fn decode(path: &Path) -> Option<egui::ColorImage> {
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
    let bytes = std::fs::read(path).ok()?;
    let image = Image::from_buffer(
        &bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Default,
        bevy::asset::RenderAssetUsages::default(),
    )
    .ok()?;
    let size = image.size();
    let mut rgba = Vec::with_capacity((size.x * size.y * 4) as usize);
    for y in 0..size.y {
        for x in 0..size.x {
            let c = image.get_color_at(x, y).ok()?.to_srgba().to_u8_array();
            rgba.extend_from_slice(&c);
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [size.x as usize, size.y as usize],
        &rgba,
    ))
}

/// What one frame of the panel asks for.
enum Action {
    Open(PathBuf),
    Rename(PathBuf, PathBuf),
    Trash(PathBuf),
    Browse,
}

/// Draws the Scenes window and invokes the operations its tiles ask for.
pub fn scenes_window(
    mut contexts: EguiContexts,
    mut panel: ResMut<ScenesPanel>,
    folder: Res<ScenesFolder>,
    scene_folder: Option<Res<SceneFolder>>,
    time: Res<Time<Real>>,
    mut invoke: MessageWriter<InvokeOp>,
    strings: Res<Strings>,
) -> Result {
    if !panel.open {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?.clone();
    let dir = folder.resolve(scene_folder.as_deref().and_then(|f| f.0.as_deref()));
    panel.refresh(&dir, time.elapsed_secs_f64());
    let mut actions = Vec::new();
    let mut open = true;
    egui::Window::new(strings.t("panel-scenes"))
        .open(&mut open)
        .default_width(440.0)
        .show(&ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(dir.display().to_string());
                if ui
                    .small_button(strings.t("scenes-folder"))
                    .on_hover_text(strings.t("scenes-folder-hint"))
                    .clicked()
                {
                    actions.push(Action::Browse);
                }
            });
            ui.separator();
            if panel.entries.is_empty() {
                crate::widgets::empty_state(ui, strings.t("scenes-empty"));
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let entries = panel.entries.clone();
                    for entry in &entries {
                        tile(ui, &ctx, &strings, &mut panel, entry, &mut actions);
                    }
                });
            });
        });
    panel.open = open;
    confirm_window(&ctx, &strings, &mut panel, &mut actions);
    for action in actions {
        invoke.write(match action {
            Action::Open(path) => {
                InvokeOp::with("scene.load", json!({"path": path.to_string_lossy()}))
            }
            Action::Rename(from, to) => InvokeOp::with(
                "scene.rename",
                json!({"from": from.to_string_lossy(), "to": to.to_string_lossy()}),
            ),
            Action::Trash(path) => {
                InvokeOp::with("scene.trash", json!({"path": path.to_string_lossy()}))
            }
            Action::Browse => InvokeOp::with("scene.browse_dir", json!({"path": ""})),
        });
    }
    Ok(())
}

/// One scene's tile: its thumbnail and its name (or the rename field).
fn tile(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    strings: &Strings,
    panel: &mut ScenesPanel,
    entry: &SceneEntry,
    actions: &mut Vec<Action>,
) {
    ui.vertical(|ui| {
        ui.set_width(TILE.x);
        let texture = entry
            .thumbnail
            .as_deref()
            .and_then(|p| panel.texture(ctx, p));
        let response = match texture {
            Some(id) => ui.add(
                egui::Image::new((id, TILE))
                    .sense(egui::Sense::click())
                    .corner_radius(2.0),
            ),
            None => ui.add_sized(TILE, egui::Button::new(strings.t("scenes-no_thumbnail"))),
        }
        .on_hover_text(strings.t("scenes-tile-hint"));
        if response.clicked() {
            actions.push(Action::Open(entry.path.clone()));
        }
        if response.hovered() {
            // The tile owns F2 and Delete, not the canvas.
            crate::unit_fields::claim_keys(ctx);
            if ui.input(|i| i.key_pressed(egui::Key::F2)) {
                panel.renaming = Some((entry.path.clone(), entry.name.clone()));
            }
            if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                panel.confirm = Some(entry.path.clone());
            }
        }
        rename_row(ui, panel, entry, actions);
    });
}

/// The tile's name, or the rename field while F2 is in progress: Enter
/// renames, Esc or a click elsewhere cancels.
fn rename_row(
    ui: &mut egui::Ui,
    panel: &mut ScenesPanel,
    entry: &SceneEntry,
    actions: &mut Vec<Action>,
) {
    let Some((path, text)) = panel.renaming.as_mut().filter(|(p, _)| *p == entry.path) else {
        ui.label(egui::RichText::new(&entry.name).small());
        return;
    };
    let field = ui.add(egui::TextEdit::singleline(text).desired_width(TILE.x));
    if !field.has_focus() && !field.lost_focus() {
        field.request_focus();
    }
    if field.lost_focus() {
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let name = text.trim();
        if enter && !name.is_empty() && name != entry.name {
            let to = path.with_file_name(format!("{name}.ron"));
            actions.push(Action::Rename(path.clone(), to));
        }
        panel.renaming = None;
    }
}

/// The trash confirm: Move to trash, or Cancel.
fn confirm_window(
    ctx: &egui::Context,
    strings: &Strings,
    panel: &mut ScenesPanel,
    actions: &mut Vec<Action>,
) {
    let Some(path) = panel.confirm.clone() else {
        return;
    };
    let name = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let mut decided = false;
    egui::Window::new(strings.t("scenes-trash-title"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(strings.t("scenes-trash-ask").replace("{}", &name));
            ui.horizontal(|ui| {
                if ui.button(strings.t("scenes-trash-yes")).clicked() {
                    actions.push(Action::Trash(path.clone()));
                    decided = true;
                }
                if ui.button(strings.t("scenes-trash-no")).clicked() {
                    decided = true;
                }
            });
        });
    if decided {
        panel.confirm = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_is_read_again_on_a_new_folder_or_after_a_second() {
        let dir =
            std::env::temp_dir().join(format!("gradiance-scenes-panel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let mut panel = ScenesPanel::default();
        panel.refresh(&dir, 0.0);
        assert_eq!(panel.entries, Vec::<SceneEntry>::new());
        std::fs::write(dir.join("a.ron"), "").unwrap_or_else(|e| panic!("{e}"));
        panel.refresh(&dir, 0.5);
        assert_eq!(panel.entries, Vec::<SceneEntry>::new(), "too soon");
        panel.refresh(&dir, 1.0);
        assert_eq!(panel.entries.len(), 1);
    }
}
