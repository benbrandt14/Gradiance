use crate::commands::GameCommand;
use avian2d::prelude::*;
use bevy::prelude::*;

pub struct SetFrictionCommand {
    pub entity: Entity,
    pub new_friction: Friction,
    pub old_friction: Option<Friction>,
}

impl GameCommand for SetFrictionCommand {
    fn execute(&mut self, world: &mut World) {
        if let Some(mut friction) = world.get_mut::<Friction>(self.entity) {
            if self.old_friction.is_none() {
                self.old_friction = Some(*friction);
            }
            *friction = self.new_friction;
        }
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(mut friction) = world.get_mut::<Friction>(self.entity) {
            if let Some(old) = self.old_friction {
                *friction = old;
            }
        }
    }
}

pub struct SetRestitutionCommand {
    pub entity: Entity,
    pub new_restitution: Restitution,
    pub old_restitution: Option<Restitution>,
}

impl GameCommand for SetRestitutionCommand {
    fn execute(&mut self, world: &mut World) {
        if let Some(mut restitution) = world.get_mut::<Restitution>(self.entity) {
            if self.old_restitution.is_none() {
                self.old_restitution = Some(*restitution);
            }
            *restitution = self.new_restitution;
        }
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(mut restitution) = world.get_mut::<Restitution>(self.entity) {
            if let Some(old) = self.old_restitution {
                *restitution = old;
            }
        }
    }
}

pub struct SetRigidBodyCommand {
    pub entity: Entity,
    pub new_body_type: RigidBody,
    pub old_body_type: Option<RigidBody>,
}

impl GameCommand for SetRigidBodyCommand {
    fn execute(&mut self, world: &mut World) {
        if let Some(rb) = world.get::<RigidBody>(self.entity) {
            if self.old_body_type.is_none() {
                self.old_body_type = Some(*rb);
            }
        }
        // Insert overwrites existing component
        world.entity_mut(self.entity).insert(self.new_body_type);
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(old) = self.old_body_type {
            world.entity_mut(self.entity).insert(old);
        }
    }
}

pub struct SetColorCommand {
    pub entity: Entity,
    pub new_color: Color,
    pub old_color: Option<Color>,
}

impl GameCommand for SetColorCommand {
    fn execute(&mut self, world: &mut World) {
        // Handle Sprite
        if let Some(mut sprite) = world.get_mut::<Sprite>(self.entity) {
            if self.old_color.is_none() {
                self.old_color = Some(sprite.color);
            }
            sprite.color = self.new_color;
            return;
        }
        // Mesh/Material handling removed to avoid Handle component issues.
        // If we add meshes later, we need to resolve the Handle<ColorMaterial> component issue or wrapper.
    }

    fn undo(&mut self, world: &mut World) {
        // Handle Sprite
        if let Some(mut sprite) = world.get_mut::<Sprite>(self.entity) {
            if let Some(old) = self.old_color {
                sprite.color = old;
            }
            return;
        }
    }
}
