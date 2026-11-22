//! Right-click context menu: grouping, pick-from-stack, and collision
//! layer operations.

use crate::joint_inspector::kind_name;
use crate::labels::{body_label, object_label};
use crate::widgets;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use gradiance_command::intent::{CommitTransformIntent, PropertyEditIntent};
use gradiance_command::property::{PropertyChange, PropertyValue};
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_core::units::PosRot;
use gradiance_domain::Body;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::joint::{JointDef, JointKind};
use gradiance_domain::node::{BehaviorNode, NodeKind};
use gradiance_domain::shape::ShapeDef;
use gradiance_interaction::PointerOverUi;
use gradiance_interaction::align::{AlignItem, AlignOp, align_changes};
use gradiance_interaction::cursor::CursorWorldPos;
use gradiance_interaction::input::{EditorAction, key_invocations};
use gradiance_interaction::joint_edit::ANCHOR_PICK_PX;
use gradiance_interaction::pointer::{PointerButtons, RIGHT_CLICK_DEADZONE_PX};
use gradiance_interaction::selection::{SelectTransition, SelectedJoint, Selection};
use gradiance_interaction::tools::bodies_at_sorted;
use gradiance_op::{InvokeOp, json};
use gradiance_physics::queries::PhysicsQueries;
use gradiance_script::actions::builtin;
use gradiance_script::bridge::{ScriptActions, ScriptInputs};

/// Action-table access, bundled into one `SystemParam` to keep `context_menu`
/// under Bevy's system-parameter count limit. Reads the action table (built-in
/// seed + user entries) and submits an invoked entry, with the click point,
/// through the shared `ScriptInputs` queue.
#[derive(SystemParam)]
pub struct ScriptMenu<'w> {
    actions: Res<'w, ScriptActions>,
    inputs: ResMut<'w, ScriptInputs>,
    labels: Res<'w, gradiance_script::bridge::WorkspaceLabels>,
}

/// Background / scene actions: per-scene settings writes (the Config
/// seam) — friction against the back plane, the top-down toggle,
/// scenery-plane visibility, and the grid's user coordinate frame.
#[derive(SystemParam)]
pub struct BackgroundMenu<'w> {
    sim: ResMut<'w, gradiance_domain::settings::SimSettings>,
    scenery: ResMut<'w, gradiance_domain::settings::ScenerySettings>,
    grid: ResMut<'w, gradiance_domain::settings::GridSettings>,
    settings_window: ResMut<'w, crate::settings::SettingsWindow>,
}

/// The built-in actions the menu renders, all in the background section.
/// Every built-in in the action table's seed appears here and nothing else
/// does (pinned by `every_builtin_has_a_menu_place`), so the menu and the
/// table cannot drift.
const BACKGROUND_BUILTINS: [&str; 2] = [builtin::RESTORE_GRAVITY, builtin::TOP_DOWN];

/// Whether a background built-in is offered right now: the two gravity
/// entries are alternatives, keyed on whether gravity is already off.
fn background_builtin_shown(name: &str, top_down: bool) -> bool {
    match name {
        builtin::RESTORE_GRAVITY => top_down,
        builtin::TOP_DOWN => !top_down,
        _ => false,
    }
}

impl BackgroundMenu<'_> {
    /// Renders the empty-canvas section; returns true when the menu should
    /// close. The built-in entries come from the action table and are invoked
    /// at `at` (the click point) like any action.
    fn section(&mut self, ui: &mut egui::Ui, script: &mut ScriptMenu, at: Vec2) -> bool {
        let mut close = false;
        ui.label(egui::RichText::new("Background").weak());
        ui.horizontal(|ui| {
            ui.label("plane friction");
            widgets::config_field(
                gradiance_domain::settings::SimSettings::default().plane_friction,
                0.01,
            )
            .range(0.0..=5.0)
            .show(ui, &mut self.sim.plane_friction)
            .on_hover_text("top-down mode: bodies rub on the back plane (0 = off)");
        });
        let top_down = self.sim.gravity.value().length() < 1e-3;
        for action in script.actions.builtins() {
            let Some(name) = action.builtin else { continue };
            if !BACKGROUND_BUILTINS.contains(&name) || !background_builtin_shown(name, top_down) {
                continue;
            }
            if row(ui, &action.label, &action.hint) {
                script.inputs.invoke(action, at);
                close = true;
            }
        }
        let scenery = self.scenery.bypass_change_detection();
        let mut touched = ui
            .checkbox(&mut scenery.back_visible, "Back plane")
            .on_hover_text("the shadow catcher behind the deepest layer")
            .changed();
        touched |= ui
            .checkbox(&mut scenery.ground_visible, "Ground planes")
            .on_hover_text("the infinite floors under half-plane bodies")
            .changed();
        if touched {
            self.scenery.set_changed();
        }
        // Reset the grid's user coordinate system to world axes (undoes an
        // "align grid to body").
        if (self.grid.origin.value() != Vec2::ZERO || self.grid.rotation.value() != 0.0)
            && row(ui, "Reset grid to world", "clear a local-frame alignment")
        {
            self.grid.origin = gradiance_units::Displacement(Vec2::ZERO);
            self.grid.rotation = gradiance_units::Angle(0.0);
            close = true;
        }
        if row(ui, "Scene & lighting settings…", "") {
            self.settings_window.open = true;
            self.settings_window.tab = crate::settings::SettingsTab::Lighting;
            close = true;
        }
        close
    }

    /// Adopts `pose` as the grid's user coordinate frame — a **local-frame
    /// grid**: the grid origin moves to the body and its lines align to the
    /// body's rotation, so sketching against a tilted structure snaps in
    /// that structure's axes (CAD "align UCS to face").
    fn align_grid_to(&mut self, pose: PosRot) {
        self.grid.origin = gradiance_units::Displacement(pose.pos);
        self.grid.rotation = gradiance_units::Angle(pose.rot);
    }
}

