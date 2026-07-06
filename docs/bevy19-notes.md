# Bevy 0.19 / Avian2d 0.7 / bevy_egui 0.41 — API notes

**Read this before writing any engine-facing code.** Model knowledge typically ends around
Bevy 0.15/0.16; every fact below was verified against the exact pinned crate sources in
`~/.cargo/registry` (rustc ≥ 1.95 required — this repo builds on 1.96). When in doubt,
grep the crate source, not memory. Items marked ⚠ were not fully verified.

## Bevy ECS: Messages vs Events (the big rename)

Buffered "events" are now **Messages**; `Event` is exclusively for observers.

```rust
#[derive(Message)] struct SpawnBodyIntent { /* .. */ }
app.add_message::<SpawnBodyIntent>();
fn producer(mut w: MessageWriter<SpawnBodyIntent>) { w.write(SpawnBodyIntent { .. }); }
fn consumer(mut r: MessageReader<SpawnBodyIntent>) { for m in r.read() { .. } }
// World-level (exclusive systems): world.write_message(m); world.resource_mut::<Messages<M>>().drain()
```

Observer events:

```rust
#[derive(Event)] struct GlobalThing;                 // world-scoped
#[derive(EntityEvent)] struct Zap(Entity);           // entity-targeted; first field = target
                                                     // (or name a field `entity`)
app.add_observer(|e: On<Zap>, mut q: Query<..>| { .. });     // global observer
world.entity_mut(e).observe(|e: On<Zap>| { .. });            // per-entity observer
// On<'w,'t,E,B> derefs to E; e.entity / the target field gives the entity.
// world.trigger(Zap(entity)) fires it.
```

## Component derive, hooks, required components

```rust
#[derive(Component)]
#[require(Mesh3d, MeshMaterial3d<StandardMaterial>)]   // required components, unchanged
#[component(on_add = my_hook, on_remove = my_unhook)]
struct Thing;
// Hook signature CHANGED: fn(DeferredWorld, HookContext)
// HookContext path: bevy::ecs::lifecycle::HookContext (NOT ecs::component)
fn my_hook(mut world: DeferredWorld, ctx: HookContext) {
    let entity = ctx.entity;    // HookContext { entity, component_id, caller, relationship_hook_mode }
}
// Derive-supported hook keys: on_add, on_insert, on_discard (≈old on_replace:
// value about to be dropped on replace/remove), on_remove (removal AND despawn),
// on_despawn, immutable, clone_behavior, map_entities. NO `on_replace` key.
// `immutable` forbids &mut queries — right for index-backed components like StableId.
```

## Plugin groups

`bevy::app::plugin_group!` requires every member plugin to implement `Default`
(members are constructed via `::default()`). Syntax: `crate::path:::PluginName`
(triple colon between module path and type).

## States (bevy_state)

```rust
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
enum GameState { #[default] Playing, Paused }
app.init_state::<GameState>();
// run conditions: in_state(GameState::Playing)
// transitions: ResMut<NextState<GameState>> → next.set(GameState::Paused)
// StateScoped is GONE → use DespawnOnExit(GameState::Playing) component
// (DespawnOnEnter also exists). No enable_state_scoped_entities needed.
```

## Systems

- **Fallible systems are idiomatic**: systems may return `bevy::ecs::error::Result`
  (`fn sys(..) -> Result`) and use `?`. Default error handler panics; fine for setup,
  prefer graceful handling in hot paths.
- Exclusive systems `fn(world: &mut World)` unchanged. Schedules PreUpdate/Update/
  PostUpdate/FixedUpdate unchanged. `.add_systems(Update, (a, b).chain().run_if(..))` unchanged.
- Hierarchy: `ChildOf(parent)` relationship component + `Children`;
  `commands.entity(e).despawn()` **despawns descendants by default** (despawn_recursive is gone);
  spawn children via `commands.spawn((.., children![..]))` or `.add_child(e)`.
- `Single<&mut T, F>` system param: panics-free "exactly one" query (system skips if not exactly 1).

## Picking (first-party bevy_picking)

`Pointer<E>` is a `Message` **and** an `EntityEvent` (field `entity` = target), auto-propagating
to parents via `ChildOf` and finally to the pointer's **Window** entity — so a window observer
sees unconsumed pointer events (empty-canvas clicks).

- Event payloads: `Pointer<Press> { button, hit: HitData, count }`, `Release`,
  `Click { button, hit, duration, count }`, `Move { hit, delta }`,
  `DragStart { button, hit }`, `Drag { button, distance, delta }`, `DragEnd { button, distance }`,
  `DragEnter/DragOver/DragDrop { dragged, .. }`, `Over/Out { hit }`, `Enter/Leave` (no propagate).
- `HitData { camera: Entity, depth: f32, position: Option<Vec3>, normal: Option<Vec3> }`
  — position space is backend-defined (avian gives world space ⚠ verify at M5).
