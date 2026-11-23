//! A scene file's thumbnail (brief S-17): the bodies of a [`SceneRecord`]
//! drawn flat in their fill colours on the look's backdrop.
//!
//! The thumbnail is drawn from the records, not captured from the window,
//! so a headless save, a CLI save and a windowed save write the same image
//! (decision `thumbnail-from-records`). Each body is sampled through its
//! signed distance field (`geometry::sdf`), the shape tree's one exact
//! inside test, at one sample per pixel centre, far bodies first. Pure: no
//! world, no disk. The persist layer encodes the pixels as the PNG sidecar.

use bevy::math::Vec2;
use gradiance_domain::appearance::Rgba;
use gradiance_domain::theme::active;
use gradiance_geometry::sdf;
use gradiance_scene::SceneRecord;

/// The thumbnail's width in pixels.
pub const THUMB_WIDTH: u32 = 256;

/// The thumbnail's height in pixels (16:10).
pub const THUMB_HEIGHT: u32 = 160;

/// How much room the bodies leave around them: a fraction of the frame.
const MARGIN: f32 = 0.08;

/// The frame an empty scene (or one with only a ground) shows, in metres.
const EMPTY_FRAME: Vec2 = Vec2::new(8.0, 5.0);

/// The thumbnail of `scene`: [`THUMB_WIDTH`] × [`THUMB_HEIGHT`] sRGB RGBA
/// bytes, top row first. The frame fits every finite body; a half-plane
/// ground fills whatever part of the frame it covers.
#[must_use]
pub fn draw(scene: &SceneRecord) -> Vec<u8> {
    let theme = active(Some(&scene.environment.render));
    let (w, h) = (THUMB_WIDTH as usize, THUMB_HEIGHT as usize);
    let mut pixels = vec![[0.0_f32; 3]; w * h];
    let back = theme.backdrop;
    pixels.fill([back.r, back.g, back.b]);

    let (centre, scale) = frame(scene);
    #[expect(clippy::cast_precision_loss, reason = "a thumbnail's size")]
    let (wf, hf) = (w as f32, h as f32);
    let to_world = |px: usize, py: usize| -> Vec2 {
        #[expect(clippy::cast_precision_loss, reason = "a thumbnail's size")]
        let screen = Vec2::new(px as f32 + 0.5 - wf / 2.0, hf / 2.0 - (py as f32 + 0.5));
        centre + screen / scale
    };

    let mut order: Vec<&gradiance_scene::BodyRecord> = scene.bodies.iter().collect();
    // Far bodies first, so a near body paints over them.
    order.sort_by(|a, b| b.depth.near.0.total_cmp(&a.depth.near.0));
    for body in order {
        let fill = theme.display(body.appearance.fill);
        let inv = Vec2::from_angle(-body.pose.rot);
        let local = |p: Vec2| inv.rotate(p - body.pose.pos);
        let (x0, x1, y0, y1) = if body.shape.contains_half_plane() {
            (0, w, 0, h)
        } else {
            pixel_box(body, centre, scale, w, h)
        };
        for py in y0..y1 {
            for px in x0..x1 {
                if sdf::eval(&body.shape, local(to_world(px, py))) < 0.0 {
                    let cell = &mut pixels[py * w + px];
                    *cell = over(*cell, fill);
                }
            }
        }
    }
    pixels
        .iter()
        .flat_map(|[r, g, b]| [byte(*r), byte(*g), byte(*b), 255])
        .collect()
}

/// The world point at the frame's centre and the pixels per metre that fit
/// every finite body (with [`MARGIN`]) into the thumbnail.
fn frame(scene: &SceneRecord) -> (Vec2, f32) {
    let bounds = scene
        .bodies
        .iter()
        .filter(|b| !b.shape.contains_half_plane())
        .map(world_box)
        .filter(|(lo, hi)| lo.is_finite() && hi.is_finite())
        .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)));
    let (centre, size) = bounds.map_or((Vec2::ZERO, EMPTY_FRAME), |(lo, hi)| {
        ((lo + hi) / 2.0, (hi - lo).max(Vec2::splat(0.1)))
    });
    #[expect(clippy::cast_precision_loss, reason = "a thumbnail's size")]
    let room = Vec2::new(THUMB_WIDTH as f32, THUMB_HEIGHT as f32) * (1.0 - 2.0 * MARGIN);
    let scale = (room / size).min_element();
    (centre, scale)
}

