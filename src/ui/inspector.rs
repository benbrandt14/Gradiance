//! Inspector panel for editing entity properties.
//!
//! Provides an Egui sidebar that allows modifying properties of selected entities,
//! such as Transform, RigidBody type, Friction, and Restitution.

use crate::events::{PropertyChange, PropertyChangeEvent};
use crate::input::editable_shape::{EditableShape, ShapeType};
use crate::input::selection::{Selection, SelectionFilter};
use crate::input::tools::connector::Connector;
use crate::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy_egui::{EguiContexts, egui};
use bevy_prototype_lyon::prelude::*;

const DRAG_SPEED: f32 = 0.1;
const FRICTION_MAX: f32 = 2.0;
const RESTITUTION_MAX: f32 = 1.0;
const DENSITY_MIN: f32 = 0.001;
const DENSITY_MAX: f32 = 1000.0;
const PRISMATIC_MIN_DEFAULT: f32 = -10.0;
const PRISMATIC_MAX_DEFAULT: f32 = 10.0;

/// Plugin for the Inspector UI.
pub struct InspectorPlugin;

impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, inspector_ui);
    }
}

#[derive(SystemParam)]
struct InspectorQuery<'w, 's> {
    selection: Res<'w, Selection>,
    selection_filter: ResMut<'w, SelectionFilter>,
    connector_query: Query<'w, 's, &'static Connector>,
    joint_query: Query<'w, 's, &'static ImpulseJoint>,
    #[expect(
        clippy::type_complexity,
        reason = "Large query tuple required for inspector"
    )]
    entity_query: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static Transform>,
            Option<&'static RigidBody>,
            Option<&'static Friction>,
            Option<&'static Restitution>,
            Option<&'static EditableShape>,
            Option<&'static Fill>,
            Option<&'static Stroke>,
            Option<&'static Sensor>,
            Option<&'static LockedAxes>,
            Option<&'static ColliderMassProperties>,
            Option<&'static GravityScale>,
            Option<&'static Sleeping>,
        ),
    >,
    events: EventWriter<'w, PropertyChangeEvent>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum AlignmentAction {
    Min,
    Center,
    Max,
    Distribute,
}

#[derive(Clone, Copy, Debug)]
enum TransformField {
    PosX,
    PosY,
    Rotation,
}

#[derive(Clone, Copy, Debug)]
enum ShapeField {
    Width,
    Height,
    Radius,
}

#[derive(Clone, PartialEq, Debug)]
enum InspectorValue<T> {
    Same(T),
    Mixed,
}

#[derive(Default)]
struct InspectorState {
    transform: Option<InspectorValue<Transform>>,
    editable_shape: Option<InspectorValue<EditableShape>>,
    rigid_body: Option<InspectorValue<RigidBody>>,
    friction: Option<InspectorValue<Friction>>,
    restitution: Option<InspectorValue<Restitution>>,
    fill: Option<InspectorValue<Fill>>,
    stroke: Option<InspectorValue<Stroke>>,
    sensor: Option<InspectorValue<bool>>,
    locked_axes: Option<InspectorValue<LockedAxes>>,
    density: Option<InspectorValue<f32>>,
    gravity_scale: Option<InspectorValue<f32>>,
}

