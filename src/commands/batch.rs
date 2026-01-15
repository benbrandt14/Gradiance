use crate::commands::GameCommand;
use bevy::prelude::*;

pub struct BatchCommand(pub Vec<Box<dyn GameCommand>>);

impl GameCommand for BatchCommand {
    fn execute(&mut self, world: &mut World) {
        for cmd in self.0.iter_mut() {
            cmd.execute(world);
        }
    }

    fn undo(&mut self, world: &mut World) {
        // Undo in reverse order
        for cmd in self.0.iter_mut().rev() {
            cmd.undo(world);
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
    fn test_batch_command(mut world: World) {
        let cmd1 = Box::new(AddCommand(10));
        let cmd2 = Box::new(AddCommand(5));

        let mut batch = BatchCommand(vec![cmd1, cmd2]);

        // Execute
        batch.execute(&mut world);
        assert_eq!(world.resource::<Counter>().0, 15);

        // Undo
        batch.undo(&mut world);
        assert_eq!(world.resource::<Counter>().0, 0);
    }
}
