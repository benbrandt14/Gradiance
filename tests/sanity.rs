use bevy::prelude::*;

#[test]
fn test_minimal_app() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.update();
    assert!(true);
}
