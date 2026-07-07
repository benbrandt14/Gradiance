//! The inspector: component copies in, `PropertyEditIntent` out.
//!
//! Every numeric field is a committing precision widget (scientific
//! notation, middle-click default reset, one undo step per gesture).
//! Multi-selection edits apply the committed value to every selected
//! body — each target's own prior value is captured for undo.

use crate::command::intent::PropertyEditIntent;
use crate::command::property::{PropertyChange, PropertyValue};
use crate::core::ids::StableId;
use crate::domain::Body;
use crate::domain::appearance::Appearance;
use crate::domain::layers::LayerMask32;
use crate::domain::props::{BodyKind, PhysicalProps};
use crate::domain::shape::ShapeDef;
use crate::interaction::selection::Selection;
use crate::ui::widgets::{Commit, precise_drag};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Applies `make_new` to every selected body's component, emitting one
/// batched intent (one undo step).
fn commit_to_selection<C: Component + Clone>(
    selection: &Selection,
    ids: &Query<&StableId>,
    components: &Query<&C, With<Body>>,
    wrap: impl Fn(C) -> PropertyValue,
    make_new: impl Fn(&C) -> C,
    writer: &mut MessageWriter<PropertyEditIntent>,
) {
    let changes: Vec<PropertyChange> = selection
        .iter()
        .filter_map(|e| {
            let id = ids.get(e).ok().copied()?;
            let old = components.get(e).ok()?.clone();
            let new = make_new(&old);
            Some(PropertyChange {
                id,
                old: wrap(old),
                new: wrap(new),
            })
        })
        .collect();
    if !changes.is_empty() {
        writer.write(PropertyEditIntent { changes });
    }
}