/// A body's world-space bounds: its local SDF bounds turned by its pose.
fn world_box(body: &gradiance_scene::BodyRecord) -> (Vec2, Vec2) {
    let (lo, hi) = sdf::aabb(&body.shape);
    let rot = Vec2::from_angle(body.pose.rot);
    [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)]
        .into_iter()
        .map(|c| body.pose.pos + rot.rotate(c))
        .fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(a, b), p| (a.min(p), b.max(p)),
        )
}

/// The pixel rows and columns a body's bounds cover, clamped to the image.
fn pixel_box(
    body: &gradiance_scene::BodyRecord,
    centre: Vec2,
    scale: f32,
    w: usize,
    h: usize,
) -> (usize, usize, usize, usize) {
    let (lo, hi) = world_box(body);
    #[expect(clippy::cast_precision_loss, reason = "a thumbnail's size")]
    let half = Vec2::new(w as f32, h as f32) / 2.0;
    let to_px = |p: Vec2| {
        let s = (p - centre) * scale;
        Vec2::new(half.x + s.x, half.y - s.y)
    };
    let (a, b) = (to_px(lo), to_px(hi));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to the image first"
    )]
    let clamp = |v: f32, max: usize| {
        #[expect(clippy::cast_precision_loss, reason = "a thumbnail's size")]
        let m = max as f32;
        v.clamp(0.0, m) as usize
    };
    (
        clamp(a.x.min(b.x).floor(), w),
        clamp(a.x.max(b.x).ceil() + 1.0, w),
        clamp(a.y.min(b.y).floor(), h),
        clamp(a.y.max(b.y).ceil() + 1.0, h),
    )
}

/// `fill` over `under` by its alpha.
fn over(under: [f32; 3], fill: Rgba) -> [f32; 3] {
    let a = fill.a.clamp(0.0, 1.0);
    [
        under[0] + (fill.r - under[0]) * a,
        under[1] + (fill.g - under[1]) * a,
        under[2] + (fill.b - under[2]) * a,
    ]
}

/// A channel in `[0, 1]` as a byte.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 first"
)]
fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_core::ids::StableId;
    use gradiance_domain::shape::ShapeDef;

    fn scene_of(bodies: Vec<gradiance_scene::BodyRecord>) -> SceneRecord {
        let mut scene = crate::ops::scene::empty_scene();
        scene.bodies = bodies;
        scene
    }

    fn body(pos: Vec2, shape: ShapeDef, fill: Rgba) -> gradiance_scene::BodyRecord {
        let mut record = crate::ops::spawned_body_record(StableId::new(), shape, pos);
        record.appearance.fill = fill;
        record
    }

    fn pixel(image: &[u8], x: usize, y: usize) -> [u8; 4] {
        let i = (y * THUMB_WIDTH as usize + x) * 4;
        [image[i], image[i + 1], image[i + 2], image[i + 3]]
    }

    #[test]
    fn an_empty_scene_is_the_backdrop() {
        let image = draw(&scene_of(Vec::new()));
        assert_eq!(image.len(), (THUMB_WIDTH * THUMB_HEIGHT * 4) as usize);
        let first = pixel(&image, 0, 0);
        assert!(image.as_chunks::<4>().0.iter().all(|p| *p == first));
    }

    #[test]
    fn a_body_fills_the_middle_in_its_colour_and_the_corners_stay_backdrop() {
        let red = Rgba::rgb(1.0, 0.0, 0.0);
        let scene = scene_of(vec![body(
            Vec2::new(3.0, 2.0),
            ShapeDef::Circle { radius: 1.0 },
            red,
        )]);
        let image = draw(&scene);
        let theme = active(Some(&scene.environment.render));
        let shown = theme.display(red);
        let middle = pixel(&image, THUMB_WIDTH as usize / 2, THUMB_HEIGHT as usize / 2);
        assert_eq!(middle, [byte(shown.r), byte(shown.g), byte(shown.b), 255]);
        let back = theme.backdrop;
        assert_eq!(
            pixel(&image, 1, 1),
            [byte(back.r), byte(back.g), byte(back.b), 255]
        );
    }
}
