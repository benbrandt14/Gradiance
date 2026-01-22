//! Inspector panel for editing entity properties.
//!
//! Provides an Egui sidebar that allows modifying properties of selected entities,
//! such as Transform, RigidBody type, Friction, and Restitution.

use crate::input::editable::{EditableBox, EditableCircle};
use crate::input::selection::Selection;
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
    mut query: Query<(
        Entity,
        Option<&mut Transform>,
        Option<&mut RigidBody>,
        Option<&mut Friction>,
        Option<&mut Restitution>,
        Option<&mut EditableBox>,
        Option<&mut EditableCircle>,
        Option<&mut Fill>,
        Option<&mut Stroke>,
        Option<&mut Sensor>,
        Option<&mut LockedAxes>,
        Option<&mut ColliderMassProperties>,
        Option<&mut GravityScale>,
        Option<&mut Sleeping>,
    )>,
    mut commands: Commands,
) {
    if selection.0.is_empty() {
        return;
    }

    // Inspect the first selected entity for initial values
    let first_entity = *selection.0.iter().next().unwrap();

    // We need to extract values. We can't hold the borrow while doing UI if we want to write later.
    // So we verify existence and copy values.

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
    let mut local_density = 1.0; // We usually edit density via ColliderMassProperties::Density

    let has_gravity;
    let mut local_gravity = 1.0;

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
            _
        )) = query.get(first_entity) else {
            return;
        };

        has_transform = t.is_some();
        if let Some(v) = t { local_transform = *v; }

        has_box = ebox.is_some();
        if let Some(v) = ebox { local_box = *v; }

        has_circle = ecircle.is_some();
        if let Some(v) = ecircle { local_circle = *v; }

        has_rb = rb.is_some();
        if let Some(v) = rb { local_rb = *v; }

        has_friction = f.is_some();
        if let Some(v) = f { local_friction = *v; }

        has_restitution = r.is_some();
        if let Some(v) = r { local_restitution = *v; }

        has_fill = fill.is_some();
        if let Some(v) = fill { local_fill = v.clone(); }

        has_stroke = stroke.is_some();
        if let Some(v) = stroke { local_stroke = v.clone(); }

        _has_sensor = sensor.is_some();
        if sensor.is_some() { local_sensor = true; }

        has_locked_axes = locked.is_some();
        if let Some(v) = locked { local_locked_axes = *v; }

        has_mass_props = mass.is_some();
        if let Some(ColliderMassProperties::Density(d)) = mass {
            local_density = *d;
        }

        has_gravity = grav.is_some();
        if let Some(v) = grav { local_gravity = v.0; }
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
                if ui.add(egui::DragValue::new(&mut local_transform.translation.x).speed(0.1)).changed() { changed = true; }
                ui.label("Pos Y:");
                if ui.add(egui::DragValue::new(&mut local_transform.translation.y).speed(0.1)).changed() { changed = true; }
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
                for &e in &selection.0 {
                    if let Ok((_, Some(mut t), ..)) = query.get_mut(e) {
                         // For transform, we might want relative movement, but here we set absolute.
                         // Setting absolute is fine for inspector.
                         t.translation = local_transform.translation;
                         t.rotation = local_transform.rotation;
                         // Wake up body if exists
                         if let Ok((_, _, Some(_), ..)) = query.get(e) {
                             commands.entity(e).insert(Sleeping::disabled());
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
            if ui.add(egui::DragValue::new(&mut local_box.width).speed(0.1).prefix("Width: ")).changed() { changed = true; }
            if ui.add(egui::DragValue::new(&mut local_box.height).speed(0.1).prefix("Height: ")).changed() { changed = true; }

            if changed {
                for &e in &selection.0 {
                    if let Ok((_, _, _, _, _, Some(mut b), ..)) = query.get_mut(e) {
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
            if ui.add(egui::DragValue::new(&mut local_circle.radius).speed(0.1).prefix("Radius: ")).changed() {
                 for &e in &selection.0 {
                    if let Ok((_, _, _, _, _, _, Some(mut c), ..)) = query.get_mut(e) {
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
                        if ui.selectable_value(&mut current, option, format!("{:?}", option)).clicked() {
                            for &e in &selection.0 {
                                commands.entity(e).insert(current).insert(Sleeping::disabled());
                            }
                        }
                    }
                });
            ui.separator();
        }

        // Sensor
        // We always show Sensor toggle if it has a rigid body or collider
        if has_rb || query.get(first_entity).map(|c| c.11.is_some()).unwrap_or(false) { // checking collider mass props presence as proxy for collider?
             // Actually sensor is a component on its own.
             let mut is_sensor = local_sensor;
             if ui.checkbox(&mut is_sensor, "Sensor").clicked() {
                 for &e in &selection.0 {
                     if is_sensor {
                         commands.entity(e).insert(Sensor);
                     } else {
                         commands.entity(e).remove::<Sensor>();
                     }
                 }
             }
        }

        // Friction
        if has_friction || has_rb {
            ui.heading("Friction");
            if ui.add(egui::Slider::new(&mut local_friction.coefficient, 0.0..=2.0).text("Coefficient")).changed() {
                for &e in &selection.0 {
                    if let Ok((_, _, _, f, ..)) = query.get_mut(e) {
                        if let Some(mut f_comp) = f {
                             f_comp.coefficient = local_friction.coefficient;
                        } else {
                             commands.entity(e).insert(Friction::coefficient(local_friction.coefficient));
                        }
                    }
                }
            }
            ui.separator();
        }

        // Restitution
        if has_restitution || has_rb {
            ui.heading("Restitution");
            if ui.add(egui::Slider::new(&mut local_restitution.coefficient, 0.0..=1.0).text("Coefficient")).changed() {
                for &e in &selection.0 {
                    if let Ok((_, _, _, _, r, ..)) = query.get_mut(e) {
                        if let Some(mut r_comp) = r {
                            r_comp.coefficient = local_restitution.coefficient;
                        } else {
                            commands.entity(e).insert(Restitution::coefficient(local_restitution.coefficient));
                        }
                    }
                }
            }
            ui.separator();
        }

        // Density
        if has_mass_props || has_rb {
            ui.heading("Density");
             if ui.add(egui::DragValue::new(&mut local_density).speed(0.1).range(0.001..=1000.0)).changed() {
                for &e in &selection.0 {
                    commands.entity(e).insert(ColliderMassProperties::Density(local_density));
                    commands.entity(e).insert(Sleeping::disabled());
                }
            }
            ui.separator();
        }

        // Gravity Scale
        if has_gravity || has_rb {
             ui.heading("Gravity Scale");
             if ui.add(egui::DragValue::new(&mut local_gravity).speed(0.1)).changed() {
                for &e in &selection.0 {
                    commands.entity(e).insert(GravityScale(local_gravity));
                    commands.entity(e).insert(Sleeping::disabled());
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
                     if locked {
                         commands.entity(e).insert(LockedAxes::ROTATION_LOCKED);
                     } else {
                         commands.entity(e).insert(LockedAxes::empty());
                     }
                     commands.entity(e).insert(Sleeping::disabled());
                 }
             }
             ui.separator();
        }

        // Fill Color
        if has_fill {
            ui.heading("Fill Color");
            let mut color_arr = local_fill.color.to_srgba().to_f32_array();
            if ui.color_edit_button_rgba_unmultiplied(&mut color_arr).changed() {
                let new_color = Color::srgba(color_arr[0], color_arr[1], color_arr[2], color_arr[3]);
                for &e in &selection.0 {
                    if let Ok((_, _, _, _, _, _, _, Some(mut f), ..)) = query.get_mut(e) {
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
                if ui.color_edit_button_rgba_unmultiplied(&mut color_arr).changed() {
                    local_stroke.color = Color::srgba(color_arr[0], color_arr[1], color_arr[2], color_arr[3]);
                    changed = true;
                }
                if ui.add(egui::DragValue::new(&mut local_stroke.options.line_width).speed(0.1).prefix("Width: ")).changed() {
                    changed = true;
                }
            });

            if changed {
                for &e in &selection.0 {
                    if let Ok((_, _, _, _, _, _, _, _, Some(mut s), ..)) = query.get_mut(e) {
                        s.color = local_stroke.color;
                        s.options.line_width = local_stroke.options.line_width;
                    }
                }
            }
            ui.separator();
        }
    });
}
