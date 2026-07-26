//! Right-click context menu: grouping, pick-from-stack, and collision
//! layer operations.

use crate::joint_inspector::kind_name;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use gradiance_command::intent::{
    CommitTransformIntent, DeleteIntent, DeleteJointIntent, GroupIntent, MergeIntent,
    PropertyEditIntent, UngroupIntent,
};
use gradiance_command::property::{PropertyChange, PropertyValue};
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_core::units::PosRot;
use gradiance_domain::Body;
use gradiance_domain::appearance::{Appearance, Rgba};
use gradiance_domain::depth::DepthBand;
use gradiance_domain::joint::JointDef;
use gradiance_domain::node::{BehaviorNode, NodeKind};
use gradiance_domain::shape::ShapeDef;
use gradiance_interaction::PointerOverUi;
use gradiance_interaction::align::{AlignItem, AlignOp, align_changes};
use gradiance_interaction::cursor::CursorWorldPos;
use gradiance_interaction::joint_edit::ANCHOR_PICK_PX;
use gradiance_interaction::pointer::PointerButtons;
use gradiance_interaction::selection::{SelectTransition, SelectedJoint, Selection};
use gradiance_interaction::tools::bodies_at_sorted;
use gradiance_physics::queries::PhysicsQueries;
use gradiance_script::bridge::{ScriptActions, ScriptInputs};

/// Script-action access, bundled into one `SystemParam` to keep `context_menu`
/// under Bevy's system-parameter count limit. Reads the registered actions and
/// submits an invoked one's source through the shared `ScriptInputs` queue.
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

impl BackgroundMenu<'_> {
    /// Renders the empty-canvas section; returns true when the menu should
    /// close.
    fn section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut close = false;
        ui.label(egui::RichText::new("Background").weak());
        ui.horizontal(|ui| {
            ui.label("plane friction");
            ui.add(
                egui::DragValue::new(&mut self.sim.plane_friction)
                    .speed(0.01)
                    .range(0.0..=5.0),
            )
            .on_hover_text("top-down mode: bodies rub on the back plane (0 = off)");
        });
        let top_down = self.sim.gravity.length() < 1e-3;
        if top_down {
            if ui.button("Restore gravity (side view)").clicked() {
                self.sim.gravity = gradiance_core::constants::GRAVITY;
                close = true;
            }
        } else if ui
            .button("Top-down mode (gravity into screen)")
            .on_hover_text("zeroes in-plane gravity; give the plane some friction")
            .clicked()
        {
            self.sim.gravity = Vec2::ZERO;
            if self.sim.plane_friction <= 0.0 {
                self.sim.plane_friction = 0.3;
            }
            close = true;
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
        if (self.grid.origin != Vec2::ZERO || self.grid.rotation != 0.0)
            && ui
                .button("Reset grid to world")
                .on_hover_text("clear a local-frame alignment")
                .clicked()
        {
            self.grid.origin = Vec2::ZERO;
            self.grid.rotation = 0.0;
            close = true;
        }
        if ui.button("Scene & lighting settings…").clicked() {
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
        self.grid.origin = pose.pos;
        self.grid.rotation = pose.rot;
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
}

/// The body-scoped intent writers, bundled into one `SystemParam` to keep
/// `context_menu` under Bevy's system-parameter count limit.
#[derive(SystemParam)]
pub struct GroupWriters<'w> {
    group: MessageWriter<'w, GroupIntent>,
    ungroup: MessageWriter<'w, UngroupIntent>,
    merge: MessageWriter<'w, MergeIntent>,
    orbits: MessageWriter<'w, gradiance_physics::fields::SetOrbitRequest>,
    /// Probe-pin target (the "Pin probe" menu action); bundled here to
    /// keep `context_menu` under Bevy's system-parameter count limit.
    probes: ResMut<'w, crate::probe::ProbePanel>,
    /// Node-graph canvas (the "Add to node editor" action); bundled here for
    /// the same param-count reason.
    node_graph: ResMut<'w, crate::node_graph::NodeGraph>,
}

/// Behavior-node access for the node-specific context menu, bundled to keep
/// `context_menu` under the system-parameter limit (it replaces the lone
/// joint-delete writer).
#[derive(SystemParam)]
pub struct NodeMenu<'w, 's> {
    /// Selectable node kinds by id (for the node menu editor).
    nodes: Query<'w, 's, (&'static StableId, &'static NodeKind), With<BehaviorNode>>,
    /// Joint deletion (moved here from a standalone param).
    delete_joint: MessageWriter<'w, DeleteJointIntent>,
    /// Body/node deletion (the node menu's Delete).
    delete: MessageWriter<'w, DeleteIntent>,
}

