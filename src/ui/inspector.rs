//! Inspector panel for editing entity properties.
//!
//! Provides an Egui sidebar that allows modifying properties of selected entities,
//! such as Transform, RigidBody type, Friction, and Restitution.

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
    joint_query: Query<'w, 's, &'static mut ImpulseJoint>,
    #[expect(
        clippy::type_complexity,
        reason = "Large query tuple required for inspector"
    )]
    entity_query: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static mut Transform>,
            Option<&'static mut RigidBody>,
            Option<&'static mut Friction>,
            Option<&'static mut Restitution>,
            Option<&'static mut EditableShape>,
            Option<&'static mut Fill>,
            Option<&'static mut Stroke>,
            Option<&'static mut Sensor>,
            Option<&'static mut LockedAxes>,
            Option<&'static mut ColliderMassProperties>,
            Option<&'static mut GravityScale>,
            Option<&'static mut Sleeping>,
        ),
    >,
    commands: Commands<'w, 's>,
}

#[derive(Default)]
struct InspectorState {
    transform: Option<Transform>,
    editable_shape: Option<EditableShape>,
    rigid_body: Option<RigidBody>,
    friction: Option<Friction>,
    restitution: Option<Restitution>,
    fill: Option<Fill>,
    stroke: Option<Stroke>,
    sensor: bool,
    locked_axes: Option<LockedAxes>,
    density: Option<f32>,
    gravity_scale: Option<f32>,
}

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

            // Inspect the first selected entity for initial values
            let first_entity = inspector.selection.0.iter().next().copied();

            if let Some(entity) = first_entity {
                let state = extract_inspector_state(entity, &inspector.entity_query);

                ui.heading("Inspector");
                ui.label(format!(
                    "Selected: {} entities",
                    inspector.selection.0.len()
                ));
                ui.separator();

                inspect_transform(ui, &mut inspector, &state);
                inspect_shape(ui, &mut inspector, &state);
                inspect_physics(ui, &mut inspector, &state);
                inspect_visuals(ui, &mut inspector, &state);
                inspect_joint(ui, &mut inspector, entity);
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
    entity: Entity,
    query: &Query<(
        Entity,
        Option<&mut Transform>,
        Option<&mut RigidBody>,
        Option<&mut Friction>,
        Option<&mut Restitution>,
        Option<&mut EditableShape>,
        Option<&mut Fill>,
        Option<&mut Stroke>,
        Option<&mut Sensor>,
        Option<&mut LockedAxes>,
        Option<&mut ColliderMassProperties>,
        Option<&mut GravityScale>,
        Option<&mut Sleeping>,
    )>,
) -> InspectorState {
    let mut state = InspectorState::default();

    if let Ok((_, t, rb, f, r, eshape, fill, stroke, sensor, locked, mass, grav, _)) =
        query.get(entity)
    {
        if let Some(v) = t {
            state.transform = Some(*v);
        }
        if let Some(v) = eshape {
            state.editable_shape = Some(v.clone());
        }
        if let Some(v) = rb {
            state.rigid_body = Some(*v);
        }
        if let Some(v) = f {
            state.friction = Some(*v);
        }
        if let Some(v) = r {
            state.restitution = Some(*v);
        }
        if let Some(v) = fill {
            state.fill = Some(*v);
        }
        if let Some(v) = stroke {
            state.stroke = Some(*v);
        }
        if sensor.is_some() {
            state.sensor = true;
        }
        if let Some(v) = locked {
            state.locked_axes = Some(*v);
        }
        if let Some(ColliderMassProperties::Density(d)) = mass {
            state.density = Some(*d);
        } else if mass.is_some() {
            // Default density if mass props exist but not density
            state.density = Some(1.0);
        }
        if let Some(v) = grav {
            state.gravity_scale = Some(v.0);
        }
    }
    state
}

