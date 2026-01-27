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

#[derive(Clone, PartialEq, Debug)]
enum InspectorValue<T> {
    Same(T),
    Mixed,
}

#[derive(Default)]
struct InspectorState {
    transform: Option<InspectorValue<Transform>>,
    editable_shape: Option<InspectorValue<EditableShape>>,
    rigid_body: Option<InspectorValue<RigidBody>>,
    friction: Option<InspectorValue<Friction>>,
    restitution: Option<InspectorValue<Restitution>>,
    fill: Option<InspectorValue<Fill>>,
    stroke: Option<InspectorValue<Stroke>>,
    sensor: Option<InspectorValue<bool>>,
    locked_axes: Option<InspectorValue<LockedAxes>>,
    density: Option<InspectorValue<f32>>,
    gravity_scale: Option<InspectorValue<f32>>,
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

            // Extract state from all selected entities
            // We collect entities into a Vec to pass to state extractor
            let entities: Vec<Entity> = inspector.selection.0.iter().copied().collect();
            let state = extract_inspector_state(&entities, &inspector.entity_query);

            ui.heading("Inspector");
            ui.label(format!("Selected: {} entities", entities.len()));
            ui.separator();

            // Transform
            if let Some(mut val) = state.transform {
                ui.collapsing("Transform", |ui| {
                    if inspect_transform(ui, &mut val) {
                        apply_transform_change(&mut inspector, &entities, val);
                    }
                });
                ui.separator();
            }

            // Shape
            if let Some(mut val) = state.editable_shape {
                 ui.collapsing("Shape", |ui| {
                    if inspect_shape(ui, &mut val) {
                        apply_shape_change(&mut inspector, &entities, val);
                    }
                });
                ui.separator();
            }

            // Physics
            ui.collapsing("Physics", |ui| {
                // Rigid Body
                if let Some(mut val) = state.rigid_body {
                    if inspect_rigid_body(ui, &mut val) {
                        apply_rigid_body_change(&mut inspector, &entities, val);
                    }
                    ui.separator();
                }

                // Sensor
                if let Some(mut val) = state.sensor {
                    if inspect_sensor(ui, &mut val) {
                         apply_sensor_change(&mut inspector, &entities, val);
                    }
                     ui.separator();
                }

                // Friction
                if let Some(mut val) = state.friction {
                    if inspect_friction(ui, &mut val) {
                        apply_friction_change(&mut inspector, &entities, val);
                    }
                     ui.separator();
                }

                // Restitution
                if let Some(mut val) = state.restitution {
                    if inspect_restitution(ui, &mut val) {
                         apply_restitution_change(&mut inspector, &entities, val);
                    }
                     ui.separator();
                }

                 // Density
                if let Some(mut val) = state.density {
                    if inspect_density(ui, &mut val) {
                         apply_density_change(&mut inspector, &entities, val);
                    }
                     ui.separator();
                }

                 // Gravity
                if let Some(mut val) = state.gravity_scale {
                    if inspect_gravity(ui, &mut val) {
                         apply_gravity_change(&mut inspector, &entities, val);
                    }
                     ui.separator();
                }

                 // Locked Axes
                if let Some(mut val) = state.locked_axes {
                    if inspect_locked_axes(ui, &mut val) {
                         apply_locked_axes_change(&mut inspector, &entities, val);
                    }
                     ui.separator();
                }
            });
            ui.separator();

            // Visuals
            ui.collapsing("Visuals", |ui| {
                if let Some(mut val) = state.fill {
                    if inspect_fill(ui, &mut val) {
                         apply_fill_change(&mut inspector, &entities, val);
                    }
                     ui.separator();
                }
                if let Some(mut val) = state.stroke {
                    if inspect_stroke(ui, &mut val) {
                         apply_stroke_change(&mut inspector, &entities, val);
                    }
                     ui.separator();
                }
            });
            ui.separator();

