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
        Option<&RigidBody>, // Avian RigidBody is usually changed by command, but let's see.
        // Avian components:
        Option<&mut Friction>,
        Option<&mut Restitution>,
        // Editable shapes
        Option<&mut EditableBox>,
        Option<&mut EditableCircle>,
        // TODO: Re-enable Fill and Stroke once bevy_prototype_lyon is compatible with Bevy 0.18 Component trait
        // Option<&mut Fill>,
        // Option<&mut Stroke>,
    )>,
    mut commands: Commands,
) {
    let Some(entity) = selection.0 else {
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

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        _ => return,
    };

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
            let options = [RigidBody::Dynamic, RigidBody::Static, RigidBody::Kinematic];
            egui::ComboBox::from_label("Type")
                .selected_text(format!("{:?}", current))
                .show_ui(ui, |ui| {
                    for option in options {
                        if ui
                            .selectable_value(&mut current, option, format!("{:?}", option))
                            .clicked()
                        {
                            // Avian requires removing old and inserting new for RigidBody change usually?
                            // Or just overwriting the component.
                            // Since RigidBody is a component, we can just insert the new one.
                            commands.entity(entity).insert(current);
                        }
                    }
                });
            ui.separator();
        }

        if let Some(ref mut f) = friction {
            ui.heading("Friction");
            ui.add(egui::Slider::new(&mut f.dynamic_coefficient, 0.0..=1.0).text("Dynamic"));
            ui.add(egui::Slider::new(&mut f.static_coefficient, 0.0..=1.0).text("Static"));
            ui.separator();
        }

        if let Some(ref mut r) = restitution {
            ui.heading("Restitution");
            ui.add(egui::Slider::new(&mut r.coefficient, 0.0..=1.0).text("Coefficient"));
            ui.separator();
        }

        // if let Some(ref mut f) = fill {
        //     ui.heading("Fill Color");
        //     // Bevy Color to Egui Color
        //     // Color is an enum in Bevy 0.18 (Srgba, etc.)
        //     // We need to convert back and forth.
        //     // Simplified: assume Srgba or convert to LinearRgba
        //     let mut color_linear = f.color.to_linear();
        //     let mut rgba = [color_linear.red, color_linear.green, color_linear.blue, color_linear.alpha];
        //     if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
        //          f.color = Color::LinearRgba(LinearRgba::from_f32_array(rgba));
        //     }
        //     ui.separator();
        // }

        // if let Some(ref mut s) = stroke {
        //     ui.heading("Stroke");
        //      let mut color_linear = s.color.to_linear();
        //     let mut rgba = [color_linear.red, color_linear.green, color_linear.blue, color_linear.alpha];
        //     if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
        //          s.color = Color::LinearRgba(LinearRgba::from_f32_array(rgba));
        //     }
        //     // Options is private in Stroke?
        //     // Stroke options are checked via options().
        //     // But we can recreate the stroke.
        //     // Stroke::new(color, width)
        //     // But width is not easily accessible if options are private?
        //     // bevy_prototype_lyon Stroke struct has `pub options: StrokeOptions`.
        //     // Wait, I should check docs or source if possible.
        //     // Assuming I can't easily edit width without potentially reconstructing, I'll stick to color.
        //     // Or I can try to access options.
        //     // Let's assume options are public for now.
        //      ui.add(egui::DragValue::new(&mut s.options.line_width).speed(0.1).prefix("Width: "));
        //     ui.separator();
        // }
    });
}