/// The editor panels the menu can open, plus the optimizer it hosts.
/// Bundled into one `SystemParam` to keep `context_menu` under Bevy's
/// system-parameter count limit.
#[derive(SystemParam)]
pub struct PanelMenu<'w> {
    inspector: ResMut<'w, crate::inspector::InspectorPanel>,
    depth: ResMut<'w, crate::depth_panel::DepthPanel>,
    optimizer: crate::optimizer::OptimizerPanel<'w>,
    array: ResMut<'w, crate::array_panel::ArrayWindow>,
}

/// The menu's writers, bundled into one `SystemParam` to keep
/// `context_menu` under Bevy's system-parameter count limit.
#[derive(SystemParam)]
pub struct GroupWriters<'w> {
    /// Every edit row (group, merge, the booleans, deletes) invokes a
    /// registry operation by id. The rows that act on the selection name
    /// `"selection"`; the right click has made what it hit the selection.
    invoke: MessageWriter<'w, InvokeOp>,
    orbits: MessageWriter<'w, gradiance_physics::fields::SetOrbitRequest>,
    /// Probe-pin target (the "Pin probe" menu action); bundled here to
    /// keep `context_menu` under Bevy's system-parameter count limit.
    probes: ResMut<'w, crate::probe::ProbePanel>,
    /// Node-graph canvas (the "Add to node editor" action); bundled here for
    /// the same param-count reason.
    node_graph: ResMut<'w, crate::node_graph::NodeGraph>,
}

/// Behavior-node access for the node-specific context menu, bundled to keep
/// `context_menu` under the system-parameter limit.
#[derive(SystemParam)]
pub struct NodeMenu<'w, 's> {
    /// Selectable node kinds by id (for the node menu editor).
    nodes: Query<'w, 's, (&'static StableId, &'static NodeKind), With<BehaviorNode>>,
}

/// Open context menu state.
#[derive(Resource, Default, Debug, Clone)]
pub struct ContextMenu {
    /// Whether the menu is showing.
    pub open: bool,
    /// Set on open, cleared after the first render pass (drives the
    /// right-click-selects reconciliation).
    pub fresh: bool,
    /// Screen position (logical px) to anchor at.
    pub screen: Vec2,
    /// World-space click point — passed to an invoked action, which reads
    /// it with `(action-x)` / `(action-y)`.
    pub world: Vec2,
    /// Bodies under the click, topmost first.
    pub under: Vec<StableId>,
    /// Joint whose anchor is under the click, if any.
    pub joint: Option<StableId>,
    /// Behavior node under the click, if any (opens the node menu instead
    /// of the physical-object menu).
    pub node: Option<StableId>,
    /// The radius "Merge with fillet" uses; kept between openings.
    pub fillet_radius: FilletRadius,
}

/// One-click actions on the selected bodies (briefs S-5, S-6), as
/// `(label, operation id, hover text)`. Each operation takes `{"i":
/// "selection"}` and is one undo step. Kept as data so a later surface (the
/// contextual task bar) renders the same table.
pub const BODY_QUICK_ACTIONS: &[(&str, &str, &str)] = &[
    (
        "Loosen",
        "body.loosen",
        "drop every joint of the selected bodies; the bodies stay",
    ),
    (
        "Pin at centre",
        "body.pin_centre",
        "hinge the body to the world at its centre, so it can only spin",
    ),
    (
        "Thruster at centre",
        "body.thruster_centre",
        "attach a thruster at the body's centre, pushing along its up",
    ),
    (
        "To circle",
        "body.to_circle",
        "turn each selected body into a circle of the same area",
    ),
    (
        "Make gear",
        "body.make_gear",
        "turn each selected circle into a toothed gear that meshes with other made gears",
    ),
];

