use bevy::prelude::*;

pub trait GameCommand: Send + Sync {
    fn execute(&mut self, world: &mut World);
    fn undo(&mut self, world: &mut World);
}

#[derive(Resource, Default)]
pub struct CommandStack {
    undo_stack: Vec<Box<dyn GameCommand>>,
    redo_stack: Vec<Box<dyn GameCommand>>,
}

impl CommandStack {
    pub fn push(&mut self, mut command: Box<dyn GameCommand>, world: &mut World) {
        command.execute(world);
        self.undo_stack.push(command);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, world: &mut World) {
        if let Some(mut cmd) = self.undo_stack.pop() {
            cmd.undo(world);
            self.redo_stack.push(cmd);
        }
    }

    pub fn redo(&mut self, world: &mut World) {
        if let Some(mut cmd) = self.redo_stack.pop() {
            cmd.execute(world);
            self.undo_stack.push(cmd);
        }
    }
}

pub struct SubmitGameCommand(pub Box<dyn GameCommand>);

impl bevy::ecs::world::Command for SubmitGameCommand {
    fn apply(self, world: &mut World) {
        if let Some(mut stack) = world.remove_resource::<CommandStack>() {
            stack.push(self.0, world);
            world.insert_resource(stack);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    #[derive(Resource, Default)]
    struct Counter(i32);

    struct AddCommand(i32);
    impl GameCommand for AddCommand {
        fn execute(&mut self, world: &mut World) {
            world.resource_mut::<Counter>().0 += self.0;
        }
        fn undo(&mut self, world: &mut World) {
            world.resource_mut::<Counter>().0 -= self.0;
        }
    }

    #[fixture]
    fn world() -> World {
        let mut w = World::new();
        w.init_resource::<Counter>();
        w
    }

    #[rstest]
    fn test_undo_redo(mut world: World) {
        let mut stack = CommandStack::default();

        stack.push(Box::new(AddCommand(10)), &mut world);
        assert_eq!(world.resource::<Counter>().0, 10);

        stack.undo(&mut world);
        assert_eq!(world.resource::<Counter>().0, 0);

        stack.redo(&mut world);
        assert_eq!(world.resource::<Counter>().0, 10);
    }
}