             // Joints (Only supports single selection or unified joint type for now, sticking to first for simplicity on joints)
             if let Some(first) = entities.first() {
                 inspect_joint(ui, &mut inspector, *first);
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
    entities: &[Entity],
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
    if entities.is_empty() {
        return InspectorState::default();
    }

    // Initialize with first entity
    let first = entities[0];
    let Ok((_, t, rb, f, r, eshape, fill, stroke, sensor, locked, mass, grav, _)) = query.get(first) else {
        return InspectorState::default();
    };

    // Helper to init logic
    fn init<T: Clone>(val: Option<&T>) -> Option<InspectorValue<T>> {
        val.map(|v| InspectorValue::Same(v.clone()))
    }
    // Specific helpers
    fn init_bool(val: Option<&Sensor>) -> Option<InspectorValue<bool>> {
        Some(InspectorValue::Same(val.is_some()))
    }
    fn init_mass(val: Option<&ColliderMassProperties>) -> Option<InspectorValue<f32>> {
        val.map(|v| {
            if let ColliderMassProperties::Density(d) = v {
                InspectorValue::Same(*d)
            } else {
                InspectorValue::Same(1.0) // fallback
            }
        })
    }
     fn init_grav(val: Option<&GravityScale>) -> Option<InspectorValue<f32>> {
        val.map(|v| InspectorValue::Same(v.0))
    }

    let mut state = InspectorState {
        transform: init(t.as_deref()),
        editable_shape: init(eshape.as_deref()),
        rigid_body: init(rb.as_deref()),
        friction: init(f.as_deref()),
        restitution: init(r.as_deref()),
        fill: init(fill.as_deref()),
        stroke: init(stroke.as_deref()),
        sensor: init_bool(sensor), // Sensor is unit struct, present = true
        locked_axes: init(locked.as_deref()),
        density: init_mass(mass),
        gravity_scale: init_grav(grav),
    };

    // Merge others
    for &entity in &entities[1..] {
        let Ok((_, t, rb, f, r, eshape, fill, stroke, sensor, locked, mass, grav, _)) = query.get(entity) else {
             continue;
        };

        // Helper merge
        fn merge<T: PartialEq + Clone>(state_val: &mut Option<InspectorValue<T>>, entity_val: Option<&T>) {
            if state_val.is_none() { return; } // Already marked missing
            match entity_val {
                Some(v) => {
                    if let Some(InspectorValue::Same(existing)) = state_val {
                        if existing != v {
                            *state_val = Some(InspectorValue::Mixed);
                        }
                    }
                }
                None => *state_val = None, // Missing on this entity -> don't show
            }
        }

        // Merge Transform
        merge(&mut state.transform, t.as_deref());
        merge(&mut state.editable_shape, eshape.as_deref());
        merge(&mut state.rigid_body, rb.as_deref());
        merge(&mut state.friction, f.as_deref());
        merge(&mut state.restitution, r.as_deref());
        merge(&mut state.fill, fill.as_deref());
        merge(&mut state.stroke, stroke.as_deref());
        merge(&mut state.locked_axes, locked.as_deref());

        // Sensor (bool)
        let is_sensor = sensor.is_some();
        if let Some(InspectorValue::Same(existing)) = state.sensor {
            if existing != is_sensor {
                state.sensor = Some(InspectorValue::Mixed);
            }
        }

        // Density
        let d = if let Some(ColliderMassProperties::Density(v)) = mass { Some(v) } else if mass.is_some() { Some(&1.0) } else { None };
        merge(&mut state.density, d);

        // Gravity
        let g = grav.as_ref().map(|v| &v.0);
        merge(&mut state.gravity_scale, g);
    }

    state
}

// UI Helpers

fn inspect_transform(ui: &mut egui::Ui, val: &mut InspectorValue<Transform>) -> bool {
    let mut changed = false;
    let mut t = match val {
        InspectorValue::Same(v) => *v,
        InspectorValue::Mixed => Transform::default(),
    };

    let mixed = matches!(val, InspectorValue::Mixed);

    ui.horizontal(|ui| {
        ui.label("Pos X:");
        let mut x = if mixed { 0.0 } else { t.translation.x };
        if ui.add(egui::DragValue::new(&mut x).speed(DRAG_SPEED)).changed() {
            t.translation.x = x;
            changed = true;
        }

        ui.label("Pos Y:");
        let mut y = if mixed { 0.0 } else { t.translation.y };
         if ui.add(egui::DragValue::new(&mut y).speed(DRAG_SPEED)).changed() {
            t.translation.y = y;
            changed = true;
        }
    });

    // Rotation
    ui.horizontal(|ui| {
        ui.label("Rotation:");
        let mut rot = if mixed { 0.0 } else { t.rotation.to_euler(EulerRot::XYZ).2 };
        if ui.drag_angle(&mut rot).changed() {
            t.rotation = Quat::from_rotation_z(rot);
            changed = true;
        }
    });

    if mixed && !changed {
        ui.label("(Mixed Values - Edit to set all)");
    }

    if changed {
        *val = InspectorValue::Same(t);
    }
    changed
}

fn inspect_shape(ui: &mut egui::Ui, val: &mut InspectorValue<EditableShape>) -> bool {
    let mut eshape = match val {
        InspectorValue::Same(v) => v.clone(),
        InspectorValue::Mixed => return false, // Too complex to edit mixed shapes
    };

    let mut changed = false;
    match &mut eshape.shape {
        ShapeType::Box { width, height } => {
            ui.label("Box Dimensions");
            if ui.add(egui::DragValue::new(width).speed(DRAG_SPEED).prefix("Width: ")).changed() { changed = true; }
            if ui.add(egui::DragValue::new(height).speed(DRAG_SPEED).prefix("Height: ")).changed() { changed = true; }
        }
        ShapeType::Circle { radius } => {
            ui.label("Circle Dimensions");
             if ui.add(egui::DragValue::new(radius).speed(DRAG_SPEED).prefix("Radius: ")).changed() { changed = true; }
        }
        ShapeType::Polygon { points } => {
            ui.label("Polygon");
            ui.label(format!("Vertices: {}", points.len()));
        }
    }

    if changed {
        *val = InspectorValue::Same(eshape);
    }
    changed
}

fn inspect_rigid_body(ui: &mut egui::Ui, val: &mut InspectorValue<RigidBody>) -> bool {
    let mut rb = match val {
        InspectorValue::Same(v) => *v,
        InspectorValue::Mixed => RigidBody::Dynamic, // default
    };
    let mixed = matches!(val, InspectorValue::Mixed);

    ui.horizontal(|ui| {
        ui.label("Type:");
        let options = [
            RigidBody::Dynamic,
            RigidBody::Fixed,
            RigidBody::KinematicPositionBased,
        ];

        let text = if mixed { "Mixed".to_string() } else { format!("{:?}", rb) };

        egui::ComboBox::from_id_salt("rb_type")
            .selected_text(text)
            .show_ui(ui, |ui| {
                for option in options {
                    if ui.selectable_value(&mut rb, option, format!("{:?}", option)).clicked() {
                        *val = InspectorValue::Same(rb);
                    }
                }
            });
    });

    // We handle changed implicitly by setting *val inside callback
    // But we need to return bool.
    // The SelectableValue modifies `rb`. We check if it matches `val`.
    if let InspectorValue::Same(_new_rb) = *val {
         // This logic is bit circular because we modify val inside closure.
         // Better:
         // If mixed, we show "Mixed". User picks -> val becomes Same(Picked).
         // If same, we show current. User picks -> val becomes Same(Picked).
         // We can return true if val changed.
         // Actually, simpler:
         return !matches!(val, InspectorValue::Mixed) || !mixed;
    }
    false
}

// Simplified generic handlers
fn inspect_float_prop(ui: &mut egui::Ui, val: &mut InspectorValue<f32>, label: &str, range: std::ops::RangeInclusive<f32>) -> bool {
    let mut v = match val { InspectorValue::Same(v) => *v, InspectorValue::Mixed => 0.0 };
    let mixed = matches!(val, InspectorValue::Mixed);

    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.add(egui::Slider::new(&mut v, range).text(label)).changed() {
            changed = true;
        }
    });

    if mixed { ui.label("(Mixed)"); }

    if changed { *val = InspectorValue::Same(v); }
    changed
}

