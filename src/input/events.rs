//! Events for handling user input and tool interactions.
//!
//! Defines the protocol for modifying the world, allowing tools and UI to decouple
//! from specific implementation details of how changes are applied.

use crate::prelude::*;
use bevy::prelude::*;

/// Event to spawn a box.
#[derive(Event)]
pub struct SpawnBoxEvent {
    /// The center position of the box.
    pub position: Vec2,
    /// The width of the box.
    pub width: f32,
    /// The height of the box.
    pub height: f32,
}

/// Event to spawn a circle.
#[derive(Event)]
pub struct SpawnCircleEvent {
    /// The center position of the circle.
    pub position: Vec2,
    /// The radius of the circle.
    pub radius: f32,
}

/// Event to spawn a polygon.
#[derive(Event)]
pub struct SpawnPolygonEvent {
    /// The center position of the polygon.
    pub position: Vec2,
    /// The vertices of the polygon relative to the center.
    pub vertices: Vec<Vec2>,
}

/// Event to spawn a ground plane.
#[derive(Event)]
pub struct SpawnGroundEvent {
    /// The center position of the ground surface.
    pub position: Vec2,
    /// The rotation of the ground plane in radians.
    pub rotation: f32,
}

/// The type of joint to create.
#[derive(Debug, Clone)]
pub enum JointType {
    /// A revolute joint (hinge).
    Revolute,
    /// A fixed joint (weld).
    Fixed {
        /// Rotation of body A.
        rot_a: f32,
        /// Rotation of body B.
        rot_b: f32,
    },
    /// A prismatic joint (slider).
    Prismatic {
        /// The axis of the slider.
        axis: Vec2,
    },
    /// A spring joint.
    Spring {
        /// The stiffness of the spring.
        stiffness: f32,
        /// The damping of the spring.
        damping: f32,
        /// The rest length of the spring.
        length: f32,
    },
    /// A rope joint.
    Rope {
        /// The maximum length of the rope.
        length: f32,
    },
}

/// Event to spawn a joint.
#[derive(Event)]
pub struct SpawnJointEvent {
    /// The type of joint to spawn.
    pub joint_type: JointType,
    /// The first entity attached to the joint.
    pub entity_a: Entity,
    /// The second entity attached to the joint (optional).
    pub entity_b: Option<Entity>,
    /// The anchor point on the first entity (local space).
    pub anchor_a: Vec2,
    /// The anchor point on the second entity (local space).
    pub anchor_b: Vec2,
}

/// Event to modify an entity's transform.
#[derive(Event)]
pub struct ModifyTransformEvent {
    /// The entity to modify.
    pub entity: Entity,
    /// The new translation (if changing).
    pub translation: Option<Vec3>,
    /// The new rotation (if changing).
    pub rotation: Option<Quat>,
}

/// Event to modify an entity's physics properties.
#[derive(Event)]
pub struct ModifyPhysicsEvent {
    /// The entity to modify.
    pub entity: Entity,
    /// The new rigid body type.
    pub body_type: Option<RigidBody>,
    /// The new friction coefficient.
    pub friction: Option<f32>,
    /// The new restitution coefficient.
    pub restitution: Option<f32>,
    /// The new density.
    pub density: Option<f32>,
    /// The new gravity scale.
    pub gravity_scale: Option<f32>,
    /// The new locked axes.
    pub locked_axes: Option<LockedAxes>,
    /// Whether the entity is a sensor.
    pub is_sensor: Option<bool>,
}

/// Event to modify an entity's shape dimensions.
#[derive(Event)]
pub struct ModifyShapeEvent {
    /// The entity to modify.
    pub entity: Entity,
    /// The new box dimensions (width, height).
    pub box_size: Option<Vec2>,
    /// The new circle radius.
    pub circle_radius: Option<f32>,
}

/// Event to modify an entity's visual appearance.
#[derive(Event)]
pub struct ModifyRenderEvent {
    /// The entity to modify.
    pub entity: Entity,
    /// The new fill color.
    pub fill_color: Option<Color>,
    /// The new stroke color.
    pub stroke_color: Option<Color>,
    /// The new stroke width.
    pub stroke_width: Option<f32>,
}

/// Event to modify an entity's attraction properties.
#[derive(Event)]
pub struct ModifyAttractionEvent {
    /// The entity to modify.
    pub entity: Entity,
    /// The new strength.
    pub strength: Option<f32>,
    /// The new range.
    pub range: Option<f32>,
}
