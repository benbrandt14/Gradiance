//! Body property editing: component copies in, `PropertyEditIntent` out.
//!
//! **Context-menu-first** (feedback 2.8): the property sections below are
//! host-agnostic renderers shared by the right-click context menu (the
//! primary editing surface) and the *Properties* pop-out window, which is
//! closed by default and opened from the menu's "Properties…" command (or
//! the toolbar toggle). One implementation, two hosts — the seam stays a
//! typed intent either way.
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
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Whether the *Properties* pop-out is showing. Closed by default —
/// the context menu is the first-line editing surface (feedback 2.8).
#[derive(Resource, Default, Debug)]
pub struct InspectorPanel {
    /// Show the pop-out window.
    pub open: bool,
}

/// Everything the body-property sections read and write, bundled as one
/// `SystemParam` so both hosts (context menu, pop-out) stay under Bevy's
/// system-parameter limit.
#[derive(SystemParam)]
pub struct BodyProps<'w, 's> {
    /// `StableId` lookup for intent targets (shared with the menu's own
    /// actions).
    pub ids: Query<'w, 's, &'static StableId>,
    /// Layer memberships (also read by the menu's layer/depth editor).
    pub layers: Query<'w, 's, &'static LayerMask32, With<Body>>,
    /// The shared property-edit intent writer.
    pub edits: MessageWriter<'w, PropertyEditIntent>,
    body_q: Query<'w, 's, &'static RigidBody, With<Body>>,
    friction_q: Query<'w, 's, &'static Friction, With<Body>>,
    restitution_q: Query<'w, 's, &'static Restitution, With<Body>>,
    density_q: Query<'w, 's, &'static ColliderDensity, With<Body>>,
    gravity_q: Query<'w, 's, &'static GravityScale, With<Body>>,
    flags_q: Query<'w, 's, (Has<Sensor>, Has<LockedAxes>), With<Body>>,
    shapes_q: Query<'w, 's, &'static ShapeDef, With<Body>>,
    appearance_q: Query<'w, 's, &'static Appearance, With<Body>>,
}

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

/// Physics ("material") properties: body kind, density, friction,
/// restitution, gravity scale, sensor/rotation-lock flags.
#[expect(clippy::too_many_lines)] // plain widgets in order
pub fn physics_section(ui: &mut egui::Ui, selection: &Selection, props: &mut BodyProps) {
    let Some(primary) = selection.primary() else {
        return;
    };
    let Ok(&rigid_body) = props.body_q.get(primary) else {
        return;
    };

    // Body kind (avian `RigidBody`).
    let mut kind = rigid_body;
    egui::ComboBox::from_id_salt(ui.id().with("body-kind"))
        .selected_text(format!("{kind:?}"))
        .show_ui(ui, |ui| {
            for option in [RigidBody::Dynamic, RigidBody::Static, RigidBody::Kinematic] {
                if ui
                    .selectable_value(&mut kind, option, format!("{option:?}"))
                    .clicked()
                {
                    commit_to_selection(
                        selection,
                        &props.ids,
                        &props.body_q,
                        PropertyValue::RigidBody,
                        move |_| option,
                        &mut props.edits,
                    );
                }
            }
        });

    // Density.
    let mut density = props.density_q.get(primary).map_or(1.0, |d| d.0);
    ui.horizontal(|ui| {
        ui.label("density");
        if let Commit::Done(_, new) =
            precise_drag(ui, ui.id().with("density"), &mut density, 1.0, 0.01)
        {
            commit_to_selection(
                selection,
                &props.ids,
                &props.density_q,
                PropertyValue::Density,
                move |_| ColliderDensity(new),
                &mut props.edits,
            );
        }
    });

    // Friction (a single coefficient drives both static and dynamic).
    let mut friction = props
        .friction_q
        .get(primary)
        .map_or(0.5, |f| f.dynamic_coefficient);
    ui.horizontal(|ui| {
        ui.label("friction");
        if let Commit::Done(_, new) =
            precise_drag(ui, ui.id().with("friction"), &mut friction, 0.5, 0.01)
        {
            commit_to_selection(
                selection,
                &props.ids,
                &props.friction_q,
                PropertyValue::Friction,
                move |old| Friction {
                    dynamic_coefficient: new,
                    static_coefficient: new,
                    ..*old
                },
                &mut props.edits,
            );
        }
    });

    // Restitution.
    let mut restitution = props
        .restitution_q
        .get(primary)
        .map_or(0.3, |r| r.coefficient);
    ui.horizontal(|ui| {
        ui.label("restitution");
        if let Commit::Done(_, new) =
            precise_drag(ui, ui.id().with("restitution"), &mut restitution, 0.3, 0.01)
        {
            commit_to_selection(
                selection,
                &props.ids,
                &props.restitution_q,
                PropertyValue::Restitution,
                move |old| Restitution {
                    coefficient: new,
                    ..*old
                },
                &mut props.edits,
            );
        }
    });

    // Gravity scale.
    let mut gravity = props.gravity_q.get(primary).map_or(1.0, |g| g.0);
    ui.horizontal(|ui| {
        ui.label("gravity scale");
        if let Commit::Done(_, new) =
            precise_drag(ui, ui.id().with("gravity"), &mut gravity, 1.0, 0.01)
        {
            commit_to_selection(
                selection,
                &props.ids,
                &props.gravity_q,
                PropertyValue::GravityScale,
                move |_| GravityScale(new),
                &mut props.edits,
            );
        }
    });

    // Flags (marker-component presence).
    let (sensor_now, locked_now) = props.flags_q.get(primary).unwrap_or((false, false));
    let mut sensor = sensor_now;
    if ui.checkbox(&mut sensor, "sensor").changed() {
        commit_flag(
            selection,
            &props.ids,
            |e| props.flags_q.get(e).ok().map(|(s, _)| s),
            PropertyValue::Sensor,
            sensor,
            &mut props.edits,
        );
    }
    let mut locked = locked_now;
    if ui.checkbox(&mut locked, "lock rotation").changed() {
        commit_flag(
            selection,
            &props.ids,
            |e| props.flags_q.get(e).ok().map(|(_, l)| l),
            PropertyValue::RotationLock,
            locked,
            &mut props.edits,
        );
    }
}

