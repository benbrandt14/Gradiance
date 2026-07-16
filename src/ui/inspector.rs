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
use crate::domain::depth::DepthBand;
use crate::domain::shape::ShapeDef;
use crate::interaction::selection::Selection;
use crate::ui::ports;
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
    /// Depth bands (also read by the menu's depth actions and the dock).
    pub depths: Query<'w, 's, &'static DepthBand, With<Body>>,
    /// The shared property-edit intent writer.
    pub edits: MessageWriter<'w, PropertyEditIntent>,
    /// Debug-overlay settings (config seam: the layers UI toggles the
    /// viewport layer visualization).
    debug: ResMut<'w, crate::domain::settings::DebugSettings>,
    body_q: Query<'w, 's, &'static RigidBody, With<Body>>,
    field_q: Query<'w, 's, &'static crate::domain::field::FieldSource, With<Body>>,
    tracer_q: Query<'w, 's, &'static crate::domain::tracer::Tracer, With<Body>>,
    friction_q: Query<'w, 's, &'static Friction, With<Body>>,
    restitution_q: Query<'w, 's, &'static Restitution, With<Body>>,
    density_q: Query<'w, 's, &'static ColliderDensity, With<Body>>,
    gravity_q: Query<'w, 's, &'static GravityScale, With<Body>>,
    flags_q: Query<'w, 's, (Has<Sensor>, Has<LockedAxes>), With<Body>>,
    shapes_q: Query<'w, 's, &'static ShapeDef, With<Body>>,
    appearance_q: Query<'w, 's, &'static Appearance, With<Body>>,
    /// Live physics reads for the sensor-port readouts (read-only facade).
    physics: crate::physics::queries::PhysicsQueries<'w, 's>,
    /// `StableId` → `Entity` for resolving sensor-port sources.
    id_index: Res<'w, crate::core::ids::IdIndex>,
    /// Body poses for the position/height sensor ports.
    body_transforms: Query<'w, 's, &'static Transform, With<Body>>,
    /// Fixed timestep, for the impulse→force sensor readout.
    fixed: Res<'w, Time<Fixed>>,
    /// Behavior-node kinds, so the pop-out edits a selected node instead of
    /// rendering dead body-section headers (the freeze fix for nodes).
    node_kinds: Query<
        'w,
        's,
        &'static crate::domain::node::NodeKind,
        With<crate::domain::node::BehaviorNode>,
    >,
}

impl BodyProps<'_, '_> {
    /// Whether `entity` is an inspectable rigid body (drives which sections
    /// render, so a non-body selection never shows dead headers).
    pub fn is_body(&self, entity: Entity) -> bool {
        self.body_q.get(entity).is_ok()
    }

    /// The fixed-step seconds used to scale the contact-force readout.
    pub fn dt(&self) -> f32 {
        self.fixed.timestep().as_secs_f32().max(1e-6)
    }

    /// Renders `entity`'s live sensor-port readouts (shared by the pop-out and
    /// the context menu). Encapsulates the read facade so hosts don't touch
    /// the private query fields. No-op for an entity without a `StableId`.
    pub fn sensors(&self, ui: &mut egui::Ui, entity: Entity) {
        if let Ok(&id) = self.ids.get(entity) {
            ports::sensor_readouts(
                ui,
                id,
                &self.id_index,
                &self.physics,
                &self.body_transforms,
                self.dt(),
            );
        }
    }
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
    field_block(ui, selection, props, primary);
    tracer_block(ui, selection, props, primary);
}