// TODO: Refactor to separate View (UI) from Model (Logic).
// Instead of mutating components directly in the UI system, emit events (e.g. `PropertyChangeEvent`)
// and handle them in a separate system. This improves testability and undo/redo support.
fn inspector_ui(mut contexts: EguiContexts, mut inspector: InspectorQuery) {
    let ctx = contexts.ctx_mut();

    egui::SidePanel::right("inspector_panel").show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            render_settings_header(ui, &mut inspector.selection_filter);
            ui.separator();

            if inspector.selection.0.is_empty() {
                ui.label("No selection.");
                return;
            }

            // Extract state from all selected entities
            // We collect entities into a Vec to pass to state extractor
            let entities: Vec<Entity> = inspector.selection.0.iter().copied().collect();
            let state = extract_inspector_state(&entities, &inspector.entity_query);

            ui.heading("Inspector");
            ui.label(format!("Selected: {} entities", entities.len()));
            ui.separator();

            // Transform
            if let Some(mut val) = state.transform {
                ui.collapsing("Transform", |ui| {
                    if inspect_transform(ui, &mut val, &mut |field, action| {
                        match field {
                            TransformField::PosX => {
                                // TODO: Restore alignment using events
                            }
                            TransformField::PosY => {
                                // TODO: Restore alignment using events
                            }
                            TransformField::Rotation => {}
                        }
                    }) {
                        apply_transform_change(&mut inspector, &entities, val);
                    }
                });
                ui.separator();
            }

            // Shape
            if let Some(mut val) = state.editable_shape {
                ui.collapsing("Shape", |ui| {
                    if inspect_shape(ui, &mut val, &mut |field, action| {
                        match field {
                            ShapeField::Width => {
                                // TODO: Restore alignment
                            }
                            ShapeField::Height => {
                                // TODO: Restore alignment
                            }
                            ShapeField::Radius => {
                                // TODO: Restore alignment
                            }
                        }
                    }) {
                        apply_shape_change(&mut inspector, &entities, val);
                    }
                });
                ui.separator();
            }

            // Physics
            ui.collapsing("Physics", |ui| {
                // Rigid Body
                if let Some(mut val) = state.rigid_body {
                    if inspect_rigid_body(ui, &mut val) {
                        apply_rigid_body_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }

                // Sensor
                if let Some(mut val) = state.sensor {
                    if inspect_sensor(ui, &mut val) {
                        apply_sensor_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }

                // Friction
                if let Some(mut val) = state.friction {
                    if inspect_friction(ui, &mut val, &mut |_action| {
                        // TODO: Restore alignment
                    }) {
                        apply_friction_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }

                // Restitution
                if let Some(mut val) = state.restitution {
                    if inspect_restitution(ui, &mut val, &mut |_action| {
                        // TODO: Restore alignment
                    }) {
                        apply_restitution_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }

                // Density
                if let Some(mut val) = state.density {
                    if inspect_density(ui, &mut val, &mut |_action| {
                        // TODO: Restore alignment
                    }) {
                        apply_density_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }

                // Gravity
                if let Some(mut val) = state.gravity_scale {
                    if inspect_gravity(ui, &mut val, &mut |_action| {
                        // TODO: Restore alignment
                    }) {
                        apply_gravity_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }

                // Locked Axes
                if let Some(mut val) = state.locked_axes {
                    if inspect_locked_axes(ui, &mut val) {
                        apply_locked_axes_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }
            });
            ui.separator();

            // Visuals
            ui.collapsing("Visuals", |ui| {
                if let Some(mut val) = state.fill {
                    if inspect_fill(ui, &mut val) {
                        apply_fill_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }
                if let Some(mut val) = state.stroke {
                    if inspect_stroke(ui, &mut val, &mut |_action| {
                        // TODO: Restore alignment
                    }) {
                        apply_stroke_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }
            });
            ui.separator();

            // Joints
            if !entities.is_empty() {
                inspect_joint(ui, &mut inspector, &entities);
            }
        });
    });
}

fn render_settings_header(ui: &mut egui::Ui, selection_filter: &mut SelectionFilter) {
    ui.heading("Settings");
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.radio_value(selection_filter, SelectionFilter::All, "All");
        ui.radio_value(selection_filter, SelectionFilter::Shapes, "Shapes");
        ui.radio_value(selection_filter, SelectionFilter::Joints, "Joints");
    });
}

fn extract_inspector_state(
    entities: &[Entity],
    query: &Query<(
        Entity,
        Option<&Transform>,
        Option<&RigidBody>,
        Option<&Friction>,
        Option<&Restitution>,
        Option<&EditableShape>,
        Option<&Fill>,
        Option<&Stroke>,
        Option<&Sensor>,
        Option<&LockedAxes>,
        Option<&ColliderMassProperties>,
        Option<&GravityScale>,
        Option<&Sleeping>,
    )>,
) -> InspectorState {
    if entities.is_empty() {
        return InspectorState::default();
    }

    // Initialize with first entity
    let first = entities[0];
    let Ok((_, t, rb, f, r, eshape, fill, stroke, sensor, locked, mass, grav, _)) =
        query.get(first)
    else {
        return InspectorState::default();
    };

    // Helper to init logic
    fn init<T: Clone>(val: Option<&T>) -> Option<InspectorValue<T>> {
        val.map(|v| InspectorValue::Same(v.clone()))
    }
    // Specific helpers
    fn init_bool(val: Option<&Sensor>) -> Option<InspectorValue<bool>> {
        Some(InspectorValue::Same(val.is_some()))
    }
    fn init_mass(val: Option<&ColliderMassProperties>) -> Option<InspectorValue<f32>> {
        val.map(|v| {
            if let ColliderMassProperties::Density(d) = v {
                InspectorValue::Same(*d)
            } else {
                InspectorValue::Same(1.0) // fallback
            }
        })
    }
    fn init_grav(val: Option<&GravityScale>) -> Option<InspectorValue<f32>> {
        val.map(|v| InspectorValue::Same(v.0))
    }

    let mut state = InspectorState {
        transform: init(t),
        editable_shape: init(eshape),
        rigid_body: init(rb),
        friction: init(f),
        restitution: init(r),
        fill: init(fill),
        stroke: init(stroke),
        sensor: init_bool(sensor), // Sensor is unit struct, present = true
        locked_axes: init(locked),
        density: init_mass(mass),
        gravity_scale: init_grav(grav),
    };

    // Merge others
    for &entity in &entities[1..] {
        let Ok((_, t, rb, f, r, eshape, fill, stroke, sensor, locked, mass, grav, _)) =
            query.get(entity)
        else {
            continue;
        };

        // Helper merge
        fn merge<T: PartialEq + Clone>(
            state_val: &mut Option<InspectorValue<T>>,
            entity_val: Option<&T>,
        ) {
            if state_val.is_none() {
                return;
            } // Already marked missing
            match entity_val {
                Some(v) => {
                    if let Some(InspectorValue::Same(existing)) = state_val {
                        if existing != v {
                            *state_val = Some(InspectorValue::Mixed);
                        }
                    }
                }
                None => *state_val = None, // Missing on this entity -> don't show
            }
        }

        // Merge Transform
        merge(&mut state.transform, t);
        merge(&mut state.editable_shape, eshape);
        merge(&mut state.rigid_body, rb);
        merge(&mut state.friction, f);
        merge(&mut state.restitution, r);
        merge(&mut state.fill, fill);
        merge(&mut state.stroke, stroke);
        merge(&mut state.locked_axes, locked);

        // Sensor (bool)
        let is_sensor = sensor.is_some();
        if let Some(InspectorValue::Same(existing)) = state.sensor {
            if existing != is_sensor {
                state.sensor = Some(InspectorValue::Mixed);
            }
        }

        // Density
        let d = if let Some(ColliderMassProperties::Density(v)) = mass {
            Some(v)
        } else if mass.is_some() {
            Some(&1.0)
        } else {
            None
        };
        merge(&mut state.density, d);

        // Gravity
        let g = grav.as_ref().map(|v| &v.0);
        merge(&mut state.gravity_scale, g);
    }

    state
}

// UI Helpers

/// Helper wrapper to render a widget with a context menu for alignment/distribution
/// and middle-click to reset.
fn inspect_with_context<T: Clone>(
    ui: &mut egui::Ui,
    val: &mut InspectorValue<T>,
    default: T,
    action: &mut Option<AlignmentAction>,
    render_ui: impl FnOnce(&mut egui::Ui, &mut T) -> egui::Response,
) -> bool {
    // Determine the value to edit. If Mixed, we use default as placeholder but don't commit unless changed.
    let mut current_val = match val {
        InspectorValue::Same(v) => v.clone(),
        InspectorValue::Mixed => default.clone(),
    };

    let response = render_ui(ui, &mut current_val);

    let mut changed = response.changed();

    // Context Menu for Alignment/Distribution
    response.context_menu(|ui| {
        ui.label("Alignment");
        if ui.button("Align Min").clicked() {
            *action = Some(AlignmentAction::Min);
            ui.close_menu();
        }
        if ui.button("Align Center").clicked() {
            *action = Some(AlignmentAction::Center);
            ui.close_menu();
        }
        if ui.button("Align Max").clicked() {
            *action = Some(AlignmentAction::Max);
            ui.close_menu();
        }
        if ui.button("Distribute").clicked() {
            *action = Some(AlignmentAction::Distribute);
            ui.close_menu();
        }
    });

    // Middle click to reset
    if response.clicked_by(egui::PointerButton::Middle) {
        current_val = default;
        changed = true;
    }

    if changed {
        *val = InspectorValue::Same(current_val);
    }

    changed
}

fn inspect_transform(
    ui: &mut egui::Ui,
    val: &mut InspectorValue<Transform>,
    mut on_align: impl FnMut(TransformField, AlignmentAction),
) -> bool {
    let mut changed = false;
    let t = match val {
        InspectorValue::Same(v) => *v,
        InspectorValue::Mixed => Transform::default(),
    };

    let mixed = matches!(val, InspectorValue::Mixed);

    // Decompose to inspect components
    // Position X
    ui.horizontal(|ui| {
        ui.label("Pos X:");
        let mut x_val = InspectorValue::Same(t.translation.x);
        if mixed {
            x_val = InspectorValue::Mixed;
        }

        let mut x_action = None;
        let changed_x = inspect_with_context(ui, &mut x_val, 0.0, &mut x_action, |ui, val| {
            ui.add(egui::DragValue::new(val).speed(DRAG_SPEED))
        });
        if let Some(act) = x_action {
            on_align(TransformField::PosX, act);
        }

        if changed_x {
            if let InspectorValue::Same(new_x) = x_val {
                let mut new_t = t;
                new_t.translation.x = new_x;
                *val = InspectorValue::Same(new_t);
                changed = true;
            }
        }

        ui.label("Pos Y:");
        let mut y_val = InspectorValue::Same(t.translation.y);
        if mixed {
            y_val = InspectorValue::Mixed;
        }

        let mut y_action = None;
        let changed_y = inspect_with_context(ui, &mut y_val, 0.0, &mut y_action, |ui, val| {
            ui.add(egui::DragValue::new(val).speed(DRAG_SPEED))
        });
        if let Some(act) = y_action {
            on_align(TransformField::PosY, act);
        }

        if changed_y {
            if let InspectorValue::Same(new_y) = y_val {
                // If x changed in same frame, update that too? Unlikely with immediate mode separate widgets.
                // We re-read `t` which might be stale if `x` changed above?
                // Actually `val` was updated above if x changed. So we should re-read `val`.
                let current_t = match val {
                    InspectorValue::Same(v) => *v,
                    _ => t,
                };
                let mut new_t = current_t;
                new_t.translation.y = new_y;
                *val = InspectorValue::Same(new_t);
                changed = true;
            }
        }
    });

    // Rotation
    ui.horizontal(|ui| {
        ui.label("Rotation:");
        let current_t = match val {
            InspectorValue::Same(v) => *v,
            _ => t,
        };
        let mut rot_val = InspectorValue::Same(current_t.rotation.to_euler(EulerRot::XYZ).2);
        if mixed && !changed {
            rot_val = InspectorValue::Mixed;
        } // careful with mixed logic

        let mut rot_action = None;
        let changed_rot = inspect_with_context(ui, &mut rot_val, 0.0, &mut rot_action, |ui, val| {
            ui.drag_angle(val)
        });
        if let Some(act) = rot_action {
            on_align(TransformField::Rotation, act);
        }

        if changed_rot {
            if let InspectorValue::Same(new_rot) = rot_val {
                let mut new_t = current_t;
                new_t.rotation = Quat::from_rotation_z(new_rot);
                *val = InspectorValue::Same(new_t);
                changed = true;
            }
        }
    });

    if mixed && !changed {
        ui.label("(Mixed Values - Edit to set all)");
    }

    changed
}

fn inspect_shape(
    ui: &mut egui::Ui,
    val: &mut InspectorValue<EditableShape>,
    mut on_align: impl FnMut(ShapeField, AlignmentAction),
) -> bool {
    let mut eshape = match val {
        InspectorValue::Same(v) => v.clone(),
        InspectorValue::Mixed => return false, // Too complex to edit mixed shapes
    };

    // Shape editing is complex because of nested fields.
    // We can't easily wrap the entire shape struct in `inspect_with_context`.
    // We would need to wrap individual fields (width, height, radius).

    let mut changed = false;
    match &mut eshape.shape {
        ShapeType::Box { width, height } => {
            ui.label("Box Dimensions");

            // Width
            let mut w_val = InspectorValue::Same(*width);
            let mut w_action = None;
            let w_changed = inspect_with_context(ui, &mut w_val, 1.0, &mut w_action, |ui, val| {
                ui.add(
                    egui::DragValue::new(val)
                        .speed(DRAG_SPEED)
                        .prefix("Width: "),
                )
            });
            if let Some(act) = w_action {
                on_align(ShapeField::Width, act);
            }
            if w_changed {
                if let InspectorValue::Same(w) = w_val {
                    *width = w;
                    changed = true;
                }
            }

            // Height
            let mut h_val = InspectorValue::Same(*height);
            let mut h_action = None;
            let h_changed = inspect_with_context(ui, &mut h_val, 1.0, &mut h_action, |ui, val| {
                ui.add(
                    egui::DragValue::new(val)
                        .speed(DRAG_SPEED)
                        .prefix("Height: "),
                )
            });
            if let Some(act) = h_action {
                on_align(ShapeField::Height, act);
            }
            if h_changed {
                if let InspectorValue::Same(h) = h_val {
                    *height = h;
                    changed = true;
                }
            }
        }
        ShapeType::Circle { radius } => {
            ui.label("Circle Dimensions");
            let mut r_val = InspectorValue::Same(*radius);
            let mut r_action = None;
            let r_changed = inspect_with_context(ui, &mut r_val, 1.0, &mut r_action, |ui, val| {
                ui.add(
                    egui::DragValue::new(val)
                        .speed(DRAG_SPEED)
                        .prefix("Radius: "),
                )
            });
            if let Some(act) = r_action {
                on_align(ShapeField::Radius, act);
            }
            if r_changed {
                if let InspectorValue::Same(r) = r_val {
                    *radius = r;
                    changed = true;
                }
            }
        }
        ShapeType::Polygon { points } => {
            ui.label("Polygon");
            ui.label(format!("Vertices: {}", points.len()));
        }
    }

    if changed {
        *val = InspectorValue::Same(eshape);
    }
    changed
}

fn inspect_rigid_body(ui: &mut egui::Ui, val: &mut InspectorValue<RigidBody>) -> bool {
    // RigidBody enum doesn't map well to "Middle Click Reset" (what is default?)
    // or alignment. So we skip the wrapper for this one, or just use it for context menu only.
    let mut rb = match val {
        InspectorValue::Same(v) => *v,
        InspectorValue::Mixed => RigidBody::Dynamic, // default
    };
    let mixed = matches!(val, InspectorValue::Mixed);

    ui.horizontal(|ui| {
        ui.label("Type:");
        let options = [
            RigidBody::Dynamic,
            RigidBody::Fixed,
            RigidBody::KinematicPositionBased,
        ];

        let text = if mixed {
            "Mixed".to_string()
        } else {
            format!("{:?}", rb)
        };

        egui::ComboBox::from_id_salt("rb_type")
            .selected_text(text)
            .show_ui(ui, |ui| {
                for option in options {
                    if ui
                        .selectable_value(&mut rb, option, format!("{:?}", option))
                        .clicked()
                    {
                        *val = InspectorValue::Same(rb);
                    }
                }
            });
    });

    if let InspectorValue::Same(_new_rb) = *val {
        return !matches!(val, InspectorValue::Mixed) || !mixed;
    }
    false
}

fn inspect_friction(
    ui: &mut egui::Ui,
    val: &mut InspectorValue<Friction>,
    mut on_align: impl FnMut(AlignmentAction),
) -> bool {
    let mut f_val = match val {
        InspectorValue::Same(v) => InspectorValue::Same(v.coefficient),
        InspectorValue::Mixed => InspectorValue::Mixed,
    };
    let mut action = None;
    let changed = inspect_with_context(ui, &mut f_val, 0.5, &mut action, |ui, val| {
        ui.add(egui::Slider::new(val, 0.0..=FRICTION_MAX).text("Friction"))
    });
    if let Some(act) = action {
        on_align(act);
    }

    if match val {
        InspectorValue::Mixed => true,
        _ => false,
    } {
        ui.label("(Mixed)");
    }

    if changed {
        if let InspectorValue::Same(coef) = f_val {
            *val = InspectorValue::Same(Friction::coefficient(coef));
        }
    }
    changed
}

fn inspect_restitution(
    ui: &mut egui::Ui,
    val: &mut InspectorValue<Restitution>,
    mut on_align: impl FnMut(AlignmentAction),
) -> bool {
    let mut r_val = match val {
        InspectorValue::Same(v) => InspectorValue::Same(v.coefficient),
        InspectorValue::Mixed => InspectorValue::Mixed,
    };
    let mut action = None;
    let changed = inspect_with_context(ui, &mut r_val, 0.0, &mut action, |ui, val| {
        ui.add(egui::Slider::new(val, 0.0..=RESTITUTION_MAX).text("Restitution"))
    });
    if let Some(act) = action {
        on_align(act);
    }

    if match val {
        InspectorValue::Mixed => true,
        _ => false,
    } {
        ui.label("(Mixed)");
    }

    if changed {
        if let InspectorValue::Same(coef) = r_val {
            *val = InspectorValue::Same(Restitution::coefficient(coef));
        }
    }
    changed
}

fn inspect_density(
    ui: &mut egui::Ui,
    val: &mut InspectorValue<f32>,
    mut on_align: impl FnMut(AlignmentAction),
) -> bool {
    let mut action = None;
    let changed = inspect_with_context(ui, val, 1.0, &mut action, |ui, val| {
        ui.add(
            egui::DragValue::new(val)
                .speed(DRAG_SPEED)
                .range(DENSITY_MIN..=DENSITY_MAX)
                .prefix("Density: "),
        )
    });
    if let Some(act) = action {
        on_align(act);
    }
    if match val {
        InspectorValue::Mixed => true,
        _ => false,
    } {
        ui.label("(Mixed)");
    }
    changed
}

fn inspect_gravity(
    ui: &mut egui::Ui,
    val: &mut InspectorValue<f32>,
    mut on_align: impl FnMut(AlignmentAction),
) -> bool {
    let mut action = None;
    let changed = inspect_with_context(ui, val, 1.0, &mut action, |ui, val| {
        ui.add(
            egui::DragValue::new(val)
                .speed(DRAG_SPEED)
                .prefix("Gravity Scale: "),
        )
    });
    if let Some(act) = action {
        on_align(act);
    }
    if match val {
        InspectorValue::Mixed => true,
        _ => false,
    } {
        ui.label("(Mixed)");
    }
    changed
}

fn inspect_sensor(ui: &mut egui::Ui, val: &mut InspectorValue<bool>) -> bool {
    let mut is_sensor = match val {
        InspectorValue::Same(v) => *v,
        InspectorValue::Mixed => false,
    };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;

    let label = if mixed { "Sensor (Mixed)" } else { "Sensor" };
    if ui.checkbox(&mut is_sensor, label).clicked() {
        changed = true;
    }

    // Checkbox middle click reset is less standard, but we can do context menu if we wrap it.
    // ui.checkbox returns Response.

    if changed {
        *val = InspectorValue::Same(is_sensor);
    }
    changed
}

fn inspect_locked_axes(ui: &mut egui::Ui, val: &mut InspectorValue<LockedAxes>) -> bool {
    let mut locked = match val {
        InspectorValue::Same(v) => v.contains(LockedAxes::ROTATION_LOCKED),
        InspectorValue::Mixed => false,
    };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;

    let label = if mixed {
        "Lock Rotation (Mixed)"
    } else {
        "Lock Rotation"
    };
    if ui.checkbox(&mut locked, label).clicked() {
        changed = true;
    }

    if changed {
        *val = InspectorValue::Same(if locked {
            LockedAxes::ROTATION_LOCKED
        } else {
            LockedAxes::empty()
        });
    }
    changed
}

fn inspect_fill(ui: &mut egui::Ui, val: &mut InspectorValue<Fill>) -> bool {
    let mut color = match val {
        InspectorValue::Same(v) => v.color,
        InspectorValue::Mixed => Color::WHITE,
    };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label("Color:");
        let mut c_arr = color.to_srgba().to_f32_array();
        if ui.color_edit_button_rgba_unmultiplied(&mut c_arr).changed() {
            color = Color::srgba(c_arr[0], c_arr[1], c_arr[2], c_arr[3]);
            changed = true;
        }
        if mixed {
            ui.label("(Mixed)");
        }
    });

    if changed {
        // Create dummy Fill to pass back
        let f = Fill {
            color,
            options: FillOptions::default(),
        };
        *val = InspectorValue::Same(f);
    }
    changed
}

fn inspect_stroke(
    ui: &mut egui::Ui,
    val: &mut InspectorValue<Stroke>,
    mut on_align: impl FnMut(AlignmentAction),
) -> bool {
    let mut color = match val {
        InspectorValue::Same(v) => v.color,
        InspectorValue::Mixed => Color::BLACK,
    };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;

    ui.horizontal(|ui| {
        let mut c_arr = color.to_srgba().to_f32_array();
        if ui.color_edit_button_rgba_unmultiplied(&mut c_arr).changed() {
            color = Color::srgba(c_arr[0], c_arr[1], c_arr[2], c_arr[3]);
            changed = true;
        }

        // Width
        let mut width_val = InspectorValue::Same(match val {
            InspectorValue::Same(v) => v.options.line_width,
            InspectorValue::Mixed => 1.0,
        });
        if mixed {
            width_val = InspectorValue::Mixed;
        }

        let mut action = None;
        let width_changed = inspect_with_context(ui, &mut width_val, 1.0, &mut action, |ui, val| {
            ui.add(egui::DragValue::new(val).speed(0.1).prefix("Width: "))
        });
        if let Some(act) = action {
            on_align(act);
        }

        if width_changed {
            changed = true;
        }
    });

    if mixed {
        ui.label("(Mixed Visuals)");
    }

    if changed {
        let width = match val {
            InspectorValue::Same(v) => v.options.line_width,
            InspectorValue::Mixed => 1.0,
        }; // Fallback to current/default
        let s = Stroke {
            color,
            options: StrokeOptions::default().with_line_width(width),
        };
        *val = InspectorValue::Same(s);
    }
    changed
}

// Appliers - Now emitting events
fn apply_transform_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<Transform>,
) {
    if let InspectorValue::Same(t) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::Transform(t),
            });
        }
    }
}

