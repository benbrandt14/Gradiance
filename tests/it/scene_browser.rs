//! The scene browser (brief S-17): a save writes a thumbnail sidecar next to
//! the scene, the Scenes panel lists the scene with it, and the rename and
//! trash operations move both files.

#![allow(clippy::unwrap_used)]

use crate::harness::paused_app;
use bevy::prelude::*;
use gradiance::op::invoke;
use gradiance::persist::browser::{ScenesFolder, TRASH_DIR, list_scenes};
use gradiance::ui::scenes::ScenesPanel;
use serde_json::json;
use std::path::PathBuf;

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gradiance-it-browser-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn op(app: &mut App, id: &str, params: serde_json::Value) {
    invoke(app.world_mut(), id, params).unwrap();
    app.update();
    app.update();
}

#[test]
fn a_save_writes_the_thumbnail_and_the_panel_lists_it() {
    let dir = temp_dir();
    let mut app = paused_app();
    op(
        &mut app,
        "body.spawn_box",
        json!({"x": 0, "y": 1, "w": 2, "h": 1}),
    );
    let scene = dir.join("boxes.ron");
    op(
        &mut app,
        "scene.save",
        json!({"path": scene.to_string_lossy()}),
    );
    let sidecar = dir.join("boxes.png");
    assert!(
        scene.is_file() && sidecar.is_file(),
        "the scene and its sidecar"
    );
    let png = std::fs::read(&sidecar).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    // The IHDR chunk holds the width and height, big-endian.
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    assert_eq!((width, height), (256, 160));

    op(
        &mut app,
        "scene.browse_dir",
        json!({"path": dir.to_string_lossy()}),
    );
    assert_eq!(app.world().resource::<ScenesFolder>().0, Some(dir.clone()));
    let mut panel = ScenesPanel::default();
    panel.refresh(&dir, 0.0);
    assert_eq!(panel.entries.len(), 1);
    assert_eq!(panel.entries[0].name, "boxes");
    assert_eq!(panel.entries[0].thumbnail, Some(sidecar));

    op(
        &mut app,
        "scene.rename",
        json!({"from": scene.to_string_lossy(), "to": dir.join("crates.ron").to_string_lossy()}),
    );
    let names: Vec<String> = list_scenes(&dir).into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["crates"]);
    assert!(dir.join("crates.png").is_file(), "the sidecar follows");

    op(
        &mut app,
        "scene.trash",
        json!({"path": dir.join("crates.ron").to_string_lossy()}),
    );
    assert_eq!(list_scenes(&dir).len(), 0);
    let bin = dir.join(TRASH_DIR);
    assert!(bin.join("crates.ron").is_file() && bin.join("crates.png").is_file());
}
