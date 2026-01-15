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
