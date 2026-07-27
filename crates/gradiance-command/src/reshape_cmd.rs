//! Replace a sketched body's geometry with a re-solved sketch.
//!
//! Re-opening a sketch is what makes constrained modelling worth the solver:
//! without it a sketch is a one-shot recipe, and the constraints it carries are
//! decoration. With it, "make that edge 3 metres" is a thing you can say to a
//! body that already exists.
//!
//! # Why this is one command rather than a property edit
//!
//! Re-solving a sketch can move its centroid, and body geometry is
//! centroid-relative — so the shape, the stored sketch, and the body's position
//! all have to change together or the body visibly jumps. Splitting that across
//! a `PropertyEditIntent` and a `CommitTransformIntent` would make one gesture
//! produce two undo steps, and an undo of only half of it would leave the body
//! somewhere it never was.

use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::sketch::SketchDoc;

use crate::{CommandError, GameCommand, resolve};

/// Rewrite one body's shape, sketch and position from a re-solved sketch.
#[derive(Debug, Clone)]
pub struct ReshapeBodyCommand {
    /// The body to rewrite.
    pub id: StableId,
    /// The new centroid-relative geometry.
    pub shape: ShapeDef,
    /// The sketch it was lowered from, stored so the body stays re-openable.
    pub sketch: SketchDoc,
    /// Where the new centroid sits in world space.
    pub origin: Vec2,
}

impl GameCommand for ReshapeBodyCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        self.shape.validate()?;
        let entity = resolve(world, self.id)?;
        let mut body = world
            .get_entity_mut(entity)
            .map_err(|_| CommandError::NoEffect)?;

        // Depth is authored separately, so only the planar position moves —
        // reshaping a body must not drag it out of its depth band.
        if let Some(mut transform) = body.get_mut::<Transform>() {
            transform.translation.x = self.origin.x;
            transform.translation.y = self.origin.y;
        }
        body.insert((self.shape.clone(), self.sketch.clone()));
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::intent::name::RESHAPE_BODY
    }
}