/// One-click actions on a hinge under the click (brief S-6), as
/// `(label, operation id, hover text)`; each takes the joint's id as `i`.
pub const HINGE_QUICK_ACTIONS: &[(&str, &str, &str)] = &[(
    "Brake motor",
    "joint.brake",
    "motor on at 0 rad/s with its torque cap kept: the hinge holds still",
)];

/// The context menu's fillet radius, in metres — a parameter of the next
/// "Merge with fillet" gesture, not authored state: it lives with the
/// menu, is never saved, and writes nothing until the button commits one
/// `body.fillet_merge` invocation. Scripts pass the radius to `(fillet-merge a b r)`
/// instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilletRadius(pub f32);

impl Default for FilletRadius {
    /// A quarter metre: a visible blend on bodies of a metre or so, and
    /// well above the ~4 contour cells a fillet needs to render as a curve
    /// rather than a chamfer (`geometry::polygonize` tests).
    fn default() -> Self {
        Self(0.25)
    }
}

/// One context-menu button, published under `context:<label>` so
/// `ui.inspect` lists it and `ui.click` / `ui.click_button` can drive the
/// menu remotely by name (a test and a script can press
/// "context:Delete node" instead of measuring pixels; the top menu bar
/// owns the `menu:` prefix). `hint` becomes the hover text when non-empty.
fn row(ui: &mut egui::Ui, label: &str, hint: &str) -> bool {
    publish_row(label, ui.button(label), hint)
}

/// The enabled-gated form of [`row`].
fn row_enabled(ui: &mut egui::Ui, label: &str, hint: &str, enabled: bool) -> bool {
    publish_row(
        label,
        ui.add_enabled(enabled, egui::Button::new(label)),
        hint,
    )
}

/// The compact form of [`row`] (the align and depth button strips).
fn row_small(ui: &mut egui::Ui, label: &str) -> bool {
    publish_row(label, ui.small_button(label), "")
}

/// Publishes the response's rect under `context:<label>` and reports the
/// click.
fn publish_row(label: &str, response: egui::Response, hint: &str) -> bool {
    crate::widget_registry::publish_target(&format!("context:{label}"), response.rect);
    if hint.is_empty() {
        response.clicked()
    } else {
        response.on_hover_text(hint).clicked()
    }
}

/// Nearest joint id whose glyph distance is within `radius`. Pure so the
/// pick can be unit-tested without a World; the caller supplies each
/// joint's distance via `joint_edit::glyph_distance` (whole ring / travel
/// line / coil, matching left-click picking).
fn nearest_joint(
    radius: f32,
    candidates: impl Iterator<Item = (StableId, f32)>,
) -> Option<StableId> {
    candidates
        .filter(|(_, d)| *d <= radius)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(id, _)| id)
}

/// Opens the menu on a right *click* — a release within the shared
/// right-click deadzone of the press. Gestures that need right-drag
/// (selection rotate, camera pan) use the same deadzone, so a click always
/// reaches the menu and a drag never does; no cross-system ordering is
/// involved. A right-click while the **left** button is held (a gesture in
/// flight owns the pointer) is ignored — it must not open a menu over a
/// running gesture, and its click-through selection must not hijack the
/// selection the gesture is already using.
#[expect(clippy::too_many_arguments)] // one click handler, grouped reads
pub fn open_context_menu(
    buttons: Res<PointerButtons>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cursor: Res<CursorWorldPos>,
    over_ui: Res<PointerOverUi>,
    physics: PhysicsQueries,
    bodies: gradiance_interaction::tools::HitQuery,
    ids: Query<&StableId>,
    joints: Query<(&StableId, &JointDef)>,
    transforms: Query<&Transform, With<Body>>,
    index: Res<IdIndex>,
    cam_scale: Res<gradiance_interaction::camera::CameraScale>,
    nodes: Query<(&StableId, &Transform), With<BehaviorNode>>,
    mut press_pos: Local<Option<Vec2>>,
    mut menu: ResMut<ContextMenu>,
) {
    let screen = windows.iter().next().and_then(Window::cursor_position);
    if buttons.just_pressed(MouseButton::Right) && !over_ui.0 {
        *press_pos = screen;
    }
    if buttons.just_released(MouseButton::Right)
        && !buttons.pressed(MouseButton::Left)
        && let (Some(start), Some(now)) = (press_pos.take(), screen)
        && start.distance(now) < RIGHT_CLICK_DEADZONE_PX
        && let Some(world) = cursor.0
    {
        menu.under = bodies_at_sorted(world, &physics, &bodies)
            .into_iter()
            .filter_map(|e| ids.get(e).ok().copied())
            .collect();
        // A joint glyph within the pick radius is offered for configuration
        // (whole glyph, matching left-click picking).
        let scale = cam_scale.0;
        menu.joint = nearest_joint(
            ANCHOR_PICK_PX * scale,
            joints.iter().filter_map(|(id, def)| {
                let entity = index.entity(def.body_a)?;
                let pose = PosRot::from_transform(transforms.get(entity).ok()?);
                let anchor = def.anchor_world(pose.pos, pose.rot);
                let world_b = match def.body_b {
                    Some(idb) => {
                        let pose_b =
                            PosRot::from_transform(transforms.get(index.entity(idb)?).ok()?);
                        Some(pose_b.pos + Vec2::from_angle(pose_b.rot).rotate(def.anchor_b.value()))
                    }
                    None => Some(def.anchor_b.value()),
                };
                Some((
                    *id,
                    gradiance_interaction::joint_edit::glyph_distance(
                        def, anchor, pose.rot, world_b, world, scale,
                    ),
                ))
            }),
        );
        // A node glyph under the click opens the node menu (its own menu,
        // distinct from the physical-object one).
        let node_radius = gradiance_interaction::node_edit::NODE_PICK_PX * scale;
        menu.node = nodes
            .iter()
            .filter_map(|(id, t)| {
                let d = t.translation.truncate().distance(world);
                (d <= node_radius).then_some((d, *id))
            })
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, id)| id);
        menu.screen = now;
        menu.world = world;
        menu.open = true;
        menu.fresh = true;
    }
}

