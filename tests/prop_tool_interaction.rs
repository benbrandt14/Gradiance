use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::input::ToolState;
use gradiance::input::editable_shape::{EditableShape, ShapeType};
use proptest::prelude::*;

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
            EditableShape {
                shape: ShapeType::Box {
                    width: 10.0,
                    height: 10.0,
                },
            },
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

