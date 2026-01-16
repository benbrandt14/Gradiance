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
        if let Some(mut rb) = world.get_mut::<RigidBody>(self.entity) {
            if self.old_body_type.is_none() {
                self.old_body_type = Some(*rb);
            }
            *rb = self.new_body_type;
        }
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(mut rb) = world.get_mut::<RigidBody>(self.entity) {
            if let Some(old) = self.old_body_type {
                *rb = old;
            }
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

        // Handle MeshMaterial2d<ColorMaterial>
        // We need to clone the handle id to release the borrow on world components
        let handle_id = if let Some(handle) = world.get::<MeshMaterial2d<ColorMaterial>>(self.entity) {
            Some(handle.id())
        } else {
            None
        };

        if let Some(id) = handle_id {
            let mut materials = world.resource_mut::<Assets<ColorMaterial>>();
            if let Some(mat) = materials.get_mut(id) {
                if self.old_color.is_none() {
                    self.old_color = Some(mat.color);
                }
                mat.color = self.new_color;
            }
        }
    }

    fn undo(&mut self, world: &mut World) {
         // Handle Sprite
        if let Some(mut sprite) = world.get_mut::<Sprite>(self.entity) {
            if let Some(old) = self.old_color {
                sprite.color = old;
            }
            return;
        }

        // Handle MeshMaterial2d<ColorMaterial>
        let handle_id = if let Some(handle) = world.get::<MeshMaterial2d<ColorMaterial>>(self.entity) {
            Some(handle.id())
        } else {
            None
        };

        if let Some(id) = handle_id {
            let mut materials = world.resource_mut::<Assets<ColorMaterial>>();
            if let Some(mat) = materials.get_mut(id) {
                 if let Some(old) = self.old_color {
                     mat.color = old;
                 }
            }
        }
    }
}
