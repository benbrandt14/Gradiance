//! Array repeats: linear and radial patterns of the selection.

use crate::scene::BodyRecord;
use crate::command::{CommandError, GameCommand, resolve};
use crate::core::ids::StableId;
use bevy::prelude::*;

/// How array copies are placed.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub enum ArrayMode {
    /// Copies at `offset`, `2·offset`, … `count·offset`.
    Linear {
        /// Step between consecutive copies.
        offset: Vec2,
    },
    /// Copies rotated about `pivot` by multiples of `angle_step`.
    Radial {
        /// Center of the pattern.
        pivot: Vec2,
        /// Angle between consecutive copies (radians).
        angle_step: f32,
        /// Also rotate each copy's own orientation.
        rotate_items: bool,
    },
}

/// Creates `count` patterned copies of the source bodies (one undo step),
/// including joints internal to the source set (patterned linkages stay
/// linked).
///
/// Clone ids are minted once and reused on redo.
#[derive(Debug)]
pub struct ArrayCommand {
    /// Bodies to pattern.
    pub sources: Vec<StableId>,
    /// Number of copies (not counting the original).
    pub count: u32,
    /// Placement rule.
    pub mode: ArrayMode,
    clones: Vec<BodyRecord>,
    joint_clones: Vec<crate::scene::JointRecord>,
}

impl ArrayCommand {
    /// Builds an array command.
    pub fn new(sources: Vec<StableId>, count: u32, mode: ArrayMode) -> Self {
        Self {
            sources,
            count,
            mode,
            clones: Vec::new(),
            joint_clones: Vec::new(),
        }
    }
}

/// World-point mapping for copy `k` of the pattern.
fn copy_map(mode: ArrayMode, k: u32) -> impl Fn(Vec2) -> Vec2 {
    move |p| match mode {
        ArrayMode::Linear { offset } => p + offset * k as f32,
        ArrayMode::Radial {
            pivot, angle_step, ..
        } => pivot + Vec2::from_angle(angle_step * k as f32).rotate(p - pivot),
    }
}

impl GameCommand for ArrayCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.count == 0 || self.sources.is_empty() {
            return Err(CommandError::NoEffect);
        }
        if self.clones.is_empty() {
            let mut clones = Vec::with_capacity(self.sources.len() * self.count as usize);
            let mut joint_clones = Vec::new();
            let mut next_group = crate::command::spawn::next_group_id(world);
            for k in 1..=self.count {
                let copy_start = clones.len();
                let mut id_map = Vec::with_capacity(self.sources.len());
                for &id in &self.sources {
                    let entity = resolve(world, id)?;
                    let mut clone = BodyRecord::capture(world, entity)
                        .ok_or(CommandError::MissingEntity(id))?;
                    clone.id = StableId::new();
                    match self.mode {
                        ArrayMode::Linear { offset } => {
                            clone.pose.pos += offset * k as f32;
                        }
                        ArrayMode::Radial {
                            pivot,
                            angle_step,
                            rotate_items,
                        } => {
                            let angle = angle_step * k as f32;
                            let rel = clone.pose.pos - pivot;
                            clone.pose.pos = pivot + Vec2::from_angle(angle).rotate(rel);
                            if rotate_items {
                                clone.pose.rot += angle;
                            }
                        }
                    }
                    id_map.push((id, clone.id));
                    clones.push(clone);
                }
                let rot_offset = match self.mode {
                    ArrayMode::Radial {
                        angle_step,
                        rotate_items: true,
                        ..
                    } => angle_step * k as f32,
                    _ => 0.0,
                };
                joint_clones.extend(crate::command::spawn::clone_internal_joints(
                    world,
                    &id_map,
                    copy_map(self.mode, k),
                    rot_offset,
                ));
                // Each copy gets its own selection groups.
                crate::command::spawn::remap_clone_groups(
                    &mut clones[copy_start..],
                    &mut next_group,
                );
            }
            self.clones = clones;
            self.joint_clones = joint_clones;
        }
        for record in &self.clones {
            record.spawn(world);
        }
        for record in &self.joint_clones {
            record.spawn(world);
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        for record in &self.joint_clones {
            let entity = resolve(world, record.id)?;
            world.despawn(entity);
        }
        for record in &self.clones {
            let entity = resolve(world, record.id)?;
            world.despawn(entity);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::command::intent::name::ARRAY
    }
}
