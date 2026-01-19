//! Inspector panel for editing entity properties.
//!
//! Provides an Egui sidebar that allows modifying properties of selected entities,
//! such as Transform, RigidBody type, Friction, and Restitution.

use crate::input::editable::{EditableBox, EditableCircle};
use crate::input::selection::Selection;
use crate::prelude::*;
use bevy_egui::{EguiContexts, egui};
// use bevy_prototype_lyon::prelude::*;

/// Plugin for the Inspector UI.
pub struct InspectorPlugin;

impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, inspector_ui);
    }
}

fn inspector_ui(
    mut contexts: EguiContexts,
    selection: Res<Selection>,
    mut query: Query<(
        Option<&mut Transform>,
        Option<&RigidBody>,
        Option<&mut Friction>,
        Option<&mut Restitution>,
        Option<&mut EditableBox>,
        Option<&mut EditableCircle>,
        // Option<&mut Fill>,
        // Option<&mut Stroke>,
    )>,
    mut commands: Commands,
) {
    // Inspect the first selected entity, or return if none.
    let Some(&entity) = selection.0.iter().next() else {
        return;
    };

    let Ok((
        mut transform,
        rigid_body,
        mut friction,
        mut restitution,
        mut editable_box,
        mut editable_circle,
        // mut fill,
        // mut stroke
    )) = query.get_mut(entity)
    else {
        return;
    };

    let ctx = contexts.ctx_mut();

    egui::SidePanel::right("inspector_panel").show(ctx, |ui| {
        ui.heading("Inspector");
        ui.separator();

        if let Some(ref mut t) = transform {
            ui.heading("Transform");
            ui.horizontal(|ui| {
                ui.label("Pos X:");
                ui.add(egui::DragValue::new(&mut t.translation.x).speed(0.1));
                ui.label("Pos Y:");
                ui.add(egui::DragValue::new(&mut t.translation.y).speed(0.1));
            });
            // Rotation z
            let mut rotation = t.rotation.to_euler(EulerRot::XYZ).2;
            let old_rotation = rotation;
            ui.horizontal(|ui| {
                ui.label("Rotation:");
                ui.drag_angle(&mut rotation);
            });
            if (rotation - old_rotation).abs() > 0.0001 {
                t.rotation = Quat::from_rotation_z(rotation);
            }
            ui.separator();
        }

        if let Some(ref mut box_shape) = editable_box {
            ui.heading("Box Dimensions");
            // box_shape is EditableBox { width: f64, height: f64 }
            // Egui DragValue works with f64 or f32.
            // If EditableBox uses f64, fine.
            ui.add(
                egui::DragValue::new(&mut box_shape.width)
                    .speed(0.1)
                    .prefix("Width: "),
            );
            ui.add(
                egui::DragValue::new(&mut box_shape.height)
                    .speed(0.1)
                    .prefix("Height: "),
            );
            ui.separator();
        }

        if let Some(ref mut circle_shape) = editable_circle {
            ui.heading("Circle Dimensions");
            ui.add(
                egui::DragValue::new(&mut circle_shape.radius)
                    .speed(0.1)
                    .prefix("Radius: "),
            );
            ui.separator();
        }

        if let Some(rb) = rigid_body {
            ui.heading("Rigid Body");
            let mut current = *rb;
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
                            commands.entity(entity).insert(current);
                        }
                    }
                });
            ui.separator();
        }

        if let Some(ref mut f) = friction {
            ui.heading("Friction");
            ui.add(egui::Slider::new(&mut f.coefficient, 0.0..=1.0).text("Coefficient"));
            ui.separator();
        }

        if let Some(ref mut r) = restitution {
            ui.heading("Restitution");
            ui.add(egui::Slider::new(&mut r.coefficient, 0.0..=1.0).text("Coefficient"));
            ui.separator();
        }

        /*
        if let Some(ref mut f) = fill {
            ui.heading("Fill Color");
            // Color is usually Srgba in 0.15.
            if let Color::Srgba(c) = &mut f.color {
                let mut rgba = [c.red, c.green, c.blue, c.alpha];
                if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                     f.color = Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]);
                }
            } else {
                 // Fallback or conversion
                 ui.label("Complex Color (TODO)");
            }
            ui.separator();
        }

        if let Some(ref mut s) = stroke {
            ui.heading("Stroke");
             if let Color::Srgba(c) = &mut s.color {
                let mut rgba = [c.red, c.green, c.blue, c.alpha];
                if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                     s.color = Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]);
                }
            }
             ui.add(egui::DragValue::new(&mut s.options.line_width).speed(0.1).prefix("Width: "));
            ui.separator();
        }
        */
    });
}