fn inspect_transform(ui: &mut egui::Ui, inspector: &mut InspectorQuery, state: &InspectorState) {
    if let Some(mut transform) = state.transform {
        ui.heading("Transform");
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Pos X:");
            if ui
                .add(egui::DragValue::new(&mut transform.translation.x).speed(DRAG_SPEED))
                .changed()
            {
                changed = true;
            }
            ui.label("Pos Y:");
            if ui
                .add(egui::DragValue::new(&mut transform.translation.y).speed(DRAG_SPEED))
                .changed()
            {
                changed = true;
            }
        });

        let mut rotation = transform.rotation.to_euler(EulerRot::XYZ).2;
        ui.horizontal(|ui| {
            ui.label("Rotation:");
            if ui.drag_angle(&mut rotation).changed() {
                transform.rotation = Quat::from_rotation_z(rotation);
                changed = true;
            }
        });

        if changed {
            for &e in &inspector.selection.0 {
                if let Ok((_, Some(mut t), ..)) = inspector.entity_query.get_mut(e) {
                    t.translation = transform.translation;
                    t.rotation = transform.rotation;
                    // Wake up body if exists
                    if let Ok((_, _, Some(_), ..)) = inspector.entity_query.get(e) {
                        inspector.commands.entity(e).insert(Sleeping::disabled());
                    }
                }
            }
        }
        ui.separator();
    }
}

fn inspect_shape(ui: &mut egui::Ui, inspector: &mut InspectorQuery, state: &InspectorState) {
    if let Some(mut eshape) = state.editable_shape.clone() {
        let mut changed = false;
        match &mut eshape.shape {
            ShapeType::Box { width, height } => {
                ui.heading("Box Dimensions");
                if ui
                    .add(
                        egui::DragValue::new(width)
                            .speed(DRAG_SPEED)
                            .prefix("Width: "),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(height)
                            .speed(DRAG_SPEED)
                            .prefix("Height: "),
                    )
                    .changed()
                {
                    changed = true;
                }
            }
            ShapeType::Circle { radius } => {
                ui.heading("Circle Dimensions");
                if ui
                    .add(
                        egui::DragValue::new(radius)
                            .speed(DRAG_SPEED)
                            .prefix("Radius: "),
                    )
                    .changed()
                {
                    changed = true;
                }
            }
            ShapeType::Polygon { points } => {
                ui.heading("Polygon");
                ui.label(format!("Vertices: {}", points.len()));
            }
        }

        if changed {
            for &e in &inspector.selection.0 {
                if let Ok((_, _, _, _, _, Some(mut s), ..)) = inspector.entity_query.get_mut(e) {
                    s.shape = eshape.shape.clone();
                }
            }
        }
        ui.separator();
    }
}