fn inspect_friction(ui: &mut egui::Ui, val: &mut InspectorValue<Friction>) -> bool {
    let mut f = match val { InspectorValue::Same(v) => v.coefficient, InspectorValue::Mixed => 0.0 };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;
    if ui.add(egui::Slider::new(&mut f, 0.0..=FRICTION_MAX).text("Friction")).changed() { changed = true; }
    if mixed { ui.label("(Mixed)"); }
    if changed { *val = InspectorValue::Same(Friction::coefficient(f)); }
    changed
}

fn inspect_restitution(ui: &mut egui::Ui, val: &mut InspectorValue<Restitution>) -> bool {
    let mut c = match val { InspectorValue::Same(v) => v.coefficient, InspectorValue::Mixed => 0.0 };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;
    if ui.add(egui::Slider::new(&mut c, 0.0..=RESTITUTION_MAX).text("Restitution")).changed() { changed = true; }
    if mixed { ui.label("(Mixed)"); }
    if changed { *val = InspectorValue::Same(Restitution::coefficient(c)); }
    changed
}

fn inspect_density(ui: &mut egui::Ui, val: &mut InspectorValue<f32>) -> bool {
    let mut d = match val { InspectorValue::Same(v) => *v, InspectorValue::Mixed => 1.0 };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;
    if ui.add(egui::DragValue::new(&mut d).speed(DRAG_SPEED).range(DENSITY_MIN..=DENSITY_MAX).prefix("Density: ")).changed() { changed = true; }
    if mixed { ui.label("(Mixed)"); }
    if changed { *val = InspectorValue::Same(d); }
    changed
}

