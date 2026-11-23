//! Shape from an image (brief S-14): `import-image` makes one body whose
//! outline follows the opaque pixels, with the image on its face.

#![allow(clippy::unwrap_used)]

use crate::harness::{paused_app, undo};
use bevy::prelude::*;
use gradiance::domain::texture::BodyTexture;
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
}

#[test]
fn an_imported_disc_is_a_disc_body_with_its_image() {
    let mut app = paused_app();
    // A 64 px PNG, a disc of 28 px radius: 1 m wide, so r = 0.4375 m.
    run(
        &mut app,
        "(import-image \"assets/images/import-disc.png\" 2 3 1)",
    );
    let mut query = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Transform, &BodyTexture), With<Body>>();
    let bodies: Vec<_> = query
        .iter(app.world())
        .map(|(s, t, x)| (s.clone(), *t, x.clone()))
        .collect();
    assert_eq!(bodies.len(), 1, "one body");
    let (shape, transform, texture) = &bodies[0];
    let area = gradiance::geometry::polygonize::polygonize(shape).area();
    let r = 28.0 / 64.0;
    let exact = std::f32::consts::PI * r * r;
    assert!((area / exact - 1.0).abs() < 0.03, "{area} vs {exact}");
    let centre = transform.translation.truncate() + texture.offset.0;
    assert!(
        centre.abs_diff_eq(Vec2::new(2.0, 3.0), 1e-3),
        "the image centre is where it was asked: {centre}"
    );
    assert_eq!(texture.width.0, 1.0);
    undo(&mut app);
    app.update();
    let mut count = app.world_mut().query_filtered::<(), With<Body>>();
    assert_eq!(count.iter(app.world()).count(), 0, "one undo step");
}

#[test]
fn a_missing_file_spawns_nothing() {
    let mut app = paused_app();
    run(&mut app, "(import-image \"no/such.png\" 0 0 1)");
    let mut count = app.world_mut().query_filtered::<(), With<Body>>();
    assert_eq!(count.iter(app.world()).count(), 0);
}
