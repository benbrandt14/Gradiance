//! CAD view-orientation cube: a small cube in the top-right corner that
//! rotates with the camera. Click a **face** for an axis-aligned view, a
//! **corner** for an isometric view, or an **edge** for the halfway view —
//! every click also levels the roll. The camera glides to the target via
//! `CameraRig::target` (the same path as `Home`).

use crate::interaction::camera::CameraRig;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Cube half-size on screen, logical pixels.
const HALF: f32 = 26.0;
/// Pick radius for corners/edge midpoints, logical pixels.
const PICK: f32 = 7.0;
/// Face-on views land just short of ±90° so ray-plane picking (and the
/// grid) stay alive; the back view is exactly 180° (the back plane's
/// inside-fade reveals the scene from behind).
const FACE_EPS: f32 = 0.045;

/// The view target for a cube feature point (corner, edge midpoint, or
/// face center, in cube space): yaw/pitch aiming the camera down `-p`,
/// roll leveled. Pure math — unit-tested below.
pub fn view_target(p: Vec3) -> Vec3 {
    let mut yaw = f32::atan2(p.x, p.z);
    let mut pitch = -f32::atan2(p.y, p.x.hypot(p.z));
    let cap = std::f32::consts::FRAC_PI_2 - FACE_EPS;
    // Nudge face-on extremes inside the picking-safe range; ±180° (the
    // back view) stays exact.
    if yaw.abs() > cap && yaw.abs() < std::f32::consts::PI - 0.01 {
        yaw = yaw.clamp(-cap, cap);
    }
    pitch = pitch.clamp(-cap, cap);
    Vec3::new(yaw, pitch, 0.0)
}

/// The 6 face centers, 12 edge midpoints, and 8 corners of the unit cube.
fn feature_points() -> Vec<Vec3> {
    let mut points = Vec::with_capacity(26);
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                if (x, y, z) != (0, 0, 0) {
                    points.push(Vec3::new(x as f32, y as f32, z as f32));
                }
            }
        }
    }
    points
}

/// Renders the view cube and steers the rig on click.
pub fn view_cube(mut contexts: EguiContexts, mut rig: ResMut<CameraRig>) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui::Area::new(egui::Id::new("view-cube"))
        .anchor(egui::Align2::RIGHT_TOP, [-16.0, 16.0])
        .show(ctx, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(HALF * 2.6, HALF * 2.6), egui::Sense::click());
            let center = rect.center();
            // World → view space: what the camera sees of the world axes.
            let inv = rig.rotation().inverse();
            let project = |p: Vec3| -> (egui::Pos2, f32) {
                let v = inv * p;
                (center + egui::vec2(v.x, -v.y) * HALF, v.z)
            };
            let painter = ui.painter_at(rect);

            // Faces, back-to-front by view depth.
            let faces: [(Vec3, &str); 6] = [
                (Vec3::Z, "F"),
                (Vec3::NEG_Z, "B"),
                (Vec3::X, "R"),
                (Vec3::NEG_X, "L"),
                (Vec3::Y, "T"),
                (Vec3::NEG_Y, "D"),
            ];
            let mut visible: Vec<(f32, Vec3, &str)> = faces
                .into_iter()
                .filter_map(|(n, label)| {
                    let depth = (inv * n).z;
                    (depth > 0.02).then_some((depth, n, label))
                })
                .collect();
            visible.sort_by(|a, b| a.0.total_cmp(&b.0));
            let hover = response.hover_pos();
            for (depth, normal, label) in &visible {
                // The face quad: the two axes perpendicular to the normal.
                let (u, v) = if normal.x.abs() > 0.5 {
                    (Vec3::Y, Vec3::Z)
                } else if normal.y.abs() > 0.5 {
                    (Vec3::X, Vec3::Z)
                } else {
                    (Vec3::X, Vec3::Y)
                };
                let corners: Vec<egui::Pos2> = [
                    *normal + u + v,
                    *normal + u - v,
                    *normal - u - v,
                    *normal - u + v,
                ]
                .into_iter()
                .map(|c| project(c).0)
                .collect();
                let hovered = hover.is_some_and(|h| point_in_quad(h, &corners));
                let shade = (105.0 + depth * 65.0) as u8;
                let fill = if hovered {
                    egui::Color32::from_rgb(190, 175, 90)
                } else {
                    egui::Color32::from_gray(shade)
                };
                painter.add(egui::Shape::convex_polygon(
                    corners.clone(),
                    fill,
                    egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
                ));
                painter.text(
                    project(*normal).0,
                    egui::Align2::CENTER_CENTER,
                    *label,
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_gray(30),
                );
            }

            // Click: nearest corner/edge feature within PICK wins over the
            // face under the pointer.
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let mut best: Option<(f32, Vec3)> = None;
                for p in feature_points() {
                    let is_face = (p.x.abs() + p.y.abs() + p.z.abs() - 1.0).abs() < 0.1;
                    let (screen, depth) = project(p);
                    if depth < -0.05 {
                        continue; // behind the cube
                    }
                    let d = screen.distance(pos);
                    let within = if is_face { HALF } else { PICK };
                    if d < within {
                        // Corners/edges (smaller radius) naturally beat the
                        // face because their distance threshold is tighter —
                        // prefer the tightest matching feature.
                        let rank = d + if is_face { 1_000.0 } else { 0.0 };
                        if best.is_none_or(|(r, _)| rank < r) {
                            best = Some((rank, p));
                        }
                    }
                }
                if let Some((_, p)) = best {
                    rig.target = Some(view_target(p));
                }
            }
            response.on_hover_text("click: face = axis view · corner = isometric · edge = level");
        });
    Ok(())
}

/// Point-in-convex-quad test (screen space).
fn point_in_quad(p: egui::Pos2, quad: &[egui::Pos2]) -> bool {
    let mut sign = 0_i8;
    for i in 0..quad.len() {
        let a = quad[i];
        let b = quad[(i + 1) % quad.len()];
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross.abs() < 1e-6 {
            continue;
        }
        let s = if cross > 0.0 { 1 } else { -1 };
        if sign == 0 {
            sign = s;
        } else if s != sign {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn face_clicks_map_to_axis_views() {
        assert_eq!(view_target(Vec3::Z), Vec3::ZERO, "front is home");
        let right = view_target(Vec3::X);
        assert!((right.x - (FRAC_PI_2 - FACE_EPS)).abs() < 1e-4);
        assert!(right.y.abs() < 1e-6 && right.z.abs() < 1e-6);
        let top = view_target(Vec3::Y);
        assert!(
            (top.y + (FRAC_PI_2 - FACE_EPS)).abs() < 1e-4,
            "top pitches the camera above"
        );
        let back = view_target(Vec3::NEG_Z);
        assert!((back.x.abs() - PI).abs() < 1e-4, "back is the 180° view");
    }

    #[test]
    fn corner_clicks_are_isometric() {
        let iso = view_target(Vec3::new(1.0, 1.0, 1.0));
        assert!((iso.x.to_degrees() - 45.0).abs() < 0.5);
        assert!((iso.y.to_degrees() + 35.264).abs() < 0.5);
        assert!(iso.z.abs() < f32::EPSILON, "clicks always level the roll");
    }

    #[test]
    fn edge_clicks_are_halfway_views() {
        let edge = view_target(Vec3::new(1.0, 0.0, 1.0));
        assert!((edge.x.to_degrees() - 45.0).abs() < 0.5);
        assert!(edge.y.abs() < 1e-6);
    }
}