fn inspect_physics(ui: &mut egui::Ui, inspector: &mut InspectorQuery, state: &InspectorState) {
    let has_rb = state.rigid_body.is_some();

    // Rigid Body Type
    if let Some(mut rb) = state.rigid_body {
        ui.heading("Rigid Body");
        let options = [
            RigidBody::Dynamic,
            RigidBody::Fixed,
            RigidBody::KinematicPositionBased,
        ];
        egui::ComboBox::from_label("Type")
            .selected_text(format!("{:?}", rb))
            .show_ui(ui, |ui| {
                for option in options {
                    if ui
                        .selectable_value(&mut rb, option, format!("{:?}", option))
                        .clicked()
                    {
                        for &e in &inspector.selection.0 {
                            inspector
                                .commands
                                .entity(e)
                                .insert(rb)
                                .insert(Sleeping::disabled());
                        }
                    }
                }
            });
        ui.separator();
    }

    // Sensor
    if has_rb || state.sensor {
        let mut is_sensor = state.sensor;
        if ui.checkbox(&mut is_sensor, "Sensor").clicked() {
            for &e in &inspector.selection.0 {
                if is_sensor {
                    inspector.commands.entity(e).insert(Sensor);
                } else {
                    inspector.commands.entity(e).remove::<Sensor>();
                }
            }
        }
    }

    // Friction
    if let Some(mut friction) = state.friction {
        ui.heading("Friction");
        if ui
            .add(
                egui::Slider::new(&mut friction.coefficient, 0.0..=FRICTION_MAX)
                    .text("Coefficient"),
            )
            .changed()
        {
            for &e in &inspector.selection.0 {
                if let Ok((_, _, _, f, ..)) = inspector.entity_query.get_mut(e) {
                    if let Some(mut f_comp) = f {
                        f_comp.coefficient = friction.coefficient;
                    } else {
                        inspector
                            .commands
                            .entity(e)
                            .insert(Friction::coefficient(friction.coefficient));
                    }
                }
            }
        }
        ui.separator();
    } else if has_rb {
        ui.heading("Friction");
        if ui.button("Add Friction").clicked() {
            for &e in &inspector.selection.0 {
                inspector.commands.entity(e).insert(Friction::default());
            }
        }
        ui.separator();
    }

    // Restitution
    if let Some(mut restitution) = state.restitution {
        ui.heading("Restitution");
        if ui
            .add(
                egui::Slider::new(&mut restitution.coefficient, 0.0..=RESTITUTION_MAX)
                    .text("Coefficient"),
            )
            .changed()
        {
            for &e in &inspector.selection.0 {
                if let Ok((_, _, _, _, r, ..)) = inspector.entity_query.get_mut(e) {
                    if let Some(mut r_comp) = r {
                        r_comp.coefficient = restitution.coefficient;
                    } else {
                        inspector
                            .commands
                            .entity(e)
                            .insert(Restitution::coefficient(restitution.coefficient));
                    }
                }
            }
        }
        ui.separator();
    } else if has_rb {
        ui.heading("Restitution");
        if ui.button("Add Restitution").clicked() {
            for &e in &inspector.selection.0 {
                inspector.commands.entity(e).insert(Restitution::default());
            }
        }
        ui.separator();
    }

    // Density
    if let Some(mut density) = state.density {
        ui.heading("Density");
        if ui
            .add(
                egui::DragValue::new(&mut density)
                    .speed(DRAG_SPEED)
                    .range(DENSITY_MIN..=DENSITY_MAX),
            )
            .changed()
        {
            for &e in &inspector.selection.0 {
                inspector
                    .commands
                    .entity(e)
                    .insert(ColliderMassProperties::Density(density));
                inspector.commands.entity(e).insert(Sleeping::disabled());
            }
        }
        ui.separator();
    } else if has_rb {
        ui.heading("Density");
        if ui.button("Set Density").clicked() {
            for &e in &inspector.selection.0 {
                inspector
                    .commands
                    .entity(e)
                    .insert(ColliderMassProperties::Density(1.0));
            }
        }
        ui.separator();
    }

    // Gravity Scale
    if let Some(mut gravity) = state.gravity_scale {
        ui.heading("Gravity Scale");
        if ui
            .add(egui::DragValue::new(&mut gravity).speed(DRAG_SPEED))
            .changed()
        {
            for &e in &inspector.selection.0 {
                inspector.commands.entity(e).insert(GravityScale(gravity));
                inspector.commands.entity(e).insert(Sleeping::disabled());
            }
        }
        ui.separator();
    } else if has_rb {
        ui.heading("Gravity Scale");
        if ui.button("Add Gravity Scale").clicked() {
            for &e in &inspector.selection.0 {
                inspector.commands.entity(e).insert(GravityScale(1.0));
            }
        }
        ui.separator();
    }

    // Locked Axes
    if let Some(locked_axes) = state.locked_axes {
        ui.heading("Locked Axes");
        let mut locked = locked_axes.contains(LockedAxes::ROTATION_LOCKED);
        if ui.checkbox(&mut locked, "Lock Rotation").clicked() {
            for &e in &inspector.selection.0 {
                if locked {
                    inspector
                        .commands
                        .entity(e)
                        .insert(LockedAxes::ROTATION_LOCKED);
                } else {
                    inspector.commands.entity(e).insert(LockedAxes::empty());
                }
                inspector.commands.entity(e).insert(Sleeping::disabled());
            }
        }
        ui.separator();
    } else if has_rb {
        ui.heading("Locked Axes");
        if ui.button("Add Axis Locking").clicked() {
            for &e in &inspector.selection.0 {
                inspector.commands.entity(e).insert(LockedAxes::empty());
            }
        }
        ui.separator();
    }
}

