use crate::commands::GameCommand;
use avian2d::prelude::*;
use bevy::prelude::*;

pub struct DeleteCommand {
    pub entity: Entity,
    pub components: Option<DeletedComponents>,
}

#[derive(Clone)]
pub struct DeletedComponents {
    pub rigid_body: Option<RigidBody>,
    pub collider: Option<Collider>,
    pub friction: Option<Friction>,
    pub restitution: Option<Restitution>,
    pub visibility: Visibility,
}

impl DeleteCommand {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            components: None,
        }
    }
}

impl GameCommand for DeleteCommand {
    fn execute(&mut self, world: &mut World) {
        let mut components = DeletedComponents {
            rigid_body: None,
            collider: None,
            friction: None,
            restitution: None,
            visibility: Visibility::Inherited,
        };

        if let Ok(mut entity_mut) = world.get_entity_mut(self.entity) {
            components.rigid_body = entity_mut.take::<RigidBody>();
            components.collider = entity_mut.take::<Collider>();
            components.friction = entity_mut.take::<Friction>();
            components.restitution = entity_mut.take::<Restitution>();

            if let Some(vis) = entity_mut.get::<Visibility>() {
                components.visibility = *vis;
            } else {
                // If no visibility, assume Inherited (default)
                components.visibility = Visibility::Inherited;
            }

            // Set visibility to hidden
            entity_mut.insert(Visibility::Hidden);
        }

        self.components = Some(components);
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(components) = &self.components {
            if let Ok(mut entity_mut) = world.get_entity_mut(self.entity) {
                if let Some(rb) = components.rigid_body {
                    entity_mut.insert(rb);
                }
                if let Some(col) = &components.collider {
                    entity_mut.insert(col.clone());
                }
                if let Some(fr) = &components.friction {
                    entity_mut.insert(fr.clone());
                }
                if let Some(re) = &components.restitution {
                    entity_mut.insert(re.clone());
                }

                // Restore visibility
                entity_mut.insert(components.visibility);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    #[fixture]
    fn world() -> World {
        World::new()
    }

    #[rstest]
    fn test_delete_command(mut world: World) {
        let entity = world.spawn((
            RigidBody::Dynamic,
            Collider::rectangle(10.0, 10.0),
            Friction::new(0.5),
            Restitution::new(0.5),
            Visibility::Visible,
        )).id();

        let mut cmd = DeleteCommand::new(entity);

        // Execute (Delete)
        cmd.execute(&mut world);

        let e_ref = world.entity(entity);
        assert!(e_ref.get::<RigidBody>().is_none());
        assert!(e_ref.get::<Collider>().is_none());
        assert!(e_ref.get::<Friction>().is_none());
        assert!(e_ref.get::<Restitution>().is_none());
        assert_eq!(*e_ref.get::<Visibility>().unwrap(), Visibility::Hidden);

        // Undo
        cmd.undo(&mut world);

        let e_ref = world.entity(entity);
        assert!(e_ref.get::<RigidBody>().is_some());
        assert!(e_ref.get::<Collider>().is_some());
        assert!(e_ref.get::<Friction>().is_some());
        assert!(e_ref.get::<Restitution>().is_some());
        assert_eq!(*e_ref.get::<Visibility>().unwrap(), Visibility::Visible);
    }
}
