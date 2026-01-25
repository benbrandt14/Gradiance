use bevy::prelude::*;
use proptest::prelude::*;
use gradiance::input::ToolState;
use gradiance::input::editable::EditableBox;
use bevy_rapier2d::prelude::*;

mod test_utils;
use test_utils::*;

#[derive(Debug, Clone)]
enum Action {
    SetTool(ToolState),
    MoveMouse(Vec2),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    KeyPress(KeyCode),
    KeyRelease(KeyCode),
    Update,
}

fn tool_strategy() -> impl Strategy<Value = ToolState> {
    prop_oneof![
        Just(ToolState::Select),
        Just(ToolState::Drag),
        Just(ToolState::Box),
        Just(ToolState::Circle),
        Just(ToolState::Polygon),
        Just(ToolState::RevoluteJoint),
        Just(ToolState::Weld),
        Just(ToolState::Ground),
    ]
}

fn key_strategy() -> impl Strategy<Value = KeyCode> {
    prop_oneof![
        Just(KeyCode::ControlLeft),
        Just(KeyCode::ShiftLeft),
        Just(KeyCode::KeyZ), // Undo
        Just(KeyCode::KeyY), // Redo
        Just(KeyCode::KeyA), // Select All
        Just(KeyCode::Delete),
        Just(KeyCode::Backspace),
        Just(KeyCode::Space), // Pause
    ]
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        tool_strategy().prop_map(Action::SetTool),
        (-100.0..100.0f32, -100.0..100.0f32).prop_map(|(x, y)| Action::MoveMouse(Vec2::new(x, y))),
        prop_oneof![Just(MouseButton::Left), Just(MouseButton::Right)].prop_map(Action::MouseDown),
        prop_oneof![Just(MouseButton::Left), Just(MouseButton::Right)].prop_map(Action::MouseUp),
        key_strategy().prop_map(Action::KeyPress),
        key_strategy().prop_map(Action::KeyRelease),
        Just(Action::Update),
    ]
}

fn apply_action(app: &mut App, action: &Action) {
    match action {
        Action::SetTool(t) => set_tool(app, *t),
        Action::MoveMouse(p) => set_cursor(app, *p),
        Action::MouseDown(b) => mouse_down(app, *b),
        Action::MouseUp(b) => mouse_up(app, *b),
        Action::KeyPress(k) => press_key(app, *k),
        Action::KeyRelease(k) => release_key(app, *k),
        Action::Update => app.update(),
    }
}

proptest! {
    // Limit cases to prevent long running tests in CI/Sandbox
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn prop_crash_safety(actions in prop::collection::vec(action_strategy(), 1..50)) {
        let mut app = create_test_app();

        // Spawn some initial state so tools have something to interact with
        let _ = app.world_mut().spawn((
            RigidBody::Dynamic,
            Collider::cuboid(5.0, 5.0),
            Transform::from_xyz(0.0, 0.0, 0.0),
            GlobalTransform::default(),
            EditableBox { width: 10.0, height: 10.0 },
        )).id();
        app.update();

        for action in actions {
            apply_action(&mut app, &action);
            // Ensure we run update periodically even if action isn't Update
            // but the strategy includes explicit updates, so we rely on that for "user creates pauses"
            // Actually, for safety, let's just apply.
        }
    }
}

// Strategy for Stability Test: Exclude Drag and Delete
fn safe_action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        // Filter out Drag
        tool_strategy().prop_filter("No Drag", |t| *t != ToolState::Drag).prop_map(Action::SetTool),

        (-100.0..100.0f32, -100.0..100.0f32).prop_map(|(x, y)| Action::MoveMouse(Vec2::new(x, y))),

        prop_oneof![Just(MouseButton::Left), Just(MouseButton::Right)].prop_map(Action::MouseDown),
        prop_oneof![Just(MouseButton::Left), Just(MouseButton::Right)].prop_map(Action::MouseUp),

        // Filter out Delete keys
        key_strategy().prop_filter("No Delete", |k| *k != KeyCode::Delete && *k != KeyCode::Backspace).prop_map(Action::KeyPress),
        key_strategy().prop_filter("No Delete", |k| *k != KeyCode::Delete && *k != KeyCode::Backspace).prop_map(Action::KeyRelease),

        Just(Action::Update),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn prop_target_stability(actions in prop::collection::vec(safe_action_strategy(), 1..50)) {
        let mut app = create_test_app();

        let initial_pos = Vec2::new(10.0, 10.0);
        let target = app.world_mut().spawn((
            RigidBody::Dynamic, // It is dynamic, but without gravity/forces/drag it should stay?
            // Wait, if it's dynamic, gravity might pull it if gravity is enabled.
            // Rapier default gravity is (0, -9.81).
            // So "Stability" implies we should disable gravity or use Static/Kinematic for the test invariant?
            // Or just check it doesn't move *unexpectedly*.
            // The user said: "Most actions ( aside from the move tool ) should not cause the target object to change position either."
            // If the simulation runs (app.update), gravity applies.
            // So we should use GravityScale(0.0) or KinematicPositionBased to isolate tool effects from physics.
            GravityScale(0.0),
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(initial_pos.x, initial_pos.y, 0.0),
            GlobalTransform::default(),
            EditableBox { width: 2.0, height: 2.0 },
            Velocity::zero(),
            Damping { linear_damping: 0.0, angular_damping: 0.0 },
        )).id();

        app.update();

        // Let's settle any initial physics (should be none with 0 gravity and 0 velocity)
        for _ in 0..10 { app.update(); }

        // Capture stable position
        let stable_pos = app.world().get::<Transform>(target).unwrap().translation.truncate();

        for action in actions {
            apply_action(&mut app, &action);

            // Invariant: Target should exist
            let transform = app.world().get::<Transform>(target);
            if transform.is_none() {
                 // It disappeared! This is a failure unless we allowed deletion (which we didn't).
                 // However, Undo might despawn it if it was created by a command.
                 // But we spawned it manually. Undo shouldn't touch manual entities.
                 panic!("Target entity disappeared!");
            }

            // Invariant: Position should be close to stable_pos
            // We only check if the action was Update, because tools might change transform momentarily (e.g. snapping preview)
            // but the actual body position should remain.
            // Actually, if we are in the middle of a drag (which we excluded), it moves.
            // Since we excluded drag, it should not move.

            if let Action::Update = action {
                 let current_pos = transform.unwrap().translation.truncate();
                 // Using a small epsilon
                 if (current_pos - stable_pos).length_squared() > 0.001 {
                     panic!("Target entity moved! Old: {:?}, New: {:?}, Action History: ...", stable_pos, current_pos);
                 }
            }
        }
    }
}