/// Shape parameters (box size, circle radius; read-only summaries for
/// polygons, ground, and CSG trees).
pub fn shape_section(ui: &mut egui::Ui, selection: &Selection, props: &mut BodyProps) {
    let Some(primary) = selection.primary() else {
        return;
    };
    let Ok(shape) = props.shapes_q.get(primary) else {
        return;
    };
    match shape.clone() {
        ShapeDef::Box {
            mut width,
            mut height,
        } => {
            ui.horizontal(|ui| {
                ui.label("w");
                let cw = precise_drag(ui, ui.id().with("shape-w"), &mut width, 100.0, 0.5);
                ui.label("h");
                let ch = precise_drag(ui, ui.id().with("shape-h"), &mut height, 100.0, 0.5);
                if matches!(cw, Commit::Done(..)) || matches!(ch, Commit::Done(..)) {
                    commit_to_selection(
                        selection,
                        &props.ids,
                        &props.shapes_q,
                        PropertyValue::Shape,
                        |old| match old {
                            ShapeDef::Box { .. } => ShapeDef::Box { width, height },
                            other => other.clone(),
                        },
                        &mut props.edits,
                    );
                }
            });
        }
        ShapeDef::Circle { mut radius } => {
            ui.horizontal(|ui| {
                ui.label("radius");
                if let Commit::Done(..) =
                    precise_drag(ui, ui.id().with("shape-r"), &mut radius, 50.0, 0.5)
                {
                    commit_to_selection(
                        selection,
                        &props.ids,
                        &props.shapes_q,
                        PropertyValue::Shape,
                        |old| match old {
                            ShapeDef::Circle { .. } => ShapeDef::Circle { radius },
                            other => other.clone(),
                        },
                        &mut props.edits,
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

/// Appearance: fill/border colors and emissive strength.
pub fn appearance_section(ui: &mut egui::Ui, selection: &Selection, props: &mut BodyProps) {
    let Some(primary) = selection.primary() else {
        return;
    };
    let Ok(appearance) = props.appearance_q.get(primary) else {
        return;
    };
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
            precise_drag(ui, ui.id().with("emissive"), &mut emissive, 0.0, 0.05)
        })
        .inner;
    if fill_changed || border_changed || matches!(ce, Commit::Done(..)) {
        commit_to_selection(
            selection,
            &props.ids,
            &props.appearance_q,
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
            &mut props.edits,
        );
    }
}

/// Layer memberships as front→back checkboxes.
pub fn layers_section(ui: &mut egui::Ui, selection: &Selection, props: &mut BodyProps) {
    let Some(primary) = selection.primary() else {
        return;
    };
    let Ok(layers) = props.layers.get(primary) else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        for bit in 0..8u32 {
            let mut on = layers.memberships & (1 << bit) != 0;
            if ui.checkbox(&mut on, format!("{bit}")).changed() {
                commit_to_selection(
                    selection,
                    &props.ids,
                    &props.layers,
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
                    &mut props.edits,
                );
            }
        }
    });
}

/// Renders the *Properties* pop-out for the current selection (opened from
/// the context menu or the toolbar; closed by default).
pub fn inspector_window(
    mut contexts: EguiContexts,
    mut panel: ResMut<InspectorPanel>,
    selection: Res<Selection>,
    mut props: BodyProps,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !panel.open || selection.is_empty() {
        return Ok(());
    }
    let mut open = panel.open;
    egui::Window::new("Properties")
        .default_width(240.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.heading(format!("Selection ({})", selection.len()));
            ui.separator();
            ui.label(egui::RichText::new("Physics").strong());
            physics_section(ui, &selection, &mut props);
            ui.separator();
            ui.label(egui::RichText::new("Shape").strong());
            shape_section(ui, &selection, &mut props);
            ui.separator();
            ui.label(egui::RichText::new("Appearance").strong());
            appearance_section(ui, &selection, &mut props);
            ui.separator();
            ui.label(egui::RichText::new("Layers (front → back)").strong());
            layers_section(ui, &selection, &mut props);
        });
    panel.open = open;
    Ok(())
}