/// Renders the inspector window for the current selection.
#[expect(clippy::too_many_lines)] // one window, plain sections in order
pub fn inspector_window(
    mut contexts: EguiContexts,
    selection: Res<Selection>,
    ids: Query<&StableId>,
    props_q: Query<&PhysicalProps, With<Body>>,
    shapes_q: Query<&ShapeDef, With<Body>>,
    appearance_q: Query<&Appearance, With<Body>>,
    layers_q: Query<&LayerMask32, With<Body>>,
    mut writer: MessageWriter<PropertyEditIntent>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let Some(primary) = selection.primary() else {
        return Ok(());
    };

    egui::Window::new("Inspector")
        .default_width(240.0)
        .anchor(egui::Align2::RIGHT_TOP, [-8.0, 8.0])
        .show(ctx, |ui| {
            ui.heading(format!("Selection ({})", selection.len()));
            let defaults = PhysicalProps::default();

            // ---- Physics ----
            if let Ok(props) = props_q.get(primary) {
                let mut p = *props;
                ui.separator();
                ui.label(egui::RichText::new("Physics").strong());
                egui::ComboBox::from_label("body")
                    .selected_text(format!("{:?}", p.body))
                    .show_ui(ui, |ui| {
                        for kind in [BodyKind::Dynamic, BodyKind::Static, BodyKind::Kinematic] {
                            if ui
                                .selectable_value(&mut p.body, kind, format!("{kind:?}"))
                                .clicked()
                            {
                                let kind = p.body;
                                commit_to_selection(
                                    &selection,
                                    &ids,
                                    &props_q,
                                    PropertyValue::Props,
                                    |old| PhysicalProps { body: kind, ..*old },
                                    &mut writer,
                                );
                            }
                        }
                    });
                let fields: [(&str, fn(&mut PhysicalProps) -> &mut f32, f32); 4] = [
                    ("density", |p| &mut p.density, defaults.density),
                    ("friction", |p| &mut p.friction, defaults.friction),
                    ("restitution", |p| &mut p.restitution, defaults.restitution),
                    (
                        "gravity scale",
                        |p| &mut p.gravity_scale,
                        defaults.gravity_scale,
                    ),
                ];
                for (label, access, default) in fields {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        if let Commit::Done(_, new) = precise_drag(
                            ui,
                            egui::Id::new(("props", label)),
                            access(&mut p),
                            default,
                            0.01,
                        ) {
                            commit_to_selection(
                                &selection,
                                &ids,
                                &props_q,
                                PropertyValue::Props,
                                |old| {
                                    let mut n = *old;
                                    *access(&mut n) = new;
                                    n
                                },
                                &mut writer,
                            );
                        }
                    });
                }
                for (label, get) in [
                    (
                        "sensor",
                        (|p: &mut PhysicalProps| &mut p.sensor)
                            as fn(&mut PhysicalProps) -> &mut bool,
                    ),
                    ("lock rotation", |p| &mut p.rotation_locked),
                ] {
                    let mut v = *get(&mut p);
                    if ui.checkbox(&mut v, label).changed() {
                        commit_to_selection(
                            &selection,
                            &ids,
                            &props_q,
                            PropertyValue::Props,
                            |old| {
                                let mut n = *old;
                                *get(&mut n) = v;
                                n
                            },
                            &mut writer,
                        );
                    }
                }
            }

            // ---- Shape ----
            if let Ok(shape) = shapes_q.get(primary) {
                ui.separator();
                ui.label(egui::RichText::new("Shape").strong());
                match shape.clone() {
                    ShapeDef::Box {
                        mut width,
                        mut height,
                    } => {
                        ui.horizontal(|ui| {
                            ui.label("w");
                            let cw =
                                precise_drag(ui, egui::Id::new("shape-w"), &mut width, 100.0, 0.5);
                            ui.label("h");
                            let ch =
                                precise_drag(ui, egui::Id::new("shape-h"), &mut height, 100.0, 0.5);
                            if matches!(cw, Commit::Done(..)) || matches!(ch, Commit::Done(..)) {
                                commit_to_selection(
                                    &selection,
                                    &ids,
                                    &shapes_q,
                                    PropertyValue::Shape,
                                    |old| match old {
                                        ShapeDef::Box { .. } => ShapeDef::Box { width, height },
                                        other => other.clone(),
                                    },
                                    &mut writer,
                                );
                            }
                        });
                    }
                    ShapeDef::Circle { mut radius } => {
                        ui.horizontal(|ui| {
                            ui.label("radius");
                            if let Commit::Done(..) =
                                precise_drag(ui, egui::Id::new("shape-r"), &mut radius, 50.0, 0.5)
                            {
                                commit_to_selection(
                                    &selection,
                                    &ids,
                                    &shapes_q,
                                    PropertyValue::Shape,
                                    |old| match old {
                                        ShapeDef::Circle { .. } => ShapeDef::Circle { radius },
                                        other => other.clone(),
                                    },
                                    &mut writer,
                                );
                            }
                        });
                    }
                    ShapeDef::Polygon { outline, .. } => {
                        ui.label(format!("polygon · {} vertices", outline.len()));
                    }
                    ShapeDef::HalfPlane => {
                        ui.label("infinite ground plane");
                    }
                    tree @ (ShapeDef::Csg { .. } | ShapeDef::Placed { .. }) => {
                        ui.label(format!("CSG shape · depth {}", tree.depth()));
                    }
                }
            }

            // ---- Appearance ----
            if let Ok(appearance) = appearance_q.get(primary) {
                ui.separator();
                ui.label(egui::RichText::new("Appearance").strong());
                let to_array = |c: crate::domain::appearance::Rgba| [c.r, c.g, c.b, c.a];
                let to_rgba = |c: [f32; 4]| crate::domain::appearance::Rgba {
                    r: c[0],
                    g: c[1],
                    b: c[2],
                    a: c[3],
                };
                let mut fill = to_array(appearance.fill);
                let mut border = to_array(appearance.border);
                let mut fill_changed = false;
                let mut border_changed = false;
                ui.horizontal(|ui| {
                    ui.label("fill");
                    fill_changed = ui.color_edit_button_rgba_unmultiplied(&mut fill).changed();
                    ui.label("border");
                    border_changed = ui
                        .color_edit_button_rgba_unmultiplied(&mut border)
                        .changed();
                });
                let mut emissive = appearance.emissive;
                let ce = ui
                    .horizontal(|ui| {
                        ui.label("emissive");
                        precise_drag(ui, egui::Id::new("app-emissive"), &mut emissive, 0.0, 0.05)
                    })
                    .inner;
                if fill_changed || border_changed || matches!(ce, Commit::Done(..)) {
                    commit_to_selection(
                        &selection,
                        &ids,
                        &appearance_q,
                        PropertyValue::Appearance,
                        |old| Appearance {
                            fill: to_rgba(fill),
                            border: to_rgba(border),
                            emissive: if matches!(ce, Commit::Done(..)) {
                                emissive.max(0.0)
                            } else {
                                old.emissive
                            },
                        },
                        &mut writer,
                    );
                }
            }

            // ---- Layers ----
            if let Ok(layers) = layers_q.get(primary) {
                ui.separator();
                ui.label(egui::RichText::new("Layers (front → back)").strong());
                ui.horizontal_wrapped(|ui| {
                    for bit in 0..8u32 {
                        let mut on = layers.memberships & (1 << bit) != 0;
                        if ui.checkbox(&mut on, format!("{bit}")).changed() {
                            commit_to_selection(
                                &selection,
                                &ids,
                                &layers_q,
                                PropertyValue::Layers,
                                |old| {
                                    let mut n = *old;
                                    if on {
                                        n.memberships |= 1 << bit;
                                    } else if n.memberships != 1 << bit {
                                        n.memberships &= !(1 << bit);
                                    }
                                    n
                                },
                                &mut writer,
                            );
                        }
                    }
                });
            }
        });
    Ok(())
}
