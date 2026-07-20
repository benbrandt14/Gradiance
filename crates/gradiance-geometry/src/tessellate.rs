//! Fill tessellation of contours via lyon.

use crate::contours::Contours;
use bevy::math::Vec2;
use lyon::math::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers,
};

/// A tessellated fill: triangle vertices and indices.
#[derive(Debug, Clone, Default)]
pub struct Triangulation {
    /// Vertex positions.
    pub vertices: Vec<Vec2>,
    /// Triangle indices into `vertices`, CCW in the XY plane.
    pub indices: Vec<u32>,
}

impl Triangulation {
    /// Total area covered by the triangles.
    pub fn area(&self) -> f32 {
        self.indices
            .chunks_exact(3)
            .map(|t| {
                let (a, b, c) = (
                    self.vertices[t[0] as usize],
                    self.vertices[t[1] as usize],
                    self.vertices[t[2] as usize],
                );
                (b - a).perp_dot(c - a) / 2.0
            })
            .sum::<f32>()
            .abs()
    }
}

/// Tessellates the interior of `contours` into triangles.
///
/// Uses the even-odd fill rule, so hole winding does not matter. Returns
/// an empty triangulation for degenerate input.
pub fn tessellate(contours: &Contours) -> Triangulation {
    let mut builder = Path::builder();
    for ring in contours.rings() {
        if ring.len() < 3 {
            continue;
        }
        builder.begin(point(ring[0].x, ring[0].y));
        for v in &ring[1..] {
            builder.line_to(point(v.x, v.y));
        }
        builder.close();
    }
    let path = builder.build();

    let mut buffers: VertexBuffers<Vec2, u32> = VertexBuffers::new();
    let options = FillOptions::tolerance(0.05).with_fill_rule(FillRule::EvenOdd);
    let mut tessellator = FillTessellator::new();
    let result = tessellator.tessellate_path(
        &path,
        &options,
        &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| {
            Vec2::new(v.position().x, v.position().y)
        }),
    );
    if result.is_err() {
        return Triangulation::default();
    }
    Triangulation {
        vertices: buffers.vertices,
        indices: buffers.indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polygonize::polygonize;
    use crate::shape::ShapeDef;

    #[test]
    fn tessellated_area_matches_contour_area() {
        let contours = polygonize(&ShapeDef::Polygon {
            // Concave L-shape, area 1200.
            outline: vec![
                Vec2::new(-20.0, -20.0),
                Vec2::new(20.0, -20.0),
                Vec2::new(20.0, 0.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(0.0, 20.0),
                Vec2::new(-20.0, 20.0),
            ],
            holes: vec![],
        });
        let tri = tessellate(&contours);
        assert!(!tri.indices.is_empty());
        assert!((tri.area() - 1200.0).abs() < 1.0);
    }

    #[test]
    fn holes_are_subtracted() {
        let mut hole = vec![
            Vec2::new(-5.0, -5.0),
            Vec2::new(5.0, -5.0),
            Vec2::new(5.0, 5.0),
            Vec2::new(-5.0, 5.0),
        ];
        hole.reverse();
        let contours = Contours {
            outline: vec![
                Vec2::new(-10.0, -10.0),
                Vec2::new(10.0, -10.0),
                Vec2::new(10.0, 10.0),
                Vec2::new(-10.0, 10.0),
            ],
            holes: vec![hole],
        };
        let tri = tessellate(&contours);
        assert!((tri.area() - 300.0).abs() < 1.0);
    }
}