fn apply_shape_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<EditableShape>,
) {
    if let InspectorValue::Same(s) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::Shape(s.shape.clone()),
            });
        }
    }
}

fn apply_rigid_body_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<RigidBody>,
) {
    if let InspectorValue::Same(rb) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::RigidBody(rb),
            });
        }
    }
}

fn apply_friction_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<Friction>,
) {
    if let InspectorValue::Same(f) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::Friction(f.coefficient),
            });
        }
    }
}

fn apply_restitution_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<Restitution>,
) {
    if let InspectorValue::Same(r) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::Restitution(r.coefficient),
            });
        }
    }
}

fn apply_density_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<f32>,
) {
    if let InspectorValue::Same(d) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::Density(d),
            });
        }
    }
}

fn apply_gravity_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<f32>,
) {
    if let InspectorValue::Same(g) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::GravityScale(g),
            });
        }
    }
}

fn apply_sensor_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<bool>,
) {
    if let InspectorValue::Same(is_sensor) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::Sensor(is_sensor),
            });
        }
    }
}

fn apply_locked_axes_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<LockedAxes>,
) {
    if let InspectorValue::Same(l) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::LockedAxes(l),
            });
        }
    }
}

fn apply_fill_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<Fill>,
) {
    if let InspectorValue::Same(f) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::FillColor(f.color),
            });
        }
    }
}