fn inspect_gravity(ui: &mut egui::Ui, val: &mut InspectorValue<f32>) -> bool {
    let mut g = match val { InspectorValue::Same(v) => *v, InspectorValue::Mixed => 1.0 };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;
    if ui.add(egui::DragValue::new(&mut g).speed(DRAG_SPEED).prefix("Gravity Scale: ")).changed() { changed = true; }
    if mixed { ui.label("(Mixed)"); }
    if changed { *val = InspectorValue::Same(g); }
    changed
}

fn inspect_sensor(ui: &mut egui::Ui, val: &mut InspectorValue<bool>) -> bool {
    let mut is_sensor = match val { InspectorValue::Same(v) => *v, InspectorValue::Mixed => false };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;

    let label = if mixed { "Sensor (Mixed)" } else { "Sensor" };
    if ui.checkbox(&mut is_sensor, label).clicked() { changed = true; }

    if changed { *val = InspectorValue::Same(is_sensor); }
    changed
}

fn inspect_locked_axes(ui: &mut egui::Ui, val: &mut InspectorValue<LockedAxes>) -> bool {
    let mut locked = match val { InspectorValue::Same(v) => v.contains(LockedAxes::ROTATION_LOCKED), InspectorValue::Mixed => false };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;

    let label = if mixed { "Lock Rotation (Mixed)" } else { "Lock Rotation" };
    if ui.checkbox(&mut locked, label).clicked() { changed = true; }

    if changed {
        *val = InspectorValue::Same(if locked { LockedAxes::ROTATION_LOCKED } else { LockedAxes::empty() });
    }
    changed
}

fn inspect_fill(ui: &mut egui::Ui, val: &mut InspectorValue<Fill>) -> bool {
    let mut color = match val { InspectorValue::Same(v) => v.color, InspectorValue::Mixed => Color::WHITE };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label("Color:");
        let mut c_arr = color.to_srgba().to_f32_array();
        if ui.color_edit_button_rgba_unmultiplied(&mut c_arr).changed() {
            color = Color::srgba(c_arr[0], c_arr[1], c_arr[2], c_arr[3]);
            changed = true;
        }
        if mixed { ui.label("(Mixed)"); }
    });

    if changed {
         // Create dummy Fill to pass back
         let f = Fill { color, options: FillOptions::default() };
         *val = InspectorValue::Same(f);
    }
    changed
}

fn inspect_stroke(ui: &mut egui::Ui, val: &mut InspectorValue<Stroke>) -> bool {
    let mut color = match val { InspectorValue::Same(v) => v.color, InspectorValue::Mixed => Color::BLACK };
    let mut width = match val { InspectorValue::Same(v) => v.options.line_width, InspectorValue::Mixed => 1.0 };
    let mixed = matches!(val, InspectorValue::Mixed);
    let mut changed = false;

    ui.horizontal(|ui| {
        let mut c_arr = color.to_srgba().to_f32_array();
         if ui.color_edit_button_rgba_unmultiplied(&mut c_arr).changed() {
            color = Color::srgba(c_arr[0], c_arr[1], c_arr[2], c_arr[3]);
            changed = true;
        }
        if ui.add(egui::DragValue::new(&mut width).speed(0.1).prefix("Width: ")).changed() {
            changed = true;
        }
    });
    if mixed { ui.label("(Mixed Visuals)"); }

    if changed {
         let s = Stroke { color, options: StrokeOptions::default().with_line_width(width) };
         *val = InspectorValue::Same(s);
    }
    changed
}