/// Open context menu state.
#[derive(Resource, Default, Debug)]
pub struct ContextMenu {
    /// Whether the menu is showing.
    pub open: bool,
    /// Set on open, cleared after the first render pass (drives the
    /// right-click-selects reconciliation).
    pub fresh: bool,
    /// Screen position (logical px) to anchor at.
    pub screen: Vec2,
    /// Bodies under the click, topmost first.
    pub under: Vec<StableId>,
    /// Joint whose anchor is under the click, if any.
    pub joint: Option<StableId>,
    /// Behavior node under the click, if any (opens the node menu instead
    /// of the physical-object menu).
    pub node: Option<StableId>,
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

/// Opens the menu on a right *click* — a release within a small screen
/// deadzone of the press. Gestures that need right-drag (selection
/// rotate, camera pan) use the same deadzone, so a click reaches the
/// menu and a drag never does; no cross-system ordering is involved.
#[expect(clippy::too_many_arguments)] // one click handler, grouped reads
pub fn open_context_menu(
    buttons: Res<PointerButtons>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cursor: Res<CursorWorldPos>,
    over_ui: Res<PointerOverUi>,
    physics: PhysicsQueries,
    bodies: Query<(&ShapeDef, &DepthBand), With<Body>>,
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
        && let (Some(start), Some(now)) = (press_pos.take(), screen)
        && start.distance(now) < 4.0
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
                        Some(pose_b.pos + Vec2::from_angle(pose_b.rot).rotate(def.anchor_b))
                    }
                    None => Some(def.anchor_b),
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
    mut node_menu: NodeMenu,
    mut script: ScriptMenu,
    mut background: BackgroundMenu,
) -> Result {
    if !menu.open {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
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
                    ui.label(egui::RichText::new(format!("Node: {}", kind.label())).strong());
                    if let Some(next) = crate::signals::node_kind_editor(ui, "menu", &kind) {
                        props.edits.write(PropertyEditIntent {
                            changes: vec![PropertyChange {
                                id: node_id,
                                old: PropertyValue::NodeKind(kind),
                                new: PropertyValue::NodeKind(next),
                            }],
                        });
                    }
                    ui.separator();
                    if ui.button("Delete node").clicked() {
                        node_menu.delete.write(DeleteIntent {
                            targets: vec![node_id],
                        });
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
                    ui.label(egui::RichText::new(kind_name(&def.kind)).strong());
                    if ui.button("Configure joint…").clicked() {
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
                    if ui.button("Delete joint").clicked() {
                        node_menu
                            .delete_joint
                            .write(DeleteJointIntent { id: joint_id });
                        close = true;
                    }
                    ui.separator();
                }

                // Context-menu-first property editing (feedback 2.8):
                // the everyday sections live here; the full Properties pane is a
                // command away.
                if !selected_ids.is_empty() {
                    if ui.button("Properties…").clicked() {
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

                if ui
                    .add_enabled(!selected_ids.is_empty(), egui::Button::new("Set in orbit"))
                    .on_hover_text(
                        "give each body the circular-orbit velocity around the \
                         dominant attractive field at its position (Algodoo)",
                    )
                    .clicked()
                {
                    writers
                        .orbits
                        .write(gradiance_physics::fields::SetOrbitRequest {
                            targets: selected_ids.clone(),
                        });
                    close = true;
                }
                if ui
                    .add_enabled(!selected_ids.is_empty(), egui::Button::new("Pin probe"))
                    .on_hover_text("pin a live physics readout in the Probes window")
                    .clicked()
                {
                    for id in &selected_ids {
                        writers.probes.pin(*id);
                    }
                    close = true;
                }
                if ui
                    .add_enabled(
                        !selected_ids.is_empty(),
                        egui::Button::new("Add to node editor"),
                    )
                    .on_hover_text("place this body's block on the ⬡ Graph canvas")
                    .clicked()
                {
                    for id in &selected_ids {
                        writers.node_graph.add_body(*id);
                    }
                    close = true;
                }
                if ui
                    .add_enabled(selected_ids.len() >= 2, egui::Button::new("Group"))
                    .clicked()
                {
                    writers.group.write(GroupIntent {
                        targets: selected_ids.clone(),
                    });
                    close = true;
                }
                if ui
                    .add_enabled(!selected_ids.is_empty(), egui::Button::new("Ungroup"))
                    .clicked()
                {
                    writers.ungroup.write(UngroupIntent {
                        targets: selected_ids.clone(),
                    });
                    close = true;
                }
                if ui
                    .add_enabled(
                        selected_ids.len() >= 2,
                        egui::Button::new("Merge into one body"),
                    )
                    .clicked()
                {
                    writers.merge.write(MergeIntent {
                        targets: selected_ids.clone(),
                    });
                    close = true;
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
                            ("center ↕", AlignOp::CenterY),
                            ("center ↔", AlignOp::CenterX),
                            ("distribute ↔", AlignOp::DistributeX),
                            ("distribute ↕", AlignOp::DistributeY),
                        ] {
                            if ui.small_button(label).clicked() {
                                emit(op);
                            }
                        }
                    });
                    ui.separator();
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
                    close |= background.section(ui);
                    ui.separator();
                }

                // Pick from overlapping bodies under the click.
                if !menu.under.is_empty() {
                    ui.label(egui::RichText::new("Select from").weak());
                    for (i, id) in menu.under.clone().into_iter().enumerate() {
                        // Workspace-labelled bodies show their script name.
                        let title = script.labels.name_of(id).map_or_else(
                            || format!("· body {i} ({id:.8})"),
                            |name| format!("· {name} ({id:.8})"),
                        );
                        if ui.button(title).clicked() {
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
                    if ui.small_button("to front").clicked() {
                        shift(&mut props.edits, &|old| DepthBand {
                            near: 0.0,
                            far: old.thickness().max(DepthBand::MIN_THICKNESS),
                        });
                        close = true;
                    }
                    if ui.small_button("forward").clicked() {
                        shift(&mut props.edits, &|old| DepthBand {
                            near: old.near - step,
                            far: old.far - step,
                        });
                        close = true;
                    }
                    if ui.small_button("backward").clicked() {
                        shift(&mut props.edits, &|old| DepthBand {
                            near: old.near + step,
                            far: old.far + step,
                        });
                        close = true;
                    }
                });
                if ui.button("Depth panel…").clicked() {
                    panels.depth.open = true;
                    close = true;
                }
                // Local-frame grid: adopt the primary body's pose as the
                // grid's user coordinate system (sketch in its axes).
                if let Some(primary) = selection.primary()
                    && let Ok((_, transform, _)) = bodies_q.get(primary)
                    && ui
                        .button("Align grid to body")
                        .on_hover_text("move & rotate the grid into this body's frame")
                        .clicked()
                {
                    background.align_grid_to(PosRot::from_transform(transform));
                    close = true;
                }
                ui.separator();
                if ui.button("Random colors (per body)").clicked() {
                    let changes: Vec<PropertyChange> = selection
                        .iter()
                        .enumerate()
                        .filter_map(|(index, e)| {
                            let id = props.ids.get(e).ok().copied()?;
                            let (_, _, old) = bodies_q.get(e).ok()?;
                            // Golden-angle hue walk from each body's id.
                            let base = (id.0.as_u128() % 360) as f32;
                            let hue = base + 137.5 * (index as f32 + 1.0);
                            let new = Appearance {
                                fill: Rgba::from_hsl(hue, 0.65, 0.55),
                                ..*old
                            };
                            Some(PropertyChange {
                                id,
                                old: PropertyValue::Appearance(*old),
                                new: PropertyValue::Appearance(new),
                            })
                        })
                        .collect();
                    if !changes.is_empty() {
                        props.edits.write(PropertyEditIntent { changes });
                    }
                    close = true;
                }

                // User-registered script actions (added from `.scm` via
                // `register-action`). Invoking one submits its source through
                // the same `ScriptInputs` seam a REPL line uses.
                if !script.actions.0.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new("Scripts").weak());
                    for action in &script.actions.0 {
                        if ui.button(&action.label).clicked() {
                            script.inputs.submit(action.source.clone());
                            close = true;
                        }
                    }
                }
            });
        })
        .response;

    if close || response.clicked_elsewhere() {
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

/// A body's conservative world-space AABB.
fn world_bounds(shape: &ShapeDef, transform: &Transform) -> (Vec2, Vec2) {
    let (min, max) = gradiance_geometry::sdf::aabb(shape);
    let affine = transform.compute_affine();
    let corners = [
        Vec2::new(min.x, min.y),
        Vec2::new(max.x, min.y),
        Vec2::new(max.x, max.y),
        Vec2::new(min.x, max.y),
    ];
    let mut wmin = Vec2::splat(f32::MAX);
    let mut wmax = Vec2::splat(f32::MIN);
    for corner in corners {
        let w = affine.transform_point3(corner.extend(0.0)).truncate();
        wmin = wmin.min(w);
        wmax = wmax.max(w);
    }
    (wmin, wmax)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn nearest_joint_empty_is_none() {
        assert_eq!(nearest_joint(5.0, std::iter::empty()), None);
    }
}
