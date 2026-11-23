//! Share a scene as one file (brief S-18): `scene.share` writes a stored
//! ZIP with the scene RON, a thumbnail and the images; opening it with
//! `scene.load` gives the same scene back.

#![allow(clippy::unwrap_used)]

use crate::harness::paused_app;
use bevy::prelude::*;
use gradiance::op::invoke;
use gradiance::persist::bundle::{IMAGES_DIR, SCENE_ENTRY, THUMBNAIL_ENTRY, read_zip};
use serde_json::{Value, json};
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gradiance-it-share-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn op(app: &mut App, id: &str, params: Value) -> Option<Value> {
    let out = invoke(app.world_mut(), id, params).unwrap();
    app.update();
    app.update();
    out
}

fn dump(app: &mut App) -> Value {
    op(app, "scene.dump", json!({})).unwrap()
}

#[test]
fn share_then_open_gives_the_same_scene() {
    let dir = temp_dir("round");
    let mut app = paused_app();
    op(
        &mut app,
        "body.spawn_box",
        json!({"x": 0, "y": 1, "w": 2, "h": 1}),
    );
    op(
        &mut app,
        "body.spawn_circle",
        json!({"x": 3, "y": 2, "r": 0.5}),
    );
    op(
        &mut app,
        "joint.hinge",
        json!({"a": 0, "b": 1, "x": 1, "y": 1}),
    );
    let before = dump(&mut app);
    let bundle = dir.join("machine.zip");
    op(
        &mut app,
        "scene.share",
        json!({"path": bundle.to_string_lossy()}),
    );
    let entries = read_zip(&std::fs::read(&bundle).unwrap()).unwrap();
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, [SCENE_ENTRY, THUMBNAIL_ENTRY]);

    op(&mut app, "scene.new", json!({}));
    assert_ne!(dump(&mut app), before);
    op(
        &mut app,
        "scene.load",
        json!({"path": bundle.to_string_lossy()}),
    );
    assert_eq!(dump(&mut app), before, "the same scene back");
}

#[test]
fn a_shared_image_travels_inside_the_bundle() {
    let dir = temp_dir("image");
    let image = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/images/import-disc.png");
    let mut app = paused_app();
    op(
        &mut app,
        "body.spawn_box",
        json!({"x": 0, "y": 1, "w": 2, "h": 1}),
    );
    op(
        &mut app,
        "body.set_texture",
        json!({"i": 0, "path": image.to_string_lossy()}),
    );
    let bundle = dir.join("textured.zip");
    op(
        &mut app,
        "scene.share",
        json!({"path": bundle.to_string_lossy()}),
    );
    let entries = read_zip(&std::fs::read(&bundle).unwrap()).unwrap();
    let inside = format!("{IMAGES_DIR}/import-disc.png");
    let (_, bytes) = entries.iter().find(|(n, _)| *n == inside).unwrap();
    assert_eq!(*bytes, std::fs::read(&image).unwrap(), "the image itself");

    op(&mut app, "scene.new", json!({}));
    op(
        &mut app,
        "scene.load",
        json!({"path": bundle.to_string_lossy()}),
    );
    let scene = dump(&mut app);
    assert_eq!(scene["bodies"][0]["texture"]["path"], json!(inside));
    let folder = app
        .world()
        .resource::<gradiance::core::scene_folder::SceneFolder>()
        .clone();
    assert!(
        folder.resolve(&inside).is_file(),
        "the image resolves where the bundle opened"
    );
}
