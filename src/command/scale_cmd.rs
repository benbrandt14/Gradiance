//! Scaling bodies about a pivot in a chosen frame.

use crate::command::{CommandError, GameCommand};
use crate::core::ids::{IdIndex, StableId};
use crate::core::units::PosRot;
use crate::domain::shape::ShapeDef;
use crate::geometry::scale::{body_scale_matrix, scale_point, scale_shape};
use bevy::prelude::*;

/// Minimum allowed scale factor magnitude.
pub const MIN_SCALE_FACTOR: f32 = 0.01;

/// Scales a set of bodies about `pivot` along the axes of a frame rotated
/// by `frame_rot` (0 = world/global axes; a body's rotation = its local
/// axes). Positions scale exactly; shapes scale parametrically when
/// representable and polygonize exactly otherwise.
#[derive(Debug)]
pub struct ScaleCommand {
    /// Bodies to scale.
    pub targets: Vec<StableId>,
    /// Fixed point of the scale, world space.
    pub pivot: Vec2,
    /// Frame rotation in radians (axes along which factors apply).
    pub frame_rot: f32,
    /// Per-axis scale factors in the frame.
    pub factors: Vec2,
    /// Captured `(shape, pose)` per target for undo; filled on first apply.
    old: Vec<(StableId, ShapeDef, PosRot)>,
}

impl ScaleCommand {
    /// Builds a scale command.
    pub fn new(targets: Vec<StableId>, pivot: Vec2, frame_rot: f32, factors: Vec2) -> Self {
        Self {
            targets,
            pivot,
            frame_rot,
            factors,
            old: Vec::new(),
        }
    }
}

impl GameCommand for ScaleCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.targets.is_empty()
            || self.factors.x.abs() < MIN_SCALE_FACTOR
            || self.factors.y.abs() < MIN_SCALE_FACTOR
        {
            return Err(CommandError::NoEffect);
        }
        // Resolve and validate everything before mutating anything.
        let mut staged = Vec::with_capacity(self.targets.len());
        for &id in &self.targets {
            let entity = world
                .resource::<IdIndex>()
                .entity(id)
                .ok_or(CommandError::MissingEntity(id))?;
            let shape = world
                .get::<ShapeDef>(entity)
                .ok_or(CommandError::MissingEntity(id))?
                .clone();
            let transform = *world
                .get::<Transform>(entity)
                .ok_or(CommandError::MissingEntity(id))?;
            let pose = PosRot::from_transform(&transform);

            let m = body_scale_matrix(pose.rot, self.frame_rot, self.factors);
            let new_shape = scale_shape(&shape, m);
            new_shape.validate()?;
            let new_pos = scale_point(pose.pos, self.pivot, self.frame_rot, self.factors);
            staged.push((entity, id, shape, pose, new_shape, new_pos));
        }

        let mut old = Vec::with_capacity(staged.len());
        for (entity, id, shape, pose, new_shape, new_pos) in staged {
            old.push((id, shape, pose));
            let mut entity_mut = world.entity_mut(entity);
            if let Some(mut s) = entity_mut.get_mut::<ShapeDef>() {
                *s = new_shape;
            }
            if let Some(mut t) = entity_mut.get_mut::<Transform>() {
                PosRot {
                    pos: new_pos,
                    rot: pose.rot,
                }
                .apply_to(&mut t);
            }
        }
        if self.old.is_empty() {
            self.old = old;
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        for (id, shape, pose) in &self.old {
            let entity = world
                .resource::<IdIndex>()
                .entity(*id)
                .ok_or(CommandError::MissingEntity(*id))?;
            let mut entity_mut = world.entity_mut(entity);
            if let Some(mut s) = entity_mut.get_mut::<ShapeDef>() {
                *s = shape.clone();
            }
            if let Some(mut t) = entity_mut.get_mut::<Transform>() {
                pose.apply_to(&mut t);
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::command::intent::name::SCALE
    }
}