/// Renders the menu and emits intents for its actions.
#[expect(clippy::too_many_lines)] // one menu, plain sections in order
pub fn context_menu(
    mut contexts: EguiContexts,
    mut menu: ResMut<ContextMenu>,
    mut selection: ResMut<Selection>,
    mut selected_joint: ResMut<SelectedJoint>,
    index: Res<IdIndex>,
    mut props: crate::inspector::BodyProps,
    mut panels: PanelMenu,
    groups: Query<(Entity, &gradiance_domain::group::SelectionGroup), With<Body>>,
    bodies_q: Query<(&ShapeDef, &Transform, &Appearance), With<Body>>,
    joints: Query<&JointDef>,
    mut writers: GroupWriters,
    mut moves: MessageWriter<CommitTransformIntent>,
    node_menu: NodeMenu,
    mut script: ScriptMenu,
    mut background: BackgroundMenu,
) -> Result {
    if !menu.open {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    // The menu's opening pass. Captured **before** the `fresh` block
    // consumes the flag below: egui sizes a brand-new area before it can
    // be interacted with, and the very click that opened the menu is still
    // in this pass's input, so `clicked_elsewhere` would read the menu's
    // own opening click as a click elsewhere and close it immediately (the
    // "first right-click of the session shows no menu" bug). The opening
    // pass never closes on an outside click.
    let opened_this_pass = menu.fresh;
    // Right-click selects what it acts on (Algodoo): when the menu opens
    // over a body that is not part of the selection, the topmost body under
    // the click becomes the selection, so every entry below binds to what
    // the user actually clicked.
    if menu.fresh {
        menu.fresh = false;
        let clicked_selected = selection
            .iter()
            .filter_map(|e| props.ids.get(e).ok())
            .any(|id| menu.under.contains(id));
        // A node click selects the node; otherwise select the clicked body.
        if let Some(node_id) = menu.node
            && let Some(entity) = index.entity(node_id)
        {
            SelectTransition::SetBodies(vec![entity]).apply(
                &mut selection,
                &mut selected_joint,
                &groups,
            );
        } else if !clicked_selected
            && let Some(top) = menu.under.first()
            && let Some(entity) = index.entity(*top)
        {
            SelectTransition::SetBodies(vec![entity]).apply(
                &mut selection,
                &mut selected_joint,
                &groups,
            );
        }
    }
    let selected_ids: Vec<StableId> = selection
        .iter()
        .filter_map(|e| props.ids.get(e).ok().copied())
        .collect();
    // Ungroup has something to undo only when a selected body is in a group
    // (brief A-3: it was enabled on any selection, a click that did nothing).
    let any_grouped = selection.iter().any(|e| groups.contains(e));

    let at = menu.world;
    let mut fillet_radius = menu.fillet_radius.0;
    let mut close = false;
    let response = egui::Area::new(egui::Id::new("context-menu"))
        .fixed_pos(egui::pos2(menu.screen.x, menu.screen.y))
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);

                // A behavior node has its own menu — kind editor + delete —
                // distinct from the physical-object menu below.
                if let Some(node_id) = menu.node
                    && let Some(entity) = index.entity(node_id)
                    && let Ok((_, kind)) = node_menu.nodes.get(entity)
                {
                    let kind = kind.clone();
                    let has_pins = matches!(kind, NodeKind::Thruster(_) | NodeKind::Laser(_));
                    ui.label(egui::RichText::new(format!("Node: {}", kind.label())).strong());
                    // One selected body other than the node: a laser target.
                    let mut others = selected_ids.iter().copied().filter(|id| {
                        index
                            .entity(*id)
                            .is_some_and(|e| node_menu.nodes.get(e).is_err())
                    });
                    let pick = others.next().filter(|_| others.next().is_none());
                    if let Some(next) = widgets::node_kind_editor(ui, "menu", &kind, pick) {
                        props.edits.write(PropertyEditIntent {
                            changes: vec![PropertyChange {
                                id: node_id,
                                old: PropertyValue::NodeKind(kind),
                                new: PropertyValue::NodeKind(next),
                            }],
                        });
                    }
                    ui.separator();
                    // A node with signal pins can sit on the node canvas
                    // without a wire (brief Q-4).
                    if has_pins
                        && row(
                            ui,
                            "Add to node editor",
                            "place this node's block on the node graph canvas",
                        )
                    {
                        writers.node_graph.add(node_id);
                        close = true;
                    }
                    if row(ui, "Delete node", "") {
                        writers.invoke.write(InvokeOp::with(
                            "body.delete",
                            json!({"i": node_id.0.to_string()}),
                        ));
                        close = true;
                    }
                    return; // node menu replaces the physical-object menu
                }

                // Joint under the click: configuration, reachable via right-click
                // (not only the select-and-inspect flow).
                if let Some(joint_id) = menu.joint
                    && let Some(entity) = index.entity(joint_id)
                    && let Ok(def) = joints.get(entity)
                {
                    widgets::section_header(ui, kind_name(&def.kind));
                    if row(ui, "Configure joint…", "") {
                        SelectTransition::SelectJoint(entity).apply(
                            &mut selection,
                            &mut selected_joint,
                            &groups,
                        );
                        close = true;
                    }
                    let mut collide = def.common.collide_connected;
                    if ui
                        .checkbox(&mut collide, "connected bodies collide")
                        .changed()
                    {
                        let mut new = def.clone();
                        new.common.collide_connected = collide;
                        props.edits.write(PropertyEditIntent {
                            changes: vec![PropertyChange {
                                id: joint_id,
                                old: PropertyValue::Joint(def.clone()),
                                new: PropertyValue::Joint(new),
                            }],
                        });
                        close = true;
                    }
                    if row(
                        ui,
                        "Add to node editor",
                        "place this joint's block on the node graph canvas",
                    ) {
                        writers.node_graph.add(joint_id);
                        close = true;
                    }
                    if matches!(def.kind, JointKind::Hinge { .. }) {
                        for &(label, id, hint) in HINGE_QUICK_ACTIONS {
                            if row(ui, label, hint) {
                                writers
                                    .invoke
                                    .write(InvokeOp::with(id, json!({"i": joint_id.0.to_string()})));
                                close = true;
                            }
                        }
                    }
                    if row(ui, "Delete joint", "") {
                        writers.invoke.write(InvokeOp::with(
                            "joint.delete",
                            json!({"i": joint_id.0.to_string()}),
                        ));
                        close = true;
                    }
                    ui.separator();
                }

                // Context-menu-first property editing (feedback 2.8):
                // the everyday sections live here; the full Properties pane is a
                // command away.
                if !selected_ids.is_empty() {
                    if row(ui, "Properties…", "") {
                        panels.inspector.open = true;
                        close = true;
                    }
                    egui::CollapsingHeader::new("Material")
                        .id_salt(ui.id().with("menu-material"))
                        .show(ui, |ui| {
                            crate::inspector::physics_section(ui, &selection, &mut props);
                        });
                    if let Some(primary) = selection.primary()
                        && props.is_body(primary)
                    {
                        egui::CollapsingHeader::new("Sensors")
                            .id_salt(ui.id().with("menu-sensors"))
                            .show(ui, |ui| props.sensors(ui, primary));
                    }
                    egui::CollapsingHeader::new("Appearance")
                        .id_salt(ui.id().with("menu-appearance"))
                        .show(ui, |ui| {
                            crate::inspector::appearance_section(ui, &selection, &mut props);
                            crate::inspector::shape_section(ui, &selection, &mut props);
                        });
                    ui.separator();
                }

                if row_enabled(
                    ui,
                    "Set in orbit",
                    "give each body the circular-orbit velocity around the \
                     dominant attractive field at its position (Algodoo)",
                    !selected_ids.is_empty(),
                ) {
                    writers
                        .orbits
                        .write(gradiance_physics::fields::SetOrbitRequest {
                            targets: selected_ids.clone(),
                        });
                    close = true;
                }
                if row_enabled(
                    ui,
                    "Pin probe",
                    "pin a live physics readout in the Probes window",
                    !selected_ids.is_empty(),
                ) {
                    for id in &selected_ids {
                        writers.probes.pin(*id);
                    }
                    close = true;
                }
                if row_enabled(
                    ui,
                    "Add to node editor",
                    "place this body's block on the node graph canvas",
                    !selected_ids.is_empty(),
                ) {
                    for id in &selected_ids {
                        writers.node_graph.add_body(*id);
                    }
                    close = true;
                }
                if row_enabled(ui, "Group", "", selected_ids.len() >= 2) {
                    writers
                        .invoke
                        .write_batch(key_invocations(EditorAction::Group));
                    close = true;
                }
                if row_enabled(ui, "Ungroup", "", any_grouped) {
                    writers
                        .invoke
                        .write_batch(key_invocations(EditorAction::Ungroup));
                    close = true;
                }
                // Quick actions (briefs S-5, S-6): one table of operation
                // ids, invoked on the selection.
                for &(label, id, hint) in BODY_QUICK_ACTIONS {
                    if row_enabled(ui, label, hint, !selected_ids.is_empty()) {
                        writers
                            .invoke
                            .write(InvokeOp::with(id, json!({"i": "selection"})));
                        close = true;
                    }
                }
                if row_enabled(ui, "Merge into one body", "", selected_ids.len() >= 2) {
                    writers.invoke.write(InvokeOp::with(
                        "body.merge",
                        json!({"a": "selection", "b": "selection"}),
                    ));
                    close = true;
                }
                // Merge with a fillet: the same boolean, as a smooth union
                // of the radius set just below it.
                if row_enabled(
                    ui,
                    "Merge with fillet",
                    "merge into one body, blending a rounded fillet where the bodies meet",
                    selected_ids.len() >= 2,
                ) {
                    writers.invoke.write(InvokeOp::with(
                        "body.fillet_merge",
                        json!({"a": "selection", "b": "selection", "radius": fillet_radius}),
                    ));
                    close = true;
                }
                egui::Grid::new("context-menu-fillet").show(ui, |ui| {
                    widgets::labelled_drag(
                        ui,
                        "Fillet radius",
                        "blend radius of Merge with fillet; also joins bodies less than half of it apart",
                        &mut fillet_radius,
                        widgets::config_field(FilletRadius::default().0, 0.005)
                            .range(0.01..=10.0)
                            .unit(gradiance_units::Dimension::Length),
                    );
                });
                // The other body booleans: the first-selected body hosts the
                // result and the rest are consumed (`command::merge_cmd`).
                for (label, id, hint) in [
                    (
                        "Subtract from first selected",
                        "body.subtract",
                        "cut the other bodies' shapes out of the first-selected body",
                    ),
                    (
                        "Intersect",
                        "body.intersect",
                        "keep only where the selected bodies overlap, on the first-selected body",
                    ),
                    (
                        "Xor",
                        "body.xor",
                        "keep what lies in exactly one selected body; the overlap is removed",
                    ),
                ] {
                    if row_enabled(ui, label, hint, selected_ids.len() >= 2) {
                        writers.invoke.write(InvokeOp::with(
                            id,
                            json!({"a": "selection", "b": "selection"}),
                        ));
                        close = true;
                    }
                }
                // Mirror, from the Edit menu's table (brief Q-3; no key).
                for (label, id) in crate::menu::MIRROR_ROWS {
                    if row_enabled(ui, label, "", !selected_ids.is_empty()) {
                        writers
                            .invoke
                            .write(InvokeOp::with(id, json!({"targets": ["selection"]})));
                        close = true;
                    }
                }
                ui.separator();

                // Align & distribute (one undo step via the move command).
                if selected_ids.len() >= 2 {
                    ui.label(egui::RichText::new("Align").weak());
                    let items: Vec<AlignItem> = selection
                        .iter()
                        .filter_map(|e| {
                            let id = props.ids.get(e).ok().copied()?;
                            let (shape, transform, _) = bodies_q.get(e).ok()?;
                            if shape.contains_half_plane() {
                                return None;
                            }
                            Some((
                                id,
                                gradiance_core::units::PosRot::from_transform(transform),
                                world_bounds(shape, transform),
                            ))
                        })
                        .collect();
                    let mut emit = |op: AlignOp| {
                        let changes = align_changes(&items, op);
                        if !changes.is_empty() {
                            moves.write(CommitTransformIntent { changes });
                        }
                        close = true;
                    };
                    ui.horizontal_wrapped(|ui| {
                        for (label, op) in [
                            ("⏴ left", AlignOp::Left),
                            ("right ⏵", AlignOp::Right),
                            ("⏶ top", AlignOp::Top),
                            ("bottom ⏷", AlignOp::Bottom),
                            ("center vertically", AlignOp::CenterY),
                            ("center horizontally", AlignOp::CenterX),
                            ("distribute horizontally", AlignOp::DistributeX),
                            ("distribute vertically", AlignOp::DistributeY),
                        ] {
                            if row_small(ui, label) {
                                emit(op);
                            }
                        }
                    });
                    ui.separator();
                }

                // Array repeat: the gesture is alt-drag on a handle, so the
                // menu entry only opens the rulebook.
                if !selected_ids.is_empty()
                    && row(
                        ui,
                        "Array options…",
                        "repeat the selection by holding Alt and dragging a \
                         selection handle",
                    )
                {
                    panels.array.open = true;
                    close = true;
                }

                // Close packing: rearrange the selection into the smallest
                // area. Also shown while a run is live, so the menu can accept
                // or cancel one started from anywhere.
                if selected_ids.len() >= 2 || panels.optimizer.session.is_active() {
                    close |=
                        crate::optimizer::context_menu_entry(ui, &selection, &mut panels.optimizer);
                    // "Pack options…" only sets the expand flag; the pane it
                    // expands into has to be open for that to mean anything.
                    if panels.optimizer.expanded.0 {
                        panels.inspector.open = true;
                    }
                    ui.separator();
                }

                // Empty-canvas click: the scene/background actions.
                if menu.under.is_empty() && menu.joint.is_none() {
                    close |= background.section(ui, &mut script, at);
                    ui.separator();
                }

                // Pick from overlapping bodies under the click.
                if !menu.under.is_empty() {
                    ui.label(egui::RichText::new("Select from").weak());
                    for id in menu.under.clone() {
                        // Workspace-labelled bodies show their script name
                        // in place of the kind word.
                        let title = script.labels.name_of(id).map_or_else(
                            || {
                                let shape = index
                                    .entity(id)
                                    .and_then(|e| bodies_q.get(e).ok())
                                    .map(|(shape, ..)| shape);
                                body_label(shape, &id)
                            },
                            |name| object_label(name, &id),
                        );
                        let title = format!("· {title}");
                        if row(ui, &title, "") {
                            if let Some(entity) = index.entity(id) {
                                gradiance_interaction::selection::SelectTransition::SetBodies(
                                    vec![entity],
                                )
                                .apply(
                                    &mut selection,
                                    &mut selected_joint,
                                    &groups,
                                );
                            }
                            close = true;
                        }
                    }
                    ui.separator();
                }

                // Depth actions on the selection (the draggable band
                // editor is the Depth panel dock; these are the one-click
                // moves). Collision volume ≡ render depth — one band.
                ui.label(egui::RichText::new("Depth").weak());
                ui.horizontal_wrapped(|ui| {
                    let step = gradiance_core::constants::LAYER_HEIGHT;
                    let shift =
                        |edits: &mut MessageWriter<PropertyEditIntent>,
                         f: &dyn Fn(&DepthBand) -> DepthBand| {
                            depth_edit(&selection, &props.ids, &props.depths, edits, f);
                        };
                    if row_small(ui, "to front") {
                        shift(&mut props.edits, &|old| DepthBand::metres(0.0, old.thickness().max(DepthBand::MIN_THICKNESS)));
                        close = true;
                    }
                    if row_small(ui, "forward") {
                        shift(&mut props.edits, &|old| DepthBand::metres(old.near.value() - step, old.far.value() - step));
                        close = true;
                    }
                    if row_small(ui, "backward") {
                        shift(&mut props.edits, &|old| DepthBand::metres(old.near.value() + step, old.far.value() + step));
                        close = true;
                    }
                });
                if row(ui, "Depth panel…", "") {
                    panels.depth.open = true;
                    close = true;
                }
                // Local-frame grid: adopt the primary body's pose as the
                // grid's user coordinate system (sketch in its axes).
                if let Some(primary) = selection.primary()
                    && let Ok((_, transform, _)) = bodies_q.get(primary)
                    && row(
                        ui,
                        "Align grid to body",
                        "move & rotate the grid into this body's frame",
                    )
                {
                    background.align_grid_to(PosRot::from_transform(transform));
                    close = true;
                }
                ui.separator();
                // Random palette colors: one per body, or one per group
                // (bodies sharing an outermost group match). The palette
                // walk is `appearance::random_fills`; this is one intent.
                let grouped_click = row(
                    ui,
                    "Random colors (per group)",
                    "grouped bodies share one color",
                );
                let per_body_click = row(ui, "Random colors (per body)", "");
                if grouped_click || per_body_click {
                    let members: Vec<_> = selection
                        .iter()
                        .filter_map(|e| {
                            let id = props.ids.get(e).ok().copied()?;
                            let (_, _, old) = bodies_q.get(e).ok()?;
                            let group = groups
                                .get(e)
                                .ok()
                                .and_then(|(_, g)| g.outermost());
                            Some((id, group, *old))
                        })
                        .collect();
                    let changes: Vec<PropertyChange> =
                        gradiance_domain::appearance::random_fills(&members, grouped_click)
                            .into_iter()
                            .zip(&members)
                            .map(|((id, new), (_, _, old))| PropertyChange {
                                id,
                                old: PropertyValue::Appearance(*old),
                                new: PropertyValue::Appearance(new),
                            })
                            .collect();
                    if !changes.is_empty() {
                        props.edits.write(PropertyEditIntent { changes });
                    }
                    close = true;
                }

                // User-registered script actions (added from `.scm` via
                // `register-action`). Invoking one submits its source, with
                // the click point, through the same `ScriptInputs` seam a
                // REPL line uses.
                if script.actions.user().next().is_some() {
                    ui.separator();
                    ui.label(egui::RichText::new("Scripts").weak());
                    for action in script.actions.user() {
                        if row(ui, &action.label, &action.hint) {
                            script.inputs.invoke(action, at);
                            close = true;
                        }
                    }
                }
            });
        })
        .response;
    menu.fillet_radius.0 = fillet_radius;
    // The optimizer entry's buttons, through the menu's one writer.
    writers.invoke.write_batch(panels.optimizer.take_requests());

    if close || (response.clicked_elsewhere() && !opened_this_pass) {
        menu.open = false;
    }
    Ok(())
}

