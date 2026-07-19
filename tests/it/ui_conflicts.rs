//! Guards against B0002 system-parameter conflicts in the egui panel
//! systems. These systems only run under the render plugin, so the headless
//! integration tests never initialize them — a conflicting-access bug (e.g.
//! two `MessageWriter`s of the same intent in one system) would panic only
//! at runtime. `System::initialize` runs Bevy's access-conflict validation
//! without a window, catching that class here.

use bevy::ecs::system::IntoSystem;
use bevy::prelude::*;

fn assert_initializes<M, S: IntoSystem<(), Result, M>>(system: S) {
    let mut world = World::new();
    let mut system = IntoSystem::into_system(system);
    system.initialize(&mut world);
}

#[test]
fn panel_systems_have_no_conflicting_params() {
    assert_initializes(gradiance::ui::dock::right_dock);
    // The bottom dock unions the node-graph and plot read-sets in one system —
    // guard that large param set against access conflicts.
    assert_initializes(gradiance::ui::bottom_dock::bottom_dock);
    assert_initializes(gradiance::ui::toolbar::toolbar);
    assert_initializes(gradiance::ui::menu::menu_bar);
    // BodyProps gained a read facade (PhysicsQueries + transforms) used by
    // both hosts — guard both against param conflicts.
    assert_initializes(gradiance::ui::inspector::inspector_window);
    assert_initializes(gradiance::ui::context_menu::context_menu);
    // joint_inspector gained transform/index reads for its sensor readouts.
    assert_initializes(gradiance::ui::joint_inspector::joint_inspector);
    // apply_scene_viewport mutates the camera viewport from the panel rects.
    assert_initializes(gradiance::ui::apply_scene_viewport);
}
