//! The sketch operation (brief N-6): a freehand stroke becomes one body.
//!
//! `body.sketch` is what the sketch tool commits: the tool invokes it with
//! the stroke it recorded and its width, so a drawn sketch and a scripted
//! one are the same record, and Adjust Last Operation can change a drawn
//! stroke's width afterwards.
//!
//! The shape comes from `geometry::sketch::sketch_shape`: an open stroke
//! is simplified (Ramer-Douglas-Peucker) into capsule segments joined as one
//! SDF union; a stroke that closes on itself fills as a polygon. Either way
//! it is one body and one `spawn-body` undo step, and `polygonize` makes its
//! collider and mesh.

use crate::intent::{SpawnBodyIntent, name};
use bevy::prelude::*;
use gradiance_geometry::sketch::{MIN_SKETCH_LENGTH, sketch_shape};
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam};
use gradiance_units::Length;
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Sketch>();
}

/// Author one body from a stroke `width` wide along `path` (flat world
/// coordinates `[x0, y0, x1, y1, …]`).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Sketch {
    /// The stroke, world coordinates in pairs: `[x0, y0, x1, y1, …]`.
    pub path: Vec<f32>,
    /// The stroke's width (each capsule's diameter).
    pub width: Length,
}

impl Operation for Sketch {
    const META: OpMeta = OpMeta {
        id: "body.sketch",
        lisp: "sketch",
        doc: "Author one body from a freehand stroke width wide along path [x0 y0 x1 y1 …]: simplified into capsule segments joined as one union, or filled as a polygon when the stroke closes on itself. Returns its handle.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SPAWN_BODY),
        example: r#"{"path": [-1, 1, -0.5, 1.4, 0, 1, 0.5, 1.4, 1, 1], "width": 0.1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        if !self.path.len().is_multiple_of(2) {
            return Err(OpError::Invalid(
                "path: an even count of numbers, x y pairs".to_owned(),
            ));
        }
        let width = self.width.0;
        if !(width.is_finite() && width > 0.0) {
            return Err(OpError::Invalid(format!(
                "width {width}: a positive length"
            )));
        }
        let path: Vec<Vec2> = self
            .path
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[x, y]| Vec2::new(*x, *y))
            .collect();
        let sketch = sketch_shape(&path, width / 2.0).ok_or_else(|| {
            OpError::Invalid(format!(
                "no sketch: the path needs finite points at least {MIN_SKETCH_LENGTH} m long along it"
            ))
        })?;
        let mut record = super::spawned_body_record(ctx.new_id(), sketch.shape, sketch.pos);
        record.pose.rot = sketch.rot;
        let handle = record.id.0.to_string();
        Ok(OpOutput::none()
            .message(SpawnBodyIntent { record })
            .returning(handle))
    }
}