fn apply_stroke_change(
    inspector: &mut InspectorQuery,
    entities: &[Entity],
    val: InspectorValue<Stroke>,
) {
    if let InspectorValue::Same(s) = val {
        for &e in entities {
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::StrokeColor(s.color),
            });
            inspector.events.send(PropertyChangeEvent {
                entity: e,
                change: PropertyChange::StrokeWidth(s.options.line_width),
            });
        }
    }
}

fn wake_up(entity: Entity, inspector: &mut InspectorQuery) {
    // Wake up is now handled by the event handler
}

fn inspect_joint(ui: &mut egui::Ui, inspector: &mut InspectorQuery, entities: &[Entity]) {
    let resolve = |e: Entity, insp: &InspectorQuery| -> Entity {
        if let Ok(connector) = insp.connector_query.get(e) {
            connector.entity_a
        } else {
            e
        }
    };

    let first = resolve(entities[0], inspector);

    // Read initial values from first entity
    let mut rev_params = None;
    let mut prism_params = None;

    if let Ok(joint) = inspector.joint_query.get(first) {
        match &joint.data {
            TypedJoint::RevoluteJoint(rev) => {
                let limits = if let Some(l) = rev.limits() {
                    [l.min, l.max]
                } else {
                    [-std::f32::consts::PI, std::f32::consts::PI]
                };
                let raw = rev.data.raw.as_revolute().unwrap();
                let motor = &raw.data.motors[2]; // Z axis
                rev_params = Some((
                    limits[0],
                    limits[1],
                    motor.target_vel,
                    motor.damping,
                    motor.max_force,
                ));
            }
            TypedJoint::PrismaticJoint(prism) => {
                let limits = if let Some(l) = prism.limits() {
                    [l.min, l.max]
                } else {
                    [PRISMATIC_MIN_DEFAULT, PRISMATIC_MAX_DEFAULT]
                };
                let raw = prism.data.raw.as_prismatic().unwrap();
                let motor = &raw.data.motors[0]; // X axis
                prism_params = Some((
                    limits[0],
                    limits[1],
                    motor.target_vel,
                    motor.damping,
                    motor.max_force,
                ));
            }
            TypedJoint::FixedJoint(_) => {
                ui.label("Fixed Joint (No limits)");
            }
            _ => {
                ui.label("Generic/Other Joint Type");
            }
        }
    }

    if let Some((min, max, vel, damp, force)) = rev_params {
        let mut min_deg = min.to_degrees();
        let mut max_deg = max.to_degrees();
        let mut target_vel = vel.to_degrees();
        let mut damping = damp;
        let mut max_force = force;

        ui.heading("Revolute Settings");
        let mut changed = false;

        ui.label("Limits (Deg)");
        if ui
            .add(
                egui::DragValue::new(&mut min_deg)
                    .speed(1.0)
                    .prefix("Min: "),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut max_deg)
                    .speed(1.0)
                    .prefix("Max: "),
            )
            .changed()
        {
            changed = true;
        }

        ui.separator();
        ui.label("Motor Settings");
        if ui
            .add(
                egui::DragValue::new(&mut target_vel)
                    .speed(1.0)
                    .prefix("Speed (deg/s): "),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut damping)
                    .speed(0.1)
                    .prefix("Damping: "),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut max_force)
                    .speed(10.0)
                    .prefix("Max Force: "),
            )
            .changed()
        {
            changed = true;
        }

        if changed {
            for &e in entities {
                let target = resolve(e, inspector);
                inspector.events.send(PropertyChangeEvent {
                    entity: target,
                    change: PropertyChange::RevoluteLimits([min_deg.to_radians(), max_deg.to_radians()]),
                });
                inspector.events.send(PropertyChangeEvent {
                    entity: target,
                    change: PropertyChange::RevoluteMotor {
                        target_vel: target_vel.to_radians(),
                        damping,
                        max_force,
                    },
                });
            }
        }
    } else if let Some((min, max, vel, damp, force)) = prism_params {
        let mut min_val = min;
        let mut max_val = max;
        let mut target_vel = vel;
        let mut damping = damp;
        let mut max_force = force;

        ui.heading("Prismatic Settings");
        let mut changed = false;

        ui.label("Limits");
        if ui
            .add(
                egui::DragValue::new(&mut min_val)
                    .speed(DRAG_SPEED)
                    .prefix("Min: "),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut max_val)
                    .speed(DRAG_SPEED)
                    .prefix("Max: "),
            )
            .changed()
        {
            changed = true;
        }

        ui.separator();
        ui.label("Motor Settings");
        if ui
            .add(
                egui::DragValue::new(&mut target_vel)
                    .speed(0.1)
                    .prefix("Speed: "),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut damping)
                    .speed(0.1)
                    .prefix("Damping: "),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut max_force)
                    .speed(10.0)
                    .prefix("Max Force: "),
            )
            .changed()
        {
            changed = true;
        }

        if changed {
            for &e in entities {
                let target = resolve(e, inspector);
                inspector.events.send(PropertyChangeEvent {
                    entity: target,
                    change: PropertyChange::PrismaticLimits([min_val, max_val]),
                });
                inspector.events.send(PropertyChangeEvent {
                    entity: target,
                    change: PropertyChange::PrismaticMotor {
                        target_vel,
                        damping,
                        max_force,
                    },
                });
            }
        }
    }
}