// Appliers
fn apply_transform_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<Transform>) {
    if let InspectorValue::Same(t) = val {
        for &e in entities {
            if let Ok((_, Some(mut tr), ..)) = inspector.entity_query.get_mut(e) {
                // If we edited mixed values, we might only want to apply changed fields.
                // But here we overwrite. Simpler.
                tr.translation = t.translation;
                tr.rotation = t.rotation;
                wake_up(e, inspector);
            }
        }
    }
}

fn apply_shape_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<EditableShape>) {
    if let InspectorValue::Same(s) = val {
        for &e in entities {
            if let Ok((_, _, _, _, _, Some(mut es), ..)) = inspector.entity_query.get_mut(e) {
                es.shape = s.shape.clone();
                // Physics update is automatic via EditableShapePlugin
            }
        }
    }
}

fn apply_rigid_body_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<RigidBody>) {
    if let InspectorValue::Same(rb) = val {
        for &e in entities {
             inspector.commands.entity(e).insert(rb);
             wake_up(e, inspector);
        }
    }
}

fn apply_friction_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<Friction>) {
    if let InspectorValue::Same(f) = val {
        for &e in entities {
            if let Ok((_, _, _, Some(mut fr), ..)) = inspector.entity_query.get_mut(e) {
                fr.coefficient = f.coefficient;
            } else {
                inspector.commands.entity(e).insert(f);
            }
        }
    }
}

fn apply_restitution_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<Restitution>) {
    if let InspectorValue::Same(r) = val {
        for &e in entities {
             if let Ok((_, _, _, _, Some(mut re), ..)) = inspector.entity_query.get_mut(e) {
                re.coefficient = r.coefficient;
            } else {
                inspector.commands.entity(e).insert(r);
            }
        }
    }
}

fn apply_density_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<f32>) {
    if let InspectorValue::Same(d) = val {
        for &e in entities {
            inspector.commands.entity(e).insert(ColliderMassProperties::Density(d));
            wake_up(e, inspector);
        }
    }
}

fn apply_gravity_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<f32>) {
    if let InspectorValue::Same(g) = val {
        for &e in entities {
            inspector.commands.entity(e).insert(GravityScale(g));
            wake_up(e, inspector);
        }
    }
}

fn apply_sensor_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<bool>) {
    if let InspectorValue::Same(is_sensor) = val {
        for &e in entities {
            if is_sensor {
                inspector.commands.entity(e).insert(Sensor);
            } else {
                inspector.commands.entity(e).remove::<Sensor>();
            }
        }
    }
}

fn apply_locked_axes_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<LockedAxes>) {
    if let InspectorValue::Same(l) = val {
        for &e in entities {
             inspector.commands.entity(e).insert(l);
             wake_up(e, inspector);
        }
    }
}

fn apply_fill_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<Fill>) {
    if let InspectorValue::Same(f) = val {
        for &e in entities {
            if let Ok((_, _, _, _, _, _, Some(mut fi), ..)) = inspector.entity_query.get_mut(e) {
                fi.color = f.color;
            }
        }
    }
}

fn apply_stroke_change(inspector: &mut InspectorQuery, entities: &[Entity], val: InspectorValue<Stroke>) {
    if let InspectorValue::Same(s) = val {
        for &e in entities {
            if let Ok((_, _, _, _, _, _, _, Some(mut st), ..)) = inspector.entity_query.get_mut(e) {
                st.color = s.color;
                st.options = s.options;
            }
        }
    }
}

fn wake_up(entity: Entity, inspector: &mut InspectorQuery) {
    // We can't easily check if Sleeping exists in the bundle without query.
    // We'll just try to insert Sleeping::disabled() if we know it might have physics.
    // Safest is to just insert it.
    inspector.commands.entity(entity).insert(Sleeping::disabled());
}


fn inspect_joint(ui: &mut egui::Ui, inspector: &mut InspectorQuery, first_entity: Entity) {
    // Keep existing logic for joints, as they are complex to multi-edit
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