/// The tracer editor: enable toggle + fade window. Commits
/// `PropertyValue::Tracer` through the shared intent seam.
fn tracer_block(ui: &mut egui::Ui, selection: &Selection, props: &mut BodyProps, primary: Entity) {
    use crate::domain::tracer::Tracer;
    let current = props.tracer_q.get(primary).ok().copied();
    let mut on = current.is_some();
    if ui
        .checkbox(&mut on, "trace trajectory")
        .on_hover_text("draw the body's recent path as a fading trail")
        .changed()
    {
        let new = on.then(|| current.unwrap_or_default());
        commit_tracers(selection, props, new);
    }
    let Some(tracer) = current else {
        return;
    };
    let mut fade = tracer.fade_secs;
    ui.horizontal(|ui| {
        ui.label("fade (s)");
        if let Commit::Done(_, new) =
            precise_drag(ui, ui.id().with("tracer-fade"), &mut fade, 3.0, 0.05)
        {
            commit_tracers(
                selection,
                props,
                Some(Tracer {
                    fade_secs: new.max(0.05),
                    ..tracer
                }),
            );
        }
    });
}

/// Commits one batched tracer edit across the selection (each target's own
/// prior value captured for undo).
fn commit_tracers(
    selection: &Selection,
    props: &mut BodyProps,
    new: Option<crate::domain::tracer::Tracer>,
) {
    let changes: Vec<PropertyChange> = selection
        .iter()
        .filter_map(|e| {
            let id = props.ids.get(e).ok().copied()?;
            let old = props.tracer_q.get(e).ok().copied();
            Some(PropertyChange {
                id,
                old: PropertyValue::Tracer(old),
                new: PropertyValue::Tracer(new),
            })
        })
        .collect();
    if !changes.is_empty() {
        props.edits.write(PropertyEditIntent { changes });
    }
}

/// The field editor (Algodoo's attraction): enable toggle, signed
/// repulsion strength (negative attracts), and linear/quadratic falloff.
/// Commits `PropertyValue::Field` through the shared intent seam.
fn field_block(ui: &mut egui::Ui, selection: &Selection, props: &mut BodyProps, primary: Entity) {
    use crate::domain::field::{FieldFalloff, FieldSource};
    let current = props.field_q.get(primary).ok().copied();
    let mut on = current.is_some();
    if ui
        .checkbox(&mut on, "field (attraction)")
        .on_hover_text("SDF force field: negative strength attracts, positive repels")
        .changed()
    {
        let new = on.then(|| current.unwrap_or_default());
        commit_fields(selection, props, new);
    }
    let Some(field) = current else {
        return;
    };
    let mut strength = field.strength;
    ui.horizontal(|ui| {
        ui.label("repulsion")
            .on_hover_text("negative = attraction (Algodoo convention)");
        if let Commit::Done(_, new) = precise_drag(
            ui,
            ui.id().with("field-strength"),
            &mut strength,
            -2000.0,
            5.0,
        ) {
            commit_fields(
                selection,
                props,
                Some(FieldSource {
                    strength: new,
                    ..field
                }),
            );
        }
    });
    ui.horizontal(|ui| {
        ui.label("falloff");
        let mut falloff = field.falloff;
        egui::ComboBox::from_id_salt(ui.id().with("field-falloff"))
            .selected_text(format!("{falloff:?}"))
            .show_ui(ui, |ui| {
                for option in [FieldFalloff::Linear, FieldFalloff::Quadratic] {
                    if ui
                        .selectable_value(&mut falloff, option, format!("{option:?}"))
                        .clicked()
                    {
                        commit_fields(
                            selection,
                            props,
                            Some(FieldSource {
                                falloff: option,
                                ..field
                            }),
                        );
                    }
                }
            });
    });
}

