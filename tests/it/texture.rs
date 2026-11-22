//! Body images (brief S-13): set, place, mode and remove, each one undo
//! step, and the record round trip through the save format.

#![allow(clippy::unwrap_used)]

use crate::harness::{paused_app, undo};
use bevy::prelude::*;
use gradiance::domain::texture::{BodyTexture, TextureMode};
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
}

/// The first body's texture.
fn texture(app: &mut App) -> Option<BodyTexture> {
    let mut query = app
        .world_mut()
        .query_filtered::<Option<&BodyTexture>, With<Body>>();
    query.iter(app.world()).next().flatten().cloned()
}

#[test]
fn an_image_covers_the_face_then_places_and_undoes() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 1 2 2 1)");
    run(&mut app, "(set-texture 0 \"images/logo.png\")");
    let t = texture(&mut app).unwrap();
    assert_eq!(t.path, "images/logo.png");
    assert!(
        (t.width.0 - 2.0).abs() < 1e-3,
        "as wide as the face: {}",
        t.width.0
    );
    assert!(t.offset.0.length() < 1e-3, "centred on the face");
    assert_eq!(t.mode, TextureMode::Toon);

    run(&mut app, "(place-texture 0 0.2 -0.1 0.5 0.3)");
    let placed = texture(&mut app).unwrap();
    assert_eq!(placed.offset.0, Vec2::new(0.2, -0.1));
    assert_eq!((placed.width.0, placed.angle.0), (0.5, 0.3));
    run(&mut app, "(set-texture-mode 0 \"raw\")");
    assert_eq!(texture(&mut app).unwrap().mode, TextureMode::Raw);
    // A new path keeps the placement.
    run(&mut app, "(set-texture 0 \"b.png\")");
    let renamed = texture(&mut app).unwrap();
    assert_eq!((renamed.path.as_str(), renamed.width.0), ("b.png", 0.5));

    run(&mut app, "(set-texture 0 \"\")");
    assert_eq!(texture(&mut app), None, "an empty path removes the image");
    undo(&mut app);
    app.update();
    assert_eq!(texture(&mut app).unwrap().path, "b.png");
    for _ in 0..3 {
        undo(&mut app);
        app.update();
    }
    assert_eq!(texture(&mut app), Some(t), "back to the first image");
    undo(&mut app);
    app.update();
    assert_eq!(texture(&mut app), None);
}

#[test]
fn a_textured_body_saves_and_loads_its_image() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 1 1 1)");
    run(&mut app, "(set-texture 0 \"a.png\")");
    run(&mut app, "(place-texture 0 0.1 0.2 0.4 1)");
    let before = texture(&mut app).unwrap();
    let scene = gradiance::scene::SceneRecord::capture(app.world_mut());
    let text = gradiance::scene::to_ron(&scene).unwrap();
    assert!(text.contains("\"a.png\""), "the path is saved");
    let back = gradiance::scene::from_ron(&text).unwrap();
    assert_eq!(back.bodies[0].texture.as_ref(), Some(&before));
}

#[test]
fn a_cut_keeps_the_image_where_it_was() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 2 1)");
    run(&mut app, "(set-texture 0 \"a.png\")");
    // Cut off the right quarter: the larger piece's centroid moves left.
    run(&mut app, "(cut 0.5 -1 0.5 1 0.01)");
    let mut query = app
        .world_mut()
        .query_filtered::<(&Transform, &BodyTexture), With<Body>>();
    let pieces: Vec<(Vec2, Vec2)> = query
        .iter(app.world())
        .map(|(t, tex)| (t.translation.truncate(), tex.offset.0))
        .collect();
    assert_eq!(pieces.len(), 2, "both pieces keep the image");
    for (pos, offset) in pieces {
        assert!(
            (pos + offset).length() < 1e-3,
            "the image centre stays at the old origin: {pos} + {offset}"
        );
    }
}
