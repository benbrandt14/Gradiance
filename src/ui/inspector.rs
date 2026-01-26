//! Inspector panel for editing entity properties.
//!
//! Provides an Egui sidebar that allows modifying properties of selected entities,
//! such as Transform, RigidBody type, Friction, and Restitution.

use crate::input::editable::{EditableBox, EditableCircle};
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
const DEFAULT_STROKE_WIDTH: f32 = 1.0;

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
            Option<&'static mut EditableBox>,
            Option<&'static mut EditableCircle>,
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

fn inspector_ui(mut contexts: EguiContexts, mut inspector: InspectorQuery) {
    let ctx = contexts.ctx_mut();

    egui::SidePanel::right("inspector_panel").show(ctx, |ui| {
        ui.heading("Settings");
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.radio_value(
                &mut *inspector.selection_filter,
                SelectionFilter::All,
                "All",
            );
            ui.radio_value(
                &mut *inspector.selection_filter,
                SelectionFilter::Shapes,
                "Shapes",
            );
            ui.radio_value(
                &mut *inspector.selection_filter,
                SelectionFilter::Joints,
                "Joints",
            );
        });
        ui.separator();

        if inspector.selection.0.is_empty() {
            ui.label("No selection.");
            return;
        }

        // Inspect the first selected entity for initial values
        let first_entity = *inspector.selection.0.iter().next().unwrap();

        // Check what components the first entity has
        let has_transform;
        let mut local_transform = Transform::default();

        let has_box;
        let mut local_box = EditableBox::default();

        let has_circle;
        let mut local_circle = EditableCircle::default();

        let has_rb;
        let mut local_rb = RigidBody::Dynamic;

        let has_friction;
        let mut local_friction = Friction::default();

        let has_restitution;
        let mut local_restitution = Restitution::default();

        let has_fill;
        let mut local_fill = Fill::color(Color::BLACK);

        let has_stroke;
        let mut local_stroke = Stroke::new(Color::BLACK, DEFAULT_STROKE_WIDTH);

        let _has_sensor;
        let mut local_sensor = false;

        let has_locked_axes;
        let mut local_locked_axes = LockedAxes::empty();

        let has_mass_props;
        let mut local_density = 1.0;

        let has_gravity;
        let mut local_gravity = 1.0;

        {
            let Ok((_, t, rb, f, r, ebox, ecircle, fill, stroke, sensor, locked, mass, grav, _)) =
                inspector.entity_query.get(first_entity)
            else {
                return;
            };

            has_transform = t.is_some();
            if let Some(v) = t {
                local_transform = *v;
            }

            has_box = ebox.is_some();
            if let Some(v) = ebox {
                local_box = *v;
            }

            has_circle = ecircle.is_some();
            if let Some(v) = ecircle {
                local_circle = *v;
            }

            has_rb = rb.is_some();
            if let Some(v) = rb {
                local_rb = *v;
            }

            has_friction = f.is_some();
            if let Some(v) = f {
                local_friction = *v;
            }

            has_restitution = r.is_some();
            if let Some(v) = r {
                local_restitution = *v;
            }

            has_fill = fill.is_some();
            if let Some(v) = fill {
                local_fill = *v;
            }

            has_stroke = stroke.is_some();
            if let Some(v) = stroke {
                local_stroke = *v;
            }

            _has_sensor = sensor.is_some();
            if sensor.is_some() {
                local_sensor = true;
            }

            has_locked_axes = locked.is_some();
            if let Some(v) = locked {
                local_locked_axes = *v;
            }

            has_mass_props = mass.is_some();
            if let Some(ColliderMassProperties::Density(d)) = mass {
                local_density = *d;
            }

            has_gravity = grav.is_some();
            if let Some(v) = grav {
                local_gravity = v.0;
            }
        }

        ui.heading("Inspector");
        ui.label(format!(
            "Selected: {} entities",
            inspector.selection.0.len()
        ));
        ui.separator();

        // Transform
        if has_transform {
            ui.heading("Transform");
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Pos X:");
                if ui
                    .add(egui::DragValue::new(&mut local_transform.translation.x).speed(DRAG_SPEED))
                    .changed()
                {
                    changed = true;
                }
                ui.label("Pos Y:");
                if ui
                    .add(egui::DragValue::new(&mut local_transform.translation.y).speed(DRAG_SPEED))
                    .changed()
                {
                    changed = true;
                }
            });

            let mut rotation = local_transform.rotation.to_euler(EulerRot::XYZ).2;
            let _old_rotation = rotation;
            ui.horizontal(|ui| {
                ui.label("Rotation:");
                if ui.drag_angle(&mut rotation).changed() {
                    local_transform.rotation = Quat::from_rotation_z(rotation);
                    changed = true;
                }
            });

            if changed {
                for &e in &inspector.selection.0 {
                    if let Ok((_, Some(mut t), ..)) = inspector.entity_query.get_mut(e) {
                        // For transform, we might want relative movement, but here we set absolute.
                        // Setting absolute is fine for inspector.
                        t.translation = local_transform.translation;
                        t.rotation = local_transform.rotation;
                        // Wake up body if exists
                        if let Ok((_, _, Some(_), ..)) = inspector.entity_query.get(e) {
                            inspector.commands.entity(e).insert(Sleeping::disabled());
                        }
                    }
                }
            }
            ui.separator();
        }

        // Box
        if has_box {
            ui.heading("Box Dimensions");
            let mut changed = false;
            if ui
                .add(
                    egui::DragValue::new(&mut local_box.width)
                        .speed(DRAG_SPEED)
                        .prefix("Width: "),
                )
                .changed()
            {
                changed = true;
            }
            if ui
                .add(
                    egui::DragValue::new(&mut local_box.height)
                        .speed(DRAG_SPEED)
                        .prefix("Height: "),
                )
                .changed()
            {
                changed = true;
            }

            if changed {
                for &e in &inspector.selection.0 {
                    if let Ok((_, _, _, _, _, Some(mut b), ..)) = inspector.entity_query.get_mut(e)
                    {
                        *b = local_box;
                        // Note: Resizing shape logic might be handled by another system observing changes
                        // or we might need to regenerate collider/shape path here.
                        // Assuming systems handle change detection or user must trigger update.
                        // But typically `EditableBox` change should trigger a system.
                    }
                }
            }
            ui.separator();
        }

        // Circle
        if has_circle {
            ui.heading("Circle Dimensions");
            if ui
                .add(
                    egui::DragValue::new(&mut local_circle.radius)
                        .speed(DRAG_SPEED)
                        .prefix("Radius: "),
                )
                .changed()
            {
                for &e in &inspector.selection.0 {
                    if let Ok((_, _, _, _, _, _, Some(mut c), ..)) =
                        inspector.entity_query.get_mut(e)
                    {
                        *c = local_circle;
                    }
                }
            }
            ui.separator();
        }

        // Physics Body Type
        if has_rb {
            ui.heading("Rigid Body");
            let mut current = local_rb;
            let options = [
                RigidBody::Dynamic,
                RigidBody::Fixed,
                RigidBody::KinematicPositionBased,
            ];
            egui::ComboBox::from_label("Type")
                .selected_text(format!("{:?}", current))
                .show_ui(ui, |ui| {
                    for option in options {
                        if ui
                            .selectable_value(&mut current, option, format!("{:?}", option))
                            .clicked()
                        {
                            for &e in &inspector.selection.0 {
                                inspector
                                    .commands
                                    .entity(e)
                                    .insert(current)
                                    .insert(Sleeping::disabled());
                            }
                        }
                    }
                });
            ui.separator();
        }

        // Sensor
        // We always show Sensor toggle if it has a rigid body or collider
        if has_rb
            || inspector
                .entity_query
                .get(first_entity)
                .map(|c| c.11.is_some())
                .unwrap_or(false)
        {
            // checking collider mass props presence as proxy for collider?
            // Actually sensor is a component on its own.
            let mut is_sensor = local_sensor;
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
        if has_friction || has_rb {
            ui.heading("Friction");
            if ui
                .add(
                    egui::Slider::new(&mut local_friction.coefficient, 0.0..=FRICTION_MAX)
                        .text("Coefficient"),
                )
                .changed()
            {
                for &e in &inspector.selection.0 {
                    if let Ok((_, _, _, f, ..)) = inspector.entity_query.get_mut(e) {
                        if let Some(mut f_comp) = f {
                            f_comp.coefficient = local_friction.coefficient;
                        } else {
                            inspector
                                .commands
                                .entity(e)
                                .insert(Friction::coefficient(local_friction.coefficient));
                        }
                    }
                }
            }
            ui.separator();
        }

        // Restitution
        if has_restitution || has_rb {
            ui.heading("Restitution");
            if ui
                .add(
                    egui::Slider::new(&mut local_restitution.coefficient, 0.0..=RESTITUTION_MAX)
                        .text("Coefficient"),
                )
                .changed()
            {
                for &e in &inspector.selection.0 {
                    if let Ok((_, _, _, _, r, ..)) = inspector.entity_query.get_mut(e) {
                        if let Some(mut r_comp) = r {
                            r_comp.coefficient = local_restitution.coefficient;
                        } else {
                            inspector
                                .commands
                                .entity(e)
                                .insert(Restitution::coefficient(local_restitution.coefficient));
                        }
                    }
                }
            }
            ui.separator();
        }

        // Density
        if has_mass_props || has_rb {
            ui.heading("Density");
            if ui
                .add(
                    egui::DragValue::new(&mut local_density)
                        .speed(DRAG_SPEED)
                        .range(DENSITY_MIN..=DENSITY_MAX),
                )
                .changed()
            {
                for &e in &inspector.selection.0 {
                    inspector
                        .commands
                        .entity(e)
                        .insert(ColliderMassProperties::Density(local_density));
                    inspector.commands.entity(e).insert(Sleeping::disabled());
                }
            }
            ui.separator();
        }

        // Gravity Scale
        if has_gravity || has_rb {
            ui.heading("Gravity Scale");
            if ui
                .add(egui::DragValue::new(&mut local_gravity).speed(DRAG_SPEED))
                .changed()
            {
                for &e in &inspector.selection.0 {
                    inspector
                        .commands
                        .entity(e)
                        .insert(GravityScale(local_gravity));
                    inspector.commands.entity(e).insert(Sleeping::disabled());
                }
            }
            ui.separator();
        }

        // Locked Axes
        if has_locked_axes || has_rb {
            ui.heading("Locked Axes");
            let mut locked = local_locked_axes.contains(LockedAxes::ROTATION_LOCKED);
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
        }

        // Fill Color
        if has_fill {
            ui.heading("Fill Color");
            let mut color_arr = local_fill.color.to_srgba().to_f32_array();
            if ui
                .color_edit_button_rgba_unmultiplied(&mut color_arr)
                .changed()
            {
                let new_color =
                    Color::srgba(color_arr[0], color_arr[1], color_arr[2], color_arr[3]);
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
        if has_stroke {
            ui.heading("Stroke");
            let mut color_arr = local_stroke.color.to_srgba().to_f32_array();
            let mut changed = false;

            ui.horizontal(|ui| {
                if ui
                    .color_edit_button_rgba_unmultiplied(&mut color_arr)
                    .changed()
                {
                    local_stroke.color =
                        Color::srgba(color_arr[0], color_arr[1], color_arr[2], color_arr[3]);
                    changed = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut local_stroke.options.line_width)
                            .speed(DRAG_SPEED)
                            .prefix("Width: "),
                    )
                    .changed()
                {
                    changed = true;
                }
            });

            if changed {
                for &e in &inspector.selection.0 {
                    if let Ok((_, _, _, _, _, _, _, _, Some(mut s), ..)) =
                        inspector.entity_query.get_mut(e)
                    {
                        s.color = local_stroke.color;
                        s.options.line_width = local_stroke.options.line_width;
                    }
                }
            }
            ui.separator();
        }

        // Joint Inspection
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
    });
}