/// Commits one batched field edit across the selection (each target's own
/// prior value captured for undo).
fn commit_fields(
    selection: &Selection,
    props: &mut BodyProps,
    new: Option<crate::domain::field::FieldSource>,
) {
    let changes: Vec<PropertyChange> = selection
        .iter()
        .filter_map(|e| {
            let id = props.ids.get(e).ok().copied()?;
            let old = props.field_q.get(e).ok().copied();
            Some(PropertyChange {
                id,
                old: PropertyValue::Field(old),
                new: PropertyValue::Field(new),
            })
        })
        .collect();
    if !changes.is_empty() {
        props.edits.write(PropertyEditIntent { changes });
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

/// The depth editor: numeric front/back faces of the authored band
/// (collision volume ≡ render depth — one value, no filter art). The
/// draggable band bars live in the Depth panel dock; this is the exact
/// numeric view.
pub fn depth_section(ui: &mut egui::Ui, selection: &Selection, props: &mut BodyProps) {
    let Some(primary) = selection.primary() else {
        return;
    };
    let Ok(band) = props.depths.get(primary).copied() else {
        return;
    };
    let mut near = band.near;
    let mut far = band.far;
    ui.horizontal(|ui| {
        ui.label("front")
            .on_hover_text("depth of the front face (0 = the interaction plane)");
        let cn = precise_drag(ui, ui.id().with("depth-near"), &mut near, 0.0, 0.5);
        ui.label("back")
            .on_hover_text("depth of the back face (thicker = spans more layers)");
        let cf = precise_drag(
            ui,
            ui.id().with("depth-far"),
            &mut far,
            crate::core::constants::LAYER_HEIGHT,
            0.5,
        );
        if matches!(cn, Commit::Done(..)) || matches!(cf, Commit::Done(..)) {
            commit_to_selection(
                selection,
                &props.ids,
                &props.depths,
                PropertyValue::Depth,
                move |_| DepthBand { near, far }.sanitized(),
                &mut props.edits,
            );
        }
    });
    // Config seam: only written on an actual toggle (change detection).
    let mut show = props.debug.show_layers;
    if ui
        .checkbox(&mut show, "color bodies by depth layer")
        .on_hover_text("outline every body in its front-most derived layer's hue")
        .changed()
    {
        props.debug.show_layers = show;
    }
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
            inspector_body(ui, &selection, &mut props);
        });
    panel.open = open;
    Ok(())
}

/// The pop-out's body, rendered per the *primary's* kind so a non-body
/// selection never shows dead section headers (the freeze fix): a rigid body
/// gets the sensor readouts + property sections; a behavior node gets its kind
/// editor; anything else gets a note rather than empty headers.
fn inspector_body(ui: &mut egui::Ui, selection: &Selection, props: &mut BodyProps) {
    let Some(primary) = selection.primary() else {
        ui.weak("Nothing selected.");
        return;
    };
    if props.is_body(primary) {
        ui.label(egui::RichText::new("Sensors").strong())
            .on_hover_text("live read-only quantities — the object's sensor ports");
        props.sensors(ui, primary);
        ui.separator();
        ui.label(egui::RichText::new("Physics").strong());
        physics_section(ui, selection, props);
        ui.separator();
        ui.label(egui::RichText::new("Shape").strong());
        shape_section(ui, selection, props);
        ui.separator();
        ui.label(egui::RichText::new("Appearance").strong());
        appearance_section(ui, selection, props);
        ui.separator();
        ui.label(egui::RichText::new("Depth").strong());
        depth_section(ui, selection, props);
        return;
    }
    // A behavior node: edit its kind here too (previously only reachable from
    // the Signals dock / context menu — the pop-out showed dead headers).
    if let Some((id, kind)) = props
        .ids
        .get(primary)
        .ok()
        .copied()
        .zip(props.node_kinds.get(primary).ok().cloned())
    {
        ui.label(egui::RichText::new(format!("Node: {}", kind.label())).strong());
        if let Some(next) = crate::ui::signals::node_kind_editor(ui, "inspector", &kind) {
            props.edits.write(PropertyEditIntent {
                changes: vec![PropertyChange {
                    id,
                    old: PropertyValue::NodeKind(kind),
                    new: PropertyValue::NodeKind(next),
                }],
            });
        }
        return;
    }
    ui.weak("This selection has no editable properties here (use its own editor).");
}