fn inspect_visuals(ui: &mut egui::Ui, inspector: &mut InspectorQuery, state: &InspectorState) {
    // Fill Color
    if let Some(fill) = state.fill {
        ui.heading("Fill Color");
        let mut color_arr = fill.color.to_srgba().to_f32_array();
        if ui
            .color_edit_button_rgba_unmultiplied(&mut color_arr)
            .changed()
        {
            let new_color = Color::srgba(color_arr[0], color_arr[1], color_arr[2], color_arr[3]);
            for &e in &inspector.selection.0 {
                if let Ok((_, _, _, _, _, _, _, Some(mut f), ..)) =
                    inspector.entity_query.get_mut(e)
                {
                    f.color = new_color;
                }
            }
        }
        ui.separator();
    }

    // Stroke
    if let Some(stroke) = state.stroke {
        ui.heading("Stroke");
        let mut color_arr = stroke.color.to_srgba().to_f32_array();
        let mut line_width = stroke.options.line_width;
        let mut changed = false;

        ui.horizontal(|ui| {
            if ui
                .color_edit_button_rgba_unmultiplied(&mut color_arr)
                .changed()
            {
                changed = true;
            }
            if ui
                .add(
                    egui::DragValue::new(&mut line_width)
                        .speed(DRAG_SPEED)
                        .prefix("Width: "),
                )
                .changed()
            {
                changed = true;
            }
        });

        if changed {
            let new_color = Color::srgba(color_arr[0], color_arr[1], color_arr[2], color_arr[3]);
            for &e in &inspector.selection.0 {
                if let Ok((_, _, _, _, _, _, _, Some(mut s), ..)) =
                    inspector.entity_query.get_mut(e)
                {
                    s.color = new_color;
                    s.options.line_width = line_width;
                }
            }
        }
        ui.separator();
    }
}

fn inspect_joint(ui: &mut egui::Ui, inspector: &mut InspectorQuery, first_entity: Entity) {
    let mut joint_entity = first_entity;
    if let Ok(connector) = inspector.connector_query.get(first_entity) {
        joint_entity = connector.entity_a;
    }

    if let Ok(mut joint) = inspector.joint_query.get_mut(joint_entity) {
        ui.heading("Joint Settings");

        match &mut joint.data {
            TypedJoint::RevoluteJoint(rev) => {
                let current_limits = if let Some(l) = rev.limits() {
                    [l.min, l.max]
                } else {
                    [-std::f32::consts::PI, std::f32::consts::PI]
                };
                let mut min = current_limits[0];
                let mut max = current_limits[1];

                ui.label("Revolute Limits");
                let mut changed = false;
                if ui
                    .add(
                        egui::DragValue::new(&mut min)
                            .speed(DRAG_SPEED)
                            .prefix("Min: "),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut max)
                            .speed(DRAG_SPEED)
                            .prefix("Max: "),
                    )
                    .changed()
                {
                    changed = true;
                }

                if changed {
                    rev.set_limits([min, max]);
                }
            }
            TypedJoint::PrismaticJoint(prism) => {
                let current_limits = if let Some(l) = prism.limits() {
                    [l.min, l.max]
                } else {
                    [PRISMATIC_MIN_DEFAULT, PRISMATIC_MAX_DEFAULT]
                };
                let mut min = current_limits[0];
                let mut max = current_limits[1];

                ui.label("Prismatic Limits");
                let mut changed = false;
                if ui
                    .add(
                        egui::DragValue::new(&mut min)
                            .speed(DRAG_SPEED)
                            .prefix("Min: "),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut max)
                            .speed(DRAG_SPEED)
                            .prefix("Max: "),
                    )
                    .changed()
                {
                    changed = true;
                }

                if changed {
                    prism.set_limits([min, max]);
                }
            }
            TypedJoint::FixedJoint(_) => {
                ui.label("Fixed Joint (No limits)");
            }
            _ => {
                ui.label("Generic/Other Joint Type");
            }
        }
        ui.separator();
    }
}
