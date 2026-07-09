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
use crate::domain::shape::ShapeDef;
use crate::interaction::selection::Selection;
use crate::ui::widgets::{Commit, precise_drag};
use avian2d::prelude::*;
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

/// Commits a boolean flag (marker-component presence) across the selection,
/// reading each target's current value via `current`. Markers cannot use
/// [`commit_to_selection`] because an absent marker has no `&C` to read.
fn commit_flag(
    selection: &Selection,
    ids: &Query<&StableId>,
    current: impl Fn(Entity) -> Option<bool>,
    wrap: impl Fn(bool) -> PropertyValue,
    new: bool,
    writer: &mut MessageWriter<PropertyEditIntent>,
) {
    let changes: Vec<PropertyChange> = selection
        .iter()
        .filter_map(|e| {
            let id = ids.get(e).ok().copied()?;
            let old = current(e)?;
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
    body_q: Query<&RigidBody, With<Body>>,
    friction_q: Query<&Friction, With<Body>>,
    restitution_q: Query<&Restitution, With<Body>>,
    density_q: Query<&ColliderDensity, With<Body>>,
    gravity_q: Query<&GravityScale, With<Body>>,
    flags_q: Query<(Has<Sensor>, Has<LockedAxes>), With<Body>>,
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
            // ---- Physics ----
            if let Ok(&rigid_body) = body_q.get(primary) {
                ui.separator();
                ui.label(egui::RichText::new("Physics").strong());

                // Body kind (avian `RigidBody`).
                let mut kind = rigid_body;
                egui::ComboBox::from_label("body")
                    .selected_text(format!("{kind:?}"))
                    .show_ui(ui, |ui| {
                        for option in [RigidBody::Dynamic, RigidBody::Static, RigidBody::Kinematic]
                        {
                            if ui
                                .selectable_value(&mut kind, option, format!("{option:?}"))
                                .clicked()
                            {
                                commit_to_selection(
                                    &selection,
                                    &ids,
                                    &body_q,
                                    PropertyValue::RigidBody,
                                    move |_| option,
                                    &mut writer,
                                );
                            }
                        }
                    });

                // Density.
                let mut density = density_q.get(primary).map_or(1.0, |d| d.0);
                ui.horizontal(|ui| {
                    ui.label("density");
                    if let Commit::Done(_, new) =
                        precise_drag(ui, egui::Id::new(("props", "density")), &mut density, 1.0, 0.01)
                    {
                        commit_to_selection(
                            &selection,
                            &ids,
                            &density_q,
                            PropertyValue::Density,
                            move |_| ColliderDensity(new),
                            &mut writer,
                        );
                    }
                });

                // Friction (a single coefficient drives both static and dynamic).
                let mut friction = friction_q.get(primary).map_or(0.5, |f| f.dynamic_coefficient);
                ui.horizontal(|ui| {
                    ui.label("friction");
                    if let Commit::Done(_, new) = precise_drag(
                        ui,
                        egui::Id::new(("props", "friction")),
                        &mut friction,
                        0.5,
                        0.01,
                    ) {
                        commit_to_selection(
                            &selection,
                            &ids,
                            &friction_q,
                            PropertyValue::Friction,
                            move |old| Friction {
                                dynamic_coefficient: new,
                                static_coefficient: new,
                                ..*old
                            },
                            &mut writer,
                        );
                    }
                });

                // Restitution.
                let mut restitution = restitution_q.get(primary).map_or(0.3, |r| r.coefficient);
                ui.horizontal(|ui| {
                    ui.label("restitution");
                    if let Commit::Done(_, new) = precise_drag(
                        ui,
                        egui::Id::new(("props", "restitution")),
                        &mut restitution,
                        0.3,
                        0.01,
                    ) {
                        commit_to_selection(
                            &selection,
                            &ids,
                            &restitution_q,
                            PropertyValue::Restitution,
                            move |old| Restitution {
                                coefficient: new,
                                ..*old
                            },
                            &mut writer,
                        );
                    }
                });

                // Gravity scale.
                let mut gravity = gravity_q.get(primary).map_or(1.0, |g| g.0);
                ui.horizontal(|ui| {
                    ui.label("gravity scale");
                    if let Commit::Done(_, new) =
                        precise_drag(ui, egui::Id::new(("props", "gravity")), &mut gravity, 1.0, 0.01)
                    {
                        commit_to_selection(
                            &selection,
                            &ids,
                            &gravity_q,
                            PropertyValue::GravityScale,
                            move |_| GravityScale(new),
                            &mut writer,
                        );
                    }
                });

                // Flags (marker-component presence).
                let (sensor_now, locked_now) = flags_q.get(primary).unwrap_or((false, false));
                let mut sensor = sensor_now;
                if ui.checkbox(&mut sensor, "sensor").changed() {
                    commit_flag(
                        &selection,
                        &ids,
                        |e| flags_q.get(e).ok().map(|(s, _)| s),
                        PropertyValue::Sensor,
                        sensor,
                        &mut writer,
                    );
                }
                let mut locked = locked_now;
                if ui.checkbox(&mut locked, "lock rotation").changed() {
                    commit_flag(
                        &selection,
                        &ids,
                        |e| flags_q.get(e).ok().map(|(_, l)| l),
                        PropertyValue::RotationLock,
                        locked,
                        &mut writer,
                    );
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