- **Drag `distance`/`delta` are in screen pixels** (Y down!) — convert via camera for world-space.
- Attach: `app.add_observer(fn(On<Pointer<Click>>, ..))` or `entity.observe(..)`.
- `Pickable` component controls behavior (`should_block_lower`, `is_hoverable`); not required
  for backends to pick an entity, only to override defaults.

## Avian2d 0.7

```rust
app.add_plugins(PhysicsPlugins::default().with_length_unit(100.0));  // PhysicsLengthUnit resource
app.insert_resource(Gravity(Vector::NEG_Y * 1000.0));                // Vector = Vec2 (f32 default)
```

- Bodies: `RigidBody::{Dynamic, Static, Kinematic}` component. Position is driven through
  `Transform` (a `Position`/`Rotation` pair exists and syncs both ways; writing `Transform`
  teleports). Velocities: `LinearVelocity(Vec2)`, `AngularVelocity(f32)` — read/write directly.
- Colliders: `Collider::circle(r)`, `Collider::rectangle(w, h)`, `Collider::triangle(a,b,c)`,
  `Collider::half_space(normal)`, `Collider::capsule(r, len)`,
  `Collider::convex_decomposition(vertices: Vec<Vector>, indices: Vec<[u32;2]>)` (loop indices),
  `Collider::convex_hull(points) -> Option<Self>` ⚠ verify signature.
- Materials/props: `Friction::new(f)`, `Restitution::new(r)`, `ColliderDensity(d)`,
  `GravityScale(s)`, `Sensor`, `LockedAxes::ROTATION_LOCKED`, `Mass(m)`, `SleepingDisabled`.
- Layers: `CollisionLayers { memberships: LayerMask, filters: LayerMask }`;
  `CollisionLayers::from_bits(u32, u32)` is const — maps 1:1 from our `LayerMask32`.
- Pause/speed: `Time<Physics>` + `PhysicsTime` trait: `.pause()`, `.unpause()`, `.is_paused()`,
  `.set_relative_speed(f32)`.
- **Joints are entities with components** referencing two bodies:
  ```rust
  commands.spawn(
      RevoluteJoint::new(e1, e2)
          .with_local_anchor1(Vec2)      // JointFrame anchors; frame1/frame2 fields
          .with_local_anchor2(Vec2)
          .with_angle_limits(min, max)
          .with_motor(AngularMotor { enabled, target_velocity, target_position, max_torque,
                                     motor_model: MotorModel::AccelerationBased { stiffness, damping }
                                     /* or MotorModel::SpringDamper { frequency, damping_ratio } */,
                                     ..default() }),
  );
  // PrismaticJoint::new(e1, e2).with_slider_axis(Vector::Y).with_limits(lo, hi)
  //     .with_motor(LinearMotor::new(model).with_target_position(x).with_max_force(f))
  // FixedJoint::new(e1, e2)  ⚠ verify anchor/rotation setters in src/dynamics/joints/fixed.rs
  // Mutate live: query &mut RevoluteJoint, edit joint.motor.target_velocity etc.
  // Extras: JointDisabled, JointCollisionDisabled (disables contacts between the two bodies).
  ```
- Spatial queries: `SpatialQuery` system param —
  `point_intersections(Vector, &SpatialQueryFilter) -> Vec<Entity>`,
  `aabb_intersections_with_aabb(ColliderAabb) -> Vec<Entity>`, `cast_ray(..)`, shape casts.
- Picking backend: add `PhysicsPickingPlugin`, put `PhysicsPickable` on the **camera** and on
  target entities (or set `PhysicsPickingSettings::require_markers = false` — default picks all ⚠
  check `picking/mod.rs`: by default all colliders pickable unless `require_markers` set).
- Determinism escape hatch: `enhanced-determinism` feature.

## bevy_egui 0.41 (egui 0.35)

```rust
app.add_plugins(EguiPlugin::default());
app.add_systems(EguiPrimaryContextPass, my_ui);                 // dedicated schedule!
fn my_ui(mut contexts: EguiContexts) -> Result {                // fallible system
    let ctx = contexts.ctx_mut()?;                              // Result, not panic
    egui::Panel::left("l").resizable(true).show(ctx, |ui| ..);  // Panel::left/right/top/bottom
    egui::Window::new("w").show(ctx, |ui| ..);                  // (SidePanel/TopBottomPanel renamed)
    Ok(())
}
// Pointer-over-UI: ctx.wants_pointer_input() / ctx.wants_keyboard_input() (egui 0.35 methods)
// Multi-window: PrimaryEguiContext marker; EguiGlobalSettings resource.
```

## Rendering

- Camera: `commands.spawn((Camera3d::default(), Transform::from_xyz(..).looking_at(..)))`;
  `Projection::Orthographic(OrthographicProjection { .. })` ⚠ verify ortho fields at M3.