fn depth_edit(
    selection: &Selection,
    ids: &Query<&StableId>,
    depths: &Query<&DepthBand, With<Body>>,
    edits: &mut MessageWriter<PropertyEditIntent>,
    make_new: impl Fn(&DepthBand) -> DepthBand,
) {
    let changes: Vec<PropertyChange> = selection
        .iter()
        .filter_map(|e| {
            let id = ids.get(e).ok().copied()?;
            let old = *depths.get(e).ok()?;
            Some(PropertyChange {
                id,
                old: PropertyValue::Depth(old),
                new: PropertyValue::Depth(make_new(&old).sanitized()),
            })
        })
        .collect();
    if !changes.is_empty() {
        edits.write(PropertyEditIntent { changes });
    }
}

/// A body's conservative world-space AABB (`geometry::sdf::world_aabb`).
fn world_bounds(shape: &ShapeDef, transform: &Transform) -> (Vec2, Vec2) {
    let rot = transform.rotation.to_euler(EulerRot::XYZ).2;
    gradiance_geometry::sdf::world_aabb(shape, transform.translation.truncate(), rot)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every quick action names a registered operation that a click can
    /// supply (its fields are targets, or optional).
    #[test]
    fn quick_actions_name_clickable_operations() {
        let mut app = App::new();
        app.add_plugins(gradiance_op::OpPlugin);
        gradiance_command::ops::install(&mut app);
        let registry = app.world().resource::<gradiance_op::OpRegistry>();
        for (label, id, _) in BODY_QUICK_ACTIONS.iter().chain(HINGE_QUICK_ACTIONS) {
            let entry = registry
                .get(id)
                .unwrap_or_else(|| panic!("{label}: {id} is not registered"));
            assert!(
                entry.selection_params().is_some(),
                "{label}: {id} needs typing"
            );
        }
    }

    #[test]
    fn nearest_joint_picks_closest_within_radius() {
        let cands = [
            (StableId::new(), 10.0),
            (StableId::new(), 3.0),
            (StableId::new(), 100.0),
        ];
        // Radius 5: only the distance-3 glyph qualifies.
        assert_eq!(nearest_joint(5.0, cands.iter().copied()), Some(cands[1].0));
    }

    #[test]
    fn nearest_joint_none_outside_radius() {
        let cands = [(StableId::new(), 50.0)];
        assert_eq!(nearest_joint(5.0, cands.iter().copied()), None);
    }

    /// The no-drift test: the menu places exactly the table's built-ins.
    #[test]
    fn every_builtin_has_a_menu_place() {
        let mut seeded: Vec<&str> = gradiance_script::actions::builtin_seed()
            .iter()
            .filter_map(|a| a.builtin)
            .collect();
        let mut placed = BACKGROUND_BUILTINS.to_vec();
        seeded.sort_unstable();
        placed.sort_unstable();
        assert_eq!(seeded, placed);
        // Each placed entry is offered in exactly one gravity state.
        for name in placed {
            assert_ne!(
                background_builtin_shown(name, true),
                background_builtin_shown(name, false),
                "{name} has no visibility rule"
            );
        }
    }

    #[test]
    fn nearest_joint_empty_is_none() {
        assert_eq!(nearest_joint(5.0, std::iter::empty()), None);
    }
}
