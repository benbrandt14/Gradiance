//! Inspector panel for editing entity properties.
//!
//! Provides an Egui sidebar that allows modifying properties of selected entities,
//! such as Transform, RigidBody type, Friction, and Restitution.

use crate::input::editable::{EditableBox, EditableCircle};
use crate::input::events::{
    ModifyAttractionEvent, ModifyPhysicsEvent, ModifyRenderEvent, ModifyShapeEvent,
    ModifyTransformEvent,
};
use crate::input::selection::Selection;
use crate::physics::attraction::Attractor;
use crate::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_prototype_lyon::prelude::*;

/// Plugin for the Inspector UI.
pub struct InspectorPlugin;

impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, inspector_ui);
    }
}

#[allow(clippy::type_complexity)]
fn inspector_ui(
    mut contexts: EguiContexts,
    selection: Res<Selection>,
    query: Query<(
        Entity,
        Option<&Transform>,
        Option<&RigidBody>,
        Option<&Friction>,
        Option<&Restitution>,
        Option<&EditableBox>,
        Option<&EditableCircle>,
        Option<&Fill>,
        Option<&Stroke>,
        Option<&Sensor>,
        Option<&LockedAxes>,
        Option<&ColliderMassProperties>,
        Option<&GravityScale>,
        Option<&Attractor>,
    )>,
    mut ev_transform: EventWriter<ModifyTransformEvent>,
    mut ev_physics: EventWriter<ModifyPhysicsEvent>,
    mut ev_shape: EventWriter<ModifyShapeEvent>,
    mut ev_render: EventWriter<ModifyRenderEvent>,
    mut ev_attraction: EventWriter<ModifyAttractionEvent>,
) {
    if selection.0.is_empty() {
        return;
    }

    // Inspect the first selected entity for initial values
    let first_entity = *selection.0.iter().next().unwrap();

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
    let mut local_fill = Fill::color(Color::srgb(0.0, 0.0, 0.0));

    let has_stroke;
    let mut local_stroke = Stroke::new(Color::srgb(0.0, 0.0, 0.0), 1.0);

    let _has_sensor;
    let mut local_sensor = false;

    let has_locked_axes;
    let mut local_locked_axes = LockedAxes::empty();

    let has_mass_props;
    let mut local_density = 1.0;

    let has_gravity;
    let mut local_gravity = 1.0;

    let has_attractor;
    let mut local_attractor = Attractor::default();

    {
        let Ok((
            _,
            t,
            rb,
            f,
            r,
            ebox,
            ecircle,
            fill,
            stroke,
            sensor,
            locked,
            mass,
            grav,
            att,
        )) = query.get(first_entity) else {
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
            local_fill = v.clone();
        }

        has_stroke = stroke.is_some();
        if let Some(v) = stroke {
            local_stroke = v.clone();
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

        has_attractor = att.is_some();
        if let Some(v) = att {
            local_attractor = *v;
        }
    }

    let ctx = contexts.ctx_mut();

    egui::SidePanel::right("inspector_panel").show(ctx, |ui| {
        ui.heading("Inspector");
        ui.label(format!("Selected: {} entities", selection.0.len()));
        ui.separator();

        // Transform
        if has_transform {
            ui.heading("Transform");
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Pos X:");
                if ui
                    .add(egui::DragValue::new(&mut local_transform.translation.x).speed(0.1))
                    .changed()
                {
                    changed = true;
                }
                ui.label("Pos Y:");
                if ui
                    .add(egui::DragValue::new(&mut local_transform.translation.y).speed(0.1))
                    .changed()
                {
                    changed = true;
                }
            });

            let mut rotation = local_transform.rotation.to_euler(EulerRot::XYZ).2;
            ui.horizontal(|ui| {
                ui.label("Rotation:");
                if ui.drag_angle(&mut rotation).changed() {
                    local_transform.rotation = Quat::from_rotation_z(rotation);
                    changed = true;
                }
            });

            if changed {
                for &e in &selection.0 {
                    ev_transform.send(ModifyTransformEvent {
                        entity: e,
                        translation: Some(local_transform.translation),
                        rotation: Some(local_transform.rotation),
                    });
                }
            }
            ui.separator();
        }

        // Attraction
        if has_attractor || has_rb {
            ui.heading("Attraction");
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Strength:");
                if ui
                    .add(egui::DragValue::new(&mut local_attractor.strength).speed(10.0))
                    .changed()
                {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Range:");
                if ui
                    .add(egui::DragValue::new(&mut local_attractor.range).speed(10.0))
                    .changed()
                {
                    changed = true;
                }
            });

            if changed {
                for &e in &selection.0 {
                    ev_attraction.send(ModifyAttractionEvent {
                        entity: e,
                        strength: Some(local_attractor.strength),
                        range: Some(local_attractor.range),
                    });
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
                        .speed(0.1)
                        .prefix("Width: "),
                )
                .changed()
            {
                changed = true;
            }
            if ui
                .add(
                    egui::DragValue::new(&mut local_box.height)
                        .speed(0.1)
                        .prefix("Height: "),
                )
                .changed()
            {
                changed = true;
            }

            if changed {
                for &e in &selection.0 {
                    ev_shape.send(ModifyShapeEvent {
                        entity: e,
                        box_size: Some(Vec2::new(local_box.width as f32, local_box.height as f32)),
                        circle_radius: None,
                    });
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
                        .speed(0.1)
                        .prefix("Radius: "),
                )
                .changed()
            {
                for &e in &selection.0 {
                    ev_shape.send(ModifyShapeEvent {
                        entity: e,
                        box_size: None,
                        circle_radius: Some(local_circle.radius as f32),
                    });
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
                            for &e in &selection.0 {
                                ev_physics.send(ModifyPhysicsEvent {
                                    entity: e,
                                    body_type: Some(current),
                                    friction: None,
                                    restitution: None,
                                    density: None,
                                    gravity_scale: None,
                                    locked_axes: None,
                                    is_sensor: None,
                                });
                            }
                        }
                    }
                });
            ui.separator();
        }

        // Sensor
        if has_rb || query.get(first_entity).map(|c| c.11.is_some()).unwrap_or(false) {
            let mut is_sensor = local_sensor;
            if ui.checkbox(&mut is_sensor, "Sensor").clicked() {
                for &e in &selection.0 {
                    ev_physics.send(ModifyPhysicsEvent {
                        entity: e,
                        body_type: None,
                        friction: None,
                        restitution: None,
                        density: None,
                        gravity_scale: None,
                        locked_axes: None,
                        is_sensor: Some(is_sensor),
                    });
                }
            }
        }

        // Friction
        if has_friction || has_rb {
            ui.heading("Friction");
            if ui
                .add(
                    egui::Slider::new(&mut local_friction.coefficient, 0.0..=2.0)
                        .text("Coefficient"),
                )
                .changed()
            {
                for &e in &selection.0 {
                    ev_physics.send(ModifyPhysicsEvent {
                        entity: e,
                        body_type: None,
                        friction: Some(local_friction.coefficient),
                        restitution: None,
                        density: None,
                        gravity_scale: None,
                        locked_axes: None,
                        is_sensor: None,
                    });
                }
            }
            ui.separator();
        }

        // Restitution
        if has_restitution || has_rb {
            ui.heading("Restitution");
            if ui
                .add(
                    egui::Slider::new(&mut local_restitution.coefficient, 0.0..=1.0)
                        .text("Coefficient"),
                )
                .changed()
            {
                for &e in &selection.0 {
                    ev_physics.send(ModifyPhysicsEvent {
                        entity: e,
                        body_type: None,
                        friction: None,
                        restitution: Some(local_restitution.coefficient),
                        density: None,
                        gravity_scale: None,
                        locked_axes: None,
                        is_sensor: None,
                    });
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
                        .speed(0.1)
                        .range(0.001..=1000.0),
                )
                .changed()
            {
                for &e in &selection.0 {
                    ev_physics.send(ModifyPhysicsEvent {
                        entity: e,
                        body_type: None,
                        friction: None,
                        restitution: None,
                        density: Some(local_density),
                        gravity_scale: None,
                        locked_axes: None,
                        is_sensor: None,
                    });
                }
            }
            ui.separator();
        }

        // Gravity Scale
        if has_gravity || has_rb {
            ui.heading("Gravity Scale");
            if ui
                .add(egui::DragValue::new(&mut local_gravity).speed(0.1))
                .changed()
            {
                for &e in &selection.0 {
                    ev_physics.send(ModifyPhysicsEvent {
                        entity: e,
                        body_type: None,
                        friction: None,
                        restitution: None,
                        density: None,
                        gravity_scale: Some(local_gravity),
                        locked_axes: None,
                        is_sensor: None,
                    });
                }
            }
            ui.separator();
        }

        // Locked Axes
        if has_locked_axes || has_rb {
            ui.heading("Locked Axes");
            let mut locked = local_locked_axes.contains(LockedAxes::ROTATION_LOCKED);
            if ui.checkbox(&mut locked, "Lock Rotation").clicked() {
                for &e in &selection.0 {
                    let new_locked = if locked {
                        LockedAxes::ROTATION_LOCKED
                    } else {
                        LockedAxes::empty()
                    };
                    ev_physics.send(ModifyPhysicsEvent {
                        entity: e,
                        body_type: None,
                        friction: None,
                        restitution: None,
                        density: None,
                        gravity_scale: None,
                        locked_axes: Some(new_locked),
                        is_sensor: None,
                    });
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
                for &e in &selection.0 {
                    ev_render.send(ModifyRenderEvent {
                        entity: e,
                        fill_color: Some(new_color),
                        stroke_color: None,
                        stroke_width: None,
                    });
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
                            .speed(0.1)
                            .prefix("Width: "),
                    )
                    .changed()
                {
                    changed = true;
                }
            });

            if changed {
                for &e in &selection.0 {
                    ev_render.send(ModifyRenderEvent {
                        entity: e,
                        fill_color: None,
                        stroke_color: Some(local_stroke.color),
                        stroke_width: Some(local_stroke.options.line_width),
                    });
                }
            }
            ui.separator();
        }
    });
}