fn apply_alignment<'w, 's, F>(
    inspector: &mut InspectorQuery<'w, 's>,
    entities: &[Entity],
    mut accessor: F,
    action: AlignmentAction,
) where
    F: for<'a> FnMut(Entity, &'a mut InspectorQuery<'w, 's>) -> Option<&'a mut f32>,
{
    if entities.len() < 2 {
        return;
    }

    // 1. Read values
    let mut values: Vec<(Entity, f32)> = Vec::with_capacity(entities.len());
    for &e in entities {
        if let Some(val) = accessor(e, inspector) {
            values.push((e, *val));
        }
    }

    if values.is_empty() {
        return;
    }

    // 2. Sort
    values.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let min = values.first().unwrap().1;
    let max = values.last().unwrap().1;

    match action {
        AlignmentAction::Min => {
            for (e, _) in values {
                if let Some(val) = accessor(e, inspector) {
                    *val = min;
                }
            }
        }
        AlignmentAction::Max => {
            for (e, _) in values {
                if let Some(val) = accessor(e, inspector) {
                    *val = max;
                }
            }
        }
        AlignmentAction::Center => {
            let center = (min + max) * 0.5;
            for (e, _) in values {
                if let Some(val) = accessor(e, inspector) {
                    *val = center;
                }
            }
        }
        AlignmentAction::Distribute => {
            if values.len() < 3 {
                return;
            }
            let count = values.len() as f32;
            let step = (max - min) / (count - 1.0);

            for (i, (e, _)) in values.iter().enumerate() {
                let target = min + (i as f32) * step;
                if let Some(val) = accessor(*e, inspector) {
                    *val = target;
                }
            }
        }
    }
}
