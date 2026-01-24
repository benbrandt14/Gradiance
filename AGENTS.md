# Instructions for Agents

This project, **Gradiance**, is a modern rewrite of the classic physics sandbox **Algodoo**, built with **Rust** and the **Bevy** game engine.

## Core Directives

1.  **Modern Algodoo Rewrite**: The ultimate goal is to replicate and improve upon Algodoo's features (lasers, thrusters, CSG, scripting) in a modern engine.
2.  **Test-Driven Development (TDD)**:
    *   **Tests First**: Whenever possible, write unit tests for logic before implementing the feature.
    *   **Massive Coverage**: Aim for high test coverage, especially for `GameCommand` implementations and tool logic.
    *   Use `rstest` for expressive testing fixtures.
3.  **Code Style**:
    *   Use updated information about Bevy 0.15.3 (https://docs.rs/bevy/0.15.3/bevy/)
    *   Use best practices in bevy & prioritize developer experience (DX).
    *   Keep systems small and focused, minimize indirection.
    *   Use `clippy` to ensure idiomatic Rust.
    *   If you are unsure, or direction is unclear, leave a comment. 
4.  **Architecture**:
    *   **ECS**: Strictly adhere to Bevy's Entity Component System.
    *   **Tools**: Implement tools as separate plugins managed by `ToolState`.
    *   **Commands**: All gameplay actions (creation, deletion, modification) MUST implement `GameCommand` to support Undo/Redo via `CommandStack`.
5.  **Progress Tracking**
    * See the detailed roadmap below, and the concise ROADMAP.md to keep track of high level goals and current status
    * Ensure all changes consider extensibility to future work, leave TODO's as appropriate to capture design assumptions
6.  **Strict Workflow: Continuous Documentation**
    * Before answering, read README.md and doc/schedule.dot.
    * doc/schedule.dot contains the authoritative ECS execution order. Use this to detect ordering conflicts.
    * The compiler denies builds with missing_docs. You must write docstrings for every public struct, enum, and function.
    * Edit src/lib.rs module docs for top content in the README

## Technical Context & Patterns

### 1. Physics (Rapier2d)
*   **Precision**: We use `f32` precision (Rapier2d default).
*   **Components**: Use `RigidBody`, `Collider` (from `bevy_rapier2d::prelude`).
*   **Joints**: Use `ImpulseJoint` (Revolute, Fixed, etc.) or `MultibodyJoint`.

### 2. Standard Tool Pattern
Tools should be implemented as separate plugins following this pattern:
```rust
pub struct MyToolPlugin;

impl Plugin for MyToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MyToolData>();
        app.add_systems(Update, my_tool_update.run_if(in_state(ToolState::MyTool)));
        app.add_systems(OnExit(ToolState::MyTool), my_tool_reset);
    }
}

// Data specific to the tool's operation (e.g., drag start point)
#[derive(Resource, Default)]
struct MyToolData { ... }

// Reset state when switching tools
fn my_tool_reset(mut data: ResMut<MyToolData>) { ... }

// Main update loop
fn my_tool_update(
    mut commands: Commands,
    mut data: ResMut<MyToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mut contexts: EguiContexts,
) {
    // 1. Block input if over UI
    if let Ok(ctx) = contexts.ctx_mut() && ctx.is_pointer_over_area() { return; }

    // 2. Get cursor position
    let Some(pos) = cursor_pos.0 else { return; };

    // 3. Handle Input (Mouse/Keyboard)
    // 4. Update Visuals (Gizmos)
    // 5. Apply Changes (Commands - NOT direct mutations)
    // commands.queue(move |world: &mut World| { ... });
}
```

### 3. Rendering (Bevy Prototype Lyon - Disabled)
*   **Status**: `bevy_prototype_lyon` is currently **disabled** due to compatibility issues with Bevy 0.15.3.
*   **Workaround**: Use `Sprite` components or Bevy `Gizmos` for visualization until this is resolved.

## Future Roadmap (Design Goals)

Phase 1: The Substrate (Months 1–2)

Goal: A stable infinite 2D world where you can spawn rigid bodies and move the camera.

Milestones:

    Core scaffolding: Bevy 0.15.3 app structure with Rapier2d.

    Input/Camera: Pan/Zoom camera and Mouse Picking.

    The Floor: Infinite plane implementation.

Tasks:

    [ ] Initialize project with bevy, Rapier2d, bevy_egui.

    [ ] Implement Camera2dBundle with custom Pan/Zoom system (Orthographic scale).

        Constraint: Zoom must center on mouse cursor.

    [ ] Implement InfiniteFloor system:

        Spawn Collider::half_space at y=0.

        Render infinite grid using a custom shader (or bevy_infinite_grid fork) that scales lines based on zoom level.

    [ ] Implement Selection resource using bevy_mod_picking:

        Click to select single.

        Shift+Click to multi-select.

        Drag background to box-select (AABB query).

    [ ] Implement MouseJoint (The "Hand" Tool):

        On drag start: Spawn DistanceJoint (high stiffness) between cursor and object.

        On drag update: Update joint anchor to mouse position.

Phase 2: Geometry & CSG Pipeline (Months 3–4)

Goal: The "Sketch" and "Cut" tools. Creating custom shapes and slicing them.

Milestones:

    Vector Rendering: High-fidelity shape drawing.

    CSG Kernel: Boolean operations (Cut, Weld).

    Tessellation: Polygon to Mesh/Collider.

Tasks:

    [ ] Integrate bevy_prototype_lyon (When compatible):

        Create VectorMesh component (holds path data separate from Bevy Mesh).

    [ ] Implement Sketch Tool:

        Capture points -> Ramer-Douglas-Peucker simplification -> Lyon Path.

    [ ] Integrate clipper2 crate:

        Implement utility fn to_clipper_path(Vec<Vec2>) -> Vec<Point64>.

        Implement scaling factor (105) to handle float-to-int conversion.

    [ ] Implement Cut Tool (Laser):

        Input: Line segment A→B.

        Expand line to thin polygon Pcut​.

        Query intersecting bodies.

        Perform Difference(Body, P_{cut}).

        Despawn old body, spawn new bodies.

        Crucial: Calculate new Center of Mass and offset children/collider/mesh to local (0,0).

    [ ] Implement Polygon Decomposition:

        Use parry2d's convex decomposition on resulting shapes to generate valid Rapier colliders.

Phase 3: The Mechanical Engineer (Months 5–6)

Goal: Advanced constraints. Gears, Pulleys, and Linkages.

Milestones:

    Standard Joints: Hinge, Fixed, Spring.

    Custom Solvers: Gear and Pulley constraints (Math-heavy phase).

    Visualization: Seeing the joints.

Tasks:

    [ ] Implement Hinge Tool:

        Spawn RevoluteJoint.

        Visuals: Draw a "Bolt" sprite at anchor.

    [ ] Implement Spring/Slider Tool:

        Spawn PrismaticJoint (Slider) or DistanceJoint (Spring).

        UI: Sliders for Stiffness, Damping.

    [ ] Custom Constraint: Gear Joint:

        Implement GeneralizedConstraint trait in Rapier (if supported) or manually via forces.

        Constraint Eq: ΔθA​+rΔθB​=0.

        Add Backlash parameter (dead zone before constraint engages).

    [ ] Custom Constraint: Pulley:

        Implement "Rope" logic: Distance(A, AnchorA) + Distance(B, AnchorB) <= Length.

        Visuals: Draw lines from A→AnchorA→AnchorB→B.

        Stretch: Allow wire to wrap around circular obstacles (requires Raycast/ConvexCast wrapping algorithm).

    [ ] Chain Tool:

        Procedural generation: Spawn N small capsule bodies linked by RevoluteJoints.

        Tune SolverIterations (substeps) to preventing chain explosion.

Phase 4: Simulation & Scripting (Months 7–8)

Goal: Fluids, Lasers, and Scripting terminal.

Milestones:

    Fluids: SPH Implementation.

    Optics: Lasers and refraction.

    Scripting: Lua integration.

Tasks:

    [ ] SPH Fluid System (CPU):

        Implement Spatial Hash Grid resource.

        Particle Entity: Position, Velocity, FluidParticle.

        Solver: Density constraint ρi​=∑W(rij​). Pressure force −∇p.

        Coupling: Raycast from particles to Rapier bodies. Apply impulse to body; reflect particle.

    [ ] Laser/Optics System:

        Recursive Raycast system.

        Materials: RefractiveIndex, Reflectivity.

        Visuals: Bloom mesh using bevy_firefly.

    [ ] Scripting Integration:

        Integrate bevy_mod_scripting with Lua.

        Create "Half-Life" style console using bevy_egui.

        Bind Entity queries to Lua (e.g., scene.get("box1").color = "red").

        Implement ScriptComponent that runs on_update(dt) hook.

Phase 5: The "Algodoo" Polish (Months 9–10)

Goal: UI, Plotting, and Quality of Life.

Milestones:

    Material Manager: Grouping attributes.

    Property Inspector: The main UI.

    Graphing: Real-time plots.

Tasks:

    [ ] Inspector UI (bevy_egui):

        Selection-driven panel.

        Reflection-based auto-UI for components (Friction, Restitution, Mass).

        Color picker (rgba).

    [ ] Material Profiles:

        Resource MaterialLibrary (Gold, Ice, Rubber).

        Applying profile batch-updates components on selected entities.

    [ ] Plotting/Tracers:

        Component Tracer { duration, color }.

        System: Record position history, render using bevy_polyline.

        Mini-graph UI: Plot velocity magnitude vs time for selected object.

Phase 6: Extension & Release (Months 11–12)

Goal: Save/Load, Lighting, Optimization.

Milestones:

    Persistence: Serialization.

    Lighting: 2D Shadows.

    Optimization: Multithreading.

Tasks:

    [ ] Save/Load:

        Integrate moonshine_save.

        Serialize Rapier components and Lyon paths to RON.

        Handle Entity ID remapping for Joints.

    [ ] Lighting:

        Integrate bevy_firefly.

        Add ShadowCaster component to all RigidBodies.

    [ ] Fracturing (Beta):

        System: On CollisionEvent, if Impulse > Threshold:

        Trigger Voronoi shatter (using delaunator or similar) -> Replace body with shards.

## Development Environment

*   Use `cargo test` to run tests.
*   Use `cargo clippy` for linting (slow).
*   Use `cargo fmt` for formatting (fast).
*   Use `cargo add` / `cargo remove` / `cargo update` to modify packages, do not touch the `Cargo.lock` or `Cargo.toml`.

## Bevy Best Practices

```
Entities
Name and Cleanup

All top-level entities must be spawned with a Name and cleanup component at the front of the bundle. It's expected that child entities don't need a cleanup component as it will be handled by the parent.

Names assist with debugging. Cleanup components indicate to which state the entity belongs, and to remove it upon exit of that state.

By always having these two "meta" components at the front it makes it easy to spot entities where they are missing.

commands
    .spawn((
        Name::new("Player"),
        cleanup::CleanupInGamePlayingExit,
        ...
    ))

As of bevy 0.14 you can now use StateScoped components which fulfill a similar role.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, States)]
enum GameState {
    #[default]
    MainMenu,
    SettingsMenu,
    InGame,
}

fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Name::new("Player"),
        StateScoped(GameState::InGame),
        ...
    ));
}

app.init_state::<GameState>();
app.enable_state_scoped_entities::<GameState>();

You can read more about the cleanup pattern I'm using in the bevy cheatbook.
Strong IDs

For things in your game that should persist between saving/loading, and networking, use your own ID type over Entity.

Entity is more akin to a pointer, it is not to be relied upon for referencing something across sessions or over the network.

Here is an example of making a strong ID type for quests.

Keeping the quest generator resource and the actual u32 value of the QuestId private to the module means quest ids can only be generated in one place, which helps with simplicity and debugging.

fn my_sys(mut qgs: ResMut<QuestGlobalState>, mut cmd: Commands) {
    let quest_id = qgs.quest_id();
    cmd.spawn(
        ...
        MyQuest {
            id: quest_id
        },
    );
}

#[derive(Reflect, Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) struct QuestId(u32);

impl std::fmt::Display for QuestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("q_")?;
        self.0.fmt(f)
    }
}

#[derive(Resource, Debug)]
struct QuestGlobalState {
    next_quest_id: u32,
}

impl QuestGlobalState {
    fn new() -> Self {
        Self { next_quest_id: 0 }
    }

    fn quest_id(&mut self) -> QuestId {
        let id = self.next_quest_id;
        self.next_quest_id += 1;
        QuestId(id)
    }
}

System Scheduling
Update systems should be bounded

All systems added to the Update schedule should be bound by run conditions on State and SystemSet.

Run states enables easy enabling/disabling of groups of behaviour & reduces systems running when they don't need to.

For example, changing PlayingState to PlayingState::Paused will automatically disable all systems that progress the game and enable systems handle actions related to the pause menu.

System sets force coarse grained ordering leading to predictable behaviour between different parts of the game.

There can be exceptions to this, for example you may have background music or UI animations that should continue in both Playing and Paused.

.add_systems(
    Update,
    (handle_player_input, camera_view_update)
        .chain()
        .run_if(in_state(PlayingState::Playing))
        .run_if(in_state(GameState::InGame))
        .in_set(UpdateSet::Player),
)

Co-locate system registration for the same State

State transitions should have setup and cleanup specific systems. Their OnEnter and OnExit registration should be co-located.

This means it's easy to see the setup systems and that it has a cleanup system to run.

.add_systems(OnEnter(GameState::MainMenu), main_menu_setup)
.add_systems(OnExit(GameState::MainMenu), cleanup_system::<CleanupMenuClose>)
.add_systems(Update, (...).run_if(in_state(GameState::MainMenu)))

Events
Prefer Events for structuring logic and systems

Use Events to structure logic and communication between subsystems of your game.

Events allow different parts of your game to opt-in to information they need, and prevents tight coupling.

EventWriters and EventReaders are implemented as a thin layer over a vanilla Vec so they're cheap to use, and subsequent sends will re-use the allocated capacity. Systems that read from the same event can also run in parallel as EventReaders are local to the system.

Here's an example of one way you might structure a projectile hitting an enemy, all the way to audio, visual effects, and achievements being updated. You should evaluate the use of events on a case by case basis, as they're not free, and for simple local operations it can be enough to mutate within the same system.
Explicit ordering

Event readers should be ordered after their respective writers within the frame. Undefined ordering between writers and readers can lead to subtle out of order bugs. Delaying communication across frames is often not intentionally desired. If it is something you want it should be made explicit.

There are exceptions for systems like achievements or analytics, but I'd only recommend excluding them from ordering if you have a good reason. Often they will not be computationally intense, so having them all run at the end of frame is fine.

You can achieve this by using event_producer.before(event_consumer) or (event_producer, event_consumer).chain() when adding systems for systems within the same SystemSet. For events that cross a SystemSet boundary this should be taken care of by the ordering of the SystemSets in your app.configure_sets() call.
Explicit event handling system run criteria

Systems that only do work based on an event should have that as part of their run condition.

fn handle_player_level_up_event(mut events: EventReader<PlayerLevelUpEvent>) {
    events.iter().for_each(|e| {
        // ...
    });
}

handle_player_level_up_event.run_if(on_event::<PlayerLevelUpEvent>())

Helpers

Write helper utilities for common operations
Cleanup

Tag entities with a cleanup Zero Sized Type (ZST) component. We can then add our cleanup utility system with our new cleanup component as the type. This creates a simple and consistent way to remove all entities marked with the component when transitioning or exiting certain states.

#[derive(Component)]
struct CleanupInGamePlayingExit;

fn cleanup_system<T: Component>(mut commands: Commands, q: Query<Entity, With<T>>) {
    q.for_each(|e| {
        commands.entity(e).despawn_recursive();
    });
}

// When spawning entities
commands.spawn((
    Name::new("projectile"),
    CleanupInGamePlayingExit,
    ...
));

// Add to state transition
.add_systems(
    OnExit(GameState::InGame),
    cleanup_system::<CleanupInGamePlayingExit>,
)

Credit to bevy cheatbook.
Getter Macros

When working with queries and the Entity type, often you'll be matching on the outcome to exit early if the entity was not found.

The tedium of writing match expressions all over the place to return early can be avoided through a few simple macros. I've provided a one but you can imagine more variations based on the methods on Query.

Do be careful when using these, as opposed to the panicing methods like query.single(), these will silently return. This may be appropriate for your game, however it could also lead to bugs and unusual behaviour if they were supposed to succeed.

You could even make variations of these that return in release but panic in debug if that fits your use case.

fn print_window_size(windows: Query<&Window>) {
    let window = get_single!(windows);
    println!("Window Size: {}", window.resolution);
}

#[macro_export]
macro_rules! get_single {
    ($q:expr) => {
        match $q.get_single() {
            Ok(m) => m,
            _ => return,
        }
    };
}

Project Structuring
Prelude

Bevy utlises a prelude module to great effect for easy access to common imports. We can do the same!

By creating a prelude module in our project and exporting the various types that are commonly used within our game we can greatly cut down on the number of imports we need to maintain. You can also bring in the preludes from commonly used dependencies if you like. I have done that here with bevy and rand.

A nice side effect of this pattern is moving around code or refactoring doesn't require changes in as many places. If you restructure your audio code, you only need to update how it's presented in the prelude, assuming the rest of your project utilises the prelude.

src/audio.rs

pub(crate) mod prelude {
  pub(crate) use super::{EventPlaySFX, SFXKind};
}

#[derive(Event)]
pub(crate) struct EventPlaySFX { /* ... */ }
pub(crate) enum SFXKind { /* ... */ }

src/prelude.rs

pub(crate) use bevy::prelude::*;
pub(crate) use rand::prelude::*;

// Common items available at the root of the prelude
pub(crate) use crate::{Enemy, Health};

// Specific areas nested within their own module for self documenting use
pub(crate) mod audio {
    pub(crate) use crate::audio::prelude::*;
}

pub(crate) mod physics { /* ... */ }

src/enemy.rs

use crate::prelude::*;

fn handle_enemy_health_changed(
    mut commands: Commands,
    enemies: Query<(&Health, Entity), (With<Enemy>, Changed<Health>)>,
    mut play_sfx: EventWriter<audio::EventPlaySFX>,
) {
    enemies.for_each(|(health, id)| {
        if health.current <= 0. {
            commands.entity(id).despawn_recursive();
            play_sfx.send(audio::EventPlaySFX::new(audio::SFXKind::EnemyDeath));
        }
    });
}

Plugins

Bevy Plugins enable grouping systems, components, and resources into logical units. They're used heavily in Bevy itself and are what powers the ability to turn on/off parts of the engine.

By constructing your game out of plugins you make it easier to find, work with, and debug subsystems. It also contextualises the setup and configuration of 3rd party crates to where they belong. For example, setting up the resources, plugins, and systems to utilise a 3rd party terrain library would go in your TerrainPlugin. That way, disabling your own terrain plugin will also disable the library you've imported, and any other resources that only it needed.

Note that while internal plugins and binaries use a simple function as a plugin, library authors are expected to expose a struct implementing Plugin instead. The reason is that this way, authors can add internal state like configuration to the plugin in the future without breaking changes.

    Note

    Your mileage may vary with "enabling"/"disabling" plugins in your game. Bevy implements it in engine because it's valuable to disable chunks of the engine. However to achieve this in the game itself is not only more difficult, but the payoff is lower. How often will you realistically want to remove physics or audio from your game?

src/audio.rs

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(some_audio_library::AudioFXPlugin)
        .init_resource::<MyAudioSettings>()
        .add_systems(...);
    }
}

src/physics.rs

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(some_physics_library::BouncyPhysicsPlugin)
        .init_resource::<MyPhysicsSettings>()
        .add_systems(...);
}

src/game.rs

pub(super) fn plugin(app: &mut App) {
    app.add_plugins((
      DefaultPlugins,
      crate::audio::plugin,
      crate::physics::plugin,
    ));
}

src/main.rs

fn main() {
    bevy::prelude::App::new()
        .add_plugins(crate::game::plugin)
        .run();
}
```
