//! Gear geometry generation.
//!
//! Provides the `GearProfile` struct and methods to generate involute gear paths and points.

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use std::f32::consts::PI;

/// Parameters for generating a spur gear.
#[derive(Clone, Copy, Debug, Reflect)]
pub struct GearProfile {
    /// Module (m) determines the size of the teeth (Pitch Diameter / Number of Teeth).
    pub module: f32,
    /// Number of teeth (Z).
    pub teeth: u32,
    /// Pressure angle in degrees (phi), typically 20.0.
    pub pressure_angle: f32,
}

impl Default for GearProfile {
    fn default() -> Self {
        Self {
            module: 10.0,
            teeth: 12,
            pressure_angle: 20.0,
        }
    }
}

impl GearProfile {
    /// Create a new GearProfile.
    pub fn new(module: f32, teeth: u32, pressure_angle: f32) -> Self {
        Self {
            module,
            teeth,
            pressure_angle,
        }
    }

    /// Generates the SVG-like Path for the gear.
    pub fn build_path(&self) -> Path {
        let mut builder = PathBuilder::new();
        // Use higher resolution for rendering
        let points = self.generate_points(10);

        if points.is_empty() {
            return builder.build();
        }

        builder.move_to(points[0]);
        for point in &points[1..] {
            builder.line_to(*point);
        }
        builder.close();

        builder.build()
    }

    /// Generates points describing the outline of the gear.
    ///
    /// `resolution` determines how many points approximate the involute curve per tooth flank.
    pub fn generate_points(&self, resolution: usize) -> Vec<Vec2> {
        let m = self.module;
        let z = self.teeth as f32;
        let phi = self.pressure_angle.to_radians();

        // Radii
        let r_pitch = z * m / 2.0;
        let r_base = r_pitch * phi.cos();
        let r_addendum = r_pitch + m;
        let r_dedendum = r_pitch - 1.25 * m;
        let r_root = r_dedendum;

        // Involute function inv(alpha) = tan(alpha) - alpha
        fn inv(alpha: f32) -> f32 {
            alpha.tan() - alpha
        }

        // Half angular thickness at base circle
        let theta_tooth_half = PI / (2.0 * z) + inv(phi);

        let mut points = Vec::new();
        let start_r = r_base.max(r_root);

        for i in 0..self.teeth {
            let angle_offset = i as f32 * 2.0 * PI / z;

            // Generate single tooth points (centered at angle 0)
            let mut left_flank = Vec::new();
            let mut right_flank = Vec::new();

            // Root to Base (if root < base) - radial line
            if r_root < r_base {
                let theta_l = theta_tooth_half;
                let theta_r = -theta_tooth_half;
                left_flank.push(Vec2::new(r_root * theta_l.cos(), r_root * theta_l.sin()));
                right_flank.push(Vec2::new(r_root * theta_r.cos(), r_root * theta_r.sin()));
            }

            // Involute curve (Base to Addendum)
            for j in 0..=resolution {
                let r = start_r + (r_addendum - start_r) * (j as f32 / resolution as f32);
                // alpha = pressure angle at radius r
                let alpha = (r_base / r).clamp(-1.0, 1.0).acos();
                let inv_alpha = alpha.tan() - alpha;

                let theta_l = theta_tooth_half - inv_alpha;
                let theta_r = -theta_tooth_half + inv_alpha;

                left_flank.push(Vec2::new(r * theta_l.cos(), r * theta_l.sin()));
                right_flank.push(Vec2::new(r * theta_r.cos(), r * theta_r.sin()));
            }

            // Combine into one tooth polygon (Right flank up -> Tip -> Left flank down)
            // Tip is implicit connection between last right and last left
            let mut tooth = right_flank;
            tooth.extend(left_flank.into_iter().rev());

            // Transform and add to main list
            for pt in tooth {
                let rot = Vec2::from_angle(angle_offset);
                points.push(Vec2::new(
                    pt.x * rot.x - pt.y * rot.y,
                    pt.x * rot.y + pt.y * rot.x,
                ));
            }

            // Root arc to next tooth
            let angle_end_current = angle_offset + theta_tooth_half;
            let angle_start_next = angle_offset + 2.0 * PI / z - theta_tooth_half;

             if resolution > 1 {
                 let arc_steps = 2; // Simple subdivision
                 for k in 1..arc_steps {
                     let t = k as f32 / arc_steps as f32;
                     let angle = angle_end_current + (angle_start_next - angle_end_current) * t;
                     points.push(Vec2::new(r_root * angle.cos(), r_root * angle.sin()));
                 }
             }
        }

        points
    }
}
