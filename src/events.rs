//! Game events definition.
//!
//! This module defines the events used to communicate user intent and game state changes
//! across different systems, following an event-driven architecture.

use crate::input::editable_shape::ShapeType;
use bevy::prelude::*;
use bevy_rapier2d::prelude::{LockedAxes, RigidBody};

/// Plugin for registering game events.
pub struct GameEventsPlugin;

impl Plugin for GameEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SpawnShapeEvent>()
            .add_event::<SpawnGroundEvent>()
            .add_event::<SpawnJointEvent>()
            .add_event::<SpawnPrismaticJointEvent>()
            .add_event::<SpawnFixedJointEvent>()
            .add_event::<UndoEvent>()
            .add_event::<RedoEvent>()
            .add_event::<PropertyChangeEvent>();
    }
}

/// Event triggered when a shape should be spawned.
#[derive(Event, Debug, Clone)]
pub struct SpawnShapeEvent {
    /// World position to spawn at.
    pub position: Vec2,
    /// Type of shape to spawn.
    pub shape: ShapeType,
}

/// Event triggered when a ground plane should be spawned.
#[derive(Event, Debug, Clone)]
pub struct SpawnGroundEvent {
    /// Center position of the ground.
    pub position: Vec2,
    /// Rotation in radians.
    pub rotation: f32,
}

/// Event triggered when a joint should be spawned.
#[derive(Event, Debug, Clone)]
pub struct SpawnJointEvent {
    /// First entity (A).
    pub entity_a: Entity,
    /// Second entity (B), or None for World.
    pub entity_b: Option<Entity>,
    /// Local anchor on A.
    pub anchor_a: Vec2,
    /// Local anchor on B.
    pub anchor_b: Vec2,
    /// Joint compliance.
    pub compliance: f32,
}

/// Event triggered when a prismatic joint should be spawned.
#[derive(Event, Debug, Clone)]
pub struct SpawnPrismaticJointEvent {
    /// First entity (A).
    pub entity_a: Entity,
    /// Second entity (B).
    pub entity_b: Option<Entity>,
    /// Local anchor on A.
    pub anchor_a: Vec2,
    /// Local anchor on B.
    pub anchor_b: Vec2,
    /// Axis of movement.
    pub axis: Vec2,
    /// Joint compliance.
    pub compliance: f32,
}

/// Event triggered when a fixed joint should be spawned.
#[derive(Event, Debug, Clone)]
pub struct SpawnFixedJointEvent {
    /// First entity (A).
    pub entity_a: Entity,
    /// Second entity (B).
    pub entity_b: Option<Entity>,
    /// Local anchor on A.
    pub anchor_a: Vec2,
    /// Local anchor on B.
    pub anchor_b: Vec2,
    /// Joint compliance.
    pub compliance: f32,
    /// Rotation offset A.
    pub rot_a: f32,
    /// Rotation offset B.
    pub rot_b: f32,
}

/// Event triggered to undo the last command.
#[derive(Event, Debug, Clone, Copy)]
pub struct UndoEvent;

/// Event triggered to redo the last command.
#[derive(Event, Debug, Clone, Copy)]
pub struct RedoEvent;

/// Enum representing the type of property change.
#[derive(Debug, Clone)]
pub enum PropertyChange {
    /// Change transform.
    Transform(Transform),
    /// Change shape geometry.
    Shape(ShapeType),
    /// Change rigid body type.
    RigidBody(RigidBody),
    /// Change friction coefficient.
    Friction(f32),
    /// Change restitution coefficient.
    Restitution(f32),
    /// Change density.
    Density(f32),
    /// Change gravity scale.
    GravityScale(f32),
    /// Change sensor status.
    Sensor(bool),
    /// Change locked axes.
    LockedAxes(LockedAxes),
    /// Change fill color.
    FillColor(Color),
    /// Change stroke color.
    StrokeColor(Color),
    /// Change stroke width.
    StrokeWidth(f32),
    /// Change revolute joint limits.
    RevoluteLimits([f32; 2]),
    /// Change revolute joint motor settings.
    RevoluteMotor {
        /// Target velocity in radians/sec.
        target_vel: f32,
        /// Damping.
        damping: f32,
        /// Max force.
        max_force: f32,
    },
    /// Change prismatic joint limits.
    PrismaticLimits([f32; 2]),
    /// Change prismatic joint motor settings.
    PrismaticMotor {
        /// Target velocity.
        target_vel: f32,
        /// Damping.
        damping: f32,
        /// Max force.
        max_force: f32,
    },
}

/// Event triggered when a property is changed via UI.
#[derive(Event, Debug, Clone)]
pub struct PropertyChangeEvent {
    /// The entity to modify.
    pub entity: Entity,
    /// The change to apply.
    pub change: PropertyChange,
}