- `Camera::viewport_to_world_2d(&GlobalTransform, Vec2) -> Result<Vec2, ViewportConversionError>`.
- Light: `DirectionalLight { color, illuminance, shadow_maps_enabled: true, .. }` + Transform
  (`shadows_enabled` was RENAMED; there's also `contact_shadows_enabled`).
  Ambient: `GlobalAmbientLight { color, brightness, .. }` **resource**; `AmbientLight` is now a
  per-camera *component* (requires Camera).
- Mesh: `Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())`,
  `.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec<[f32;3]>)`, `ATTRIBUTE_NORMAL`, `ATTRIBUTE_UV_0`,
  `.insert_indices(Indices::U32(v))`. Components: `Mesh3d(Handle<Mesh>)`,
  `MeshMaterial3d(Handle<M>)`. Built-in primitive meshing includes `Extrusion<P>`
  (bevy_mesh/src/primitives/extrusion.rs) — `Extrusion::new(Rectangle::new(w,h), depth)` meshes
  a prism; useful for Box/Circle bodies (⚠ check whether extrusion is centered on Z at M3).
- Custom material: `trait Material: Asset + AsBindGroup + Clone` with `fn fragment_shader() -> ShaderRef`
  — same shape as 0.15; register `MaterialPlugin::<MyMat>::default()`.
- Gizmos: immediate-mode `Gizmos` system param: `.line_2d(a, b, color)`, `.rect_2d(..)`,
  `.circle_2d(..)` — unchanged in spirit.

## clipper2 0.6

- `Paths<P: PointScaler>` / `Point<P>` — scaling is a **type parameter** (`Centi` = ×100 default;
  we want 5 decimals → define a custom `PointScaler` with `MULTIPLIER = 100_000.0` ⚠ verify trait).
- Free fns in `clipper2::*`: `difference(subject, clip, FillRule) -> Result<Paths>`,
  `union`, `intersect`; `Paths::simplify(epsilon, is_open)`.
- FFI (bundled C via clipper2c-sys) — needs a C compiler; pure-Rust fallback crate: `i_overlay 7`.

## lyon 1.0.19

Unchanged from prior experience: `lyon::path::Path` builder, `FillTessellator::tessellate_path`
(or `tessellate` with iterator), `VertexBuffers` + `BuffersBuilder`, `FillOptions::tolerance(..)`.

## Headless test apps

- Call `app.finish(); app.cleanup();` after adding plugins and before the first
  `app.update()` — plugin `finish()` hooks (where avian registers diagnostics
  resources) otherwise never run and systems fail parameter validation.
- Insert `TimeUpdateStrategy::ManualDuration(frame)` for deterministic stepping.
- `PhysicsPickingPlugin` requires bevy's core `PickingPlugin` (absent headless);
  gate it with `app.is_plugin_added::<bevy::picking::PickingPlugin>()`.

## Gotchas checklist

- `StateScoped` → `DespawnOnExit`. `add_event` → `add_message`. `EventReader.send` → `MessageWriter.write`.
- `despawn()` is recursive now; there is no `despawn_recursive()`.
- egui systems go in `EguiPrimaryContextPass`, and `ctx_mut()` returns `Result`.
- Pointer drag deltas are screen-space (Y inverted vs world).
- avian `Scalar`/`Vector` = f32/Vec2 under default `parry-f32`.
- rustc ≥ 1.95 required by bevy 0.19 (container toolchain updated to 1.96.1).

## Custom materials (verified against bevy_pbr 0.19 sources, M10)

- Extend `StandardMaterial` instead of replacing it:
  `ExtendedMaterial<StandardMaterial, MyExtension>` + `MaterialPlugin::<That>::default()`.
  The extension derives `Asset, AsBindGroup, TypePath, Clone` and implements
  `MaterialExtension` (`fragment_shader() -> ShaderRef`); base-material shadow
  maps, clustered lights, and the prepass keep working untouched.
- Extension bind-group entries must start at `@binding(100)` to avoid clashing
  with `StandardMaterial`'s bindings; in WGSL the material group index is the
  preprocessor def `#{MATERIAL_BIND_GROUP}`.
- Fragment recipe (from `pbr_fragment.wgsl` / `pbr_functions.wgsl`):
  `pbr_input_from_standard_material(in, is_front)` → `alpha_discard` →
  `apply_pbr_lighting(pbr_input)` → tweak → `main_pass_post_lighting_processing`.
  `PbrInput` exposes `N` and `V` for rim/fresnel terms.
- Ship shaders inside the binary with `bevy::asset::embedded_asset!(app, "file.wgsl")`
  (path relative to the calling file, `src/` stripped) and reference them as
  `"embedded://<crate>/<dir>/file.wgsl"`. `ShaderRef: From<&'static str>`.
- `bevy_shader` is re-exported as `bevy::shader` (home of `ShaderRef`).
