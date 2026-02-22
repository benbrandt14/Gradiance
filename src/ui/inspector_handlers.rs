//! Event handlers for applying property changes from the Inspector.

use crate::events::{PropertyChange, PropertyChangeEvent};
use crate::input::editable_shape::EditableShape;
use crate::physics::controllers::MotorController;
use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;
use bevy_rapier2d::rapier::dynamics::{JointAxis};

/// Handles `PropertyChangeEvent` by applying changes to entities.
///
/// TODO: Integrate with `CommandStack` for Undo/Redo.
/// Currently, this system mutates components directly. It should instead construct
/// a `ModifyPropertyCommand` (to be implemented) and push it to the stack.
pub fn handle_property_change_event(
    mut events: EventReader<PropertyChangeEvent>,
    mut commands: Commands,
    mut query: Query<(
// [dunnage] WARN: very complex type used. Consider factoring parts into `type` definitions
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
        Option<&mut ImpulseJoint>,
        Option<&mut MotorController>,
    )>,
) {
    for event in events.read() {
        let entity = event.entity;

        // Check if entity exists and get components
        if let Ok((
            mut t,
            _rb,
            mut f,
            mut r,
            mut es,
            mut fill,
            mut stroke,
            _sensor,
            _locked,
            _mass,
            _grav,
            _sleep,
            mut joint,
            mut motor_controller,
        )) = query.get_mut(entity)
        {
            match &event.change {
                PropertyChange::Transform(new_t) => {
                    if let Some(t) = &mut t {
                        t.translation = new_t.translation;
                        t.rotation = new_t.rotation;
                    }
                }
                PropertyChange::Shape(new_shape) => {
                    if let Some(es) = &mut es {
                        es.shape = new_shape.clone();
                    }
                }
                PropertyChange::RigidBody(new_rb) => {
                    commands.entity(entity).insert(*new_rb);
                }
                PropertyChange::Friction(val) => {
                    if let Some(f) = &mut f {
                        f.coefficient = *val;
                    } else {
                        commands.entity(entity).insert(Friction::coefficient(*val));
                    }
                }
                PropertyChange::Restitution(val) => {
                    if let Some(r) = &mut r {
                        r.coefficient = *val;
                    } else {
                        commands
                            .entity(entity)
                            .insert(Restitution::coefficient(*val));
                    }
                }
                PropertyChange::Density(val) => {
                    commands
                        .entity(entity)
                        .insert(ColliderMassProperties::Density(*val));
                }
                PropertyChange::GravityScale(val) => {
                    commands.entity(entity).insert(GravityScale(*val));
                }
                PropertyChange::Sensor(is_sensor) => {
                    if *is_sensor {
                        commands.entity(entity).insert(Sensor);
                    } else {
                        commands.entity(entity).remove::<Sensor>();
                    }
                }
                PropertyChange::LockedAxes(axes) => {
                    commands.entity(entity).insert(*axes);
                }
                PropertyChange::FillColor(color) => {
                    if let Some(fill) = &mut fill {
                        fill.color = *color;
                    }
                }
                PropertyChange::StrokeColor(color) => {
                    if let Some(stroke) = &mut stroke {
                        stroke.color = *color;
                    }
                }
                PropertyChange::StrokeWidth(width) => {
                    if let Some(stroke) = &mut stroke {
                        stroke.options.line_width = *width;
                    }
                }
                PropertyChange::RevoluteLimits(limits) => {
                    if let Some(joint) = &mut joint {
                        match &mut joint.data {
                            TypedJoint::RevoluteJoint(r) => { r.set_limits(*limits); }
                            TypedJoint::GenericJoint(g) => {
                                // Assume AngZ -> AngX
                                g.set_limits(JointAxis::AngX, *limits);
                            }
                            _ => {}
                        }
                    }
                }
                PropertyChange::RevoluteMotor {
                    target_vel,
                    damping,
                    max_force,
                } => {
                    if let Some(joint) = &mut joint {
                        let mut updated = false;
                        match &mut joint.data {
                            TypedJoint::RevoluteJoint(r) => {
                                r.set_motor_velocity(*target_vel, *damping);
                                r.set_motor_max_force(*max_force);
                                updated = true;
                            }
                            TypedJoint::GenericJoint(g) => {
                                g.set_motor_velocity(JointAxis::AngX, *target_vel, *damping);
                                g.set_motor_max_force(JointAxis::AngX, *max_force);
                                updated = true;
                            }
                            _ => {}
                        }

                        if updated {
                            // TODO: Avoid hardcoding `oscillate: true`.
                            // This should be exposed in `PropertyChangeEvent` and the Inspector UI.
                            // Users might want a simple motor without oscillation.
                            let controller = MotorController {
                                target_vel: *target_vel,
                                damping: *damping,
                                max_force: *max_force,
                                oscillate: true,
                            };
                            if let Some(mc) = &mut motor_controller {
                                **mc = controller;
                            } else {
                                commands.entity(entity).insert(controller);
                            }
                        }
                    }
                }
                PropertyChange::PrismaticLimits(limits) => {
                    if let Some(joint) = &mut joint {
                        match &mut joint.data {
                            TypedJoint::PrismaticJoint(p) => { p.set_limits(*limits); }
                            TypedJoint::GenericJoint(g) => {
                                // Assume X -> LinX
                                g.set_limits(JointAxis::LinX, *limits);
                            }
                            _ => {}
                        }
                    }
                }
                PropertyChange::PrismaticMotor {
                    target_vel,
                    damping,
                    max_force,
                } => {
                    if let Some(joint) = &mut joint {
                        let mut updated = false;
                        match &mut joint.data {
                            TypedJoint::PrismaticJoint(p) => {
                                p.set_motor_velocity(*target_vel, *damping);
                                p.set_motor_max_force(*max_force);
                                updated = true;
                            }
                            TypedJoint::GenericJoint(g) => {
                                g.set_motor_velocity(JointAxis::LinX, *target_vel, *damping);
                                g.set_motor_max_force(JointAxis::LinX, *max_force);
                                updated = true;
                            }
                            _ => {}
                        }

                        if updated {
                            let controller = MotorController {
                                target_vel: *target_vel,
                                damping: *damping,
                                max_force: *max_force,
                                oscillate: true,
                            };
                            if let Some(mc) = &mut motor_controller {
                                **mc = controller;
                            } else {
                                commands.entity(entity).insert(controller);
                            }
                        }
                    }
                }
            }

            // Wake up physics
            commands.entity(entity).insert(Sleeping::disabled());
        }
    }
}
