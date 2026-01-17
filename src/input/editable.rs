use bevy::prelude::*;
use avian2d::prelude::*;
use bevy_prototype_lyon::prelude::*;

#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
pub struct EditableBox {
    pub width: f64,
    pub height: f64,
}

#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
pub struct EditableCircle {
    pub radius: f64,
}

pub struct EditablePlugin;

impl Plugin for EditablePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<EditableBox>();
        app.register_type::<EditableCircle>();
        app.add_systems(Update, (resize_box, resize_circle));
    }
}

fn resize_box(
    mut commands: Commands,
    query: Query<(Entity, &EditableBox), Changed<EditableBox>>,
) {
    for (entity, editable) in query.iter() {
        if editable.width <= 0.0 || editable.height <= 0.0 {
            continue;
        }

        let shape = shapes::Rectangle {
            extents: Vec2::new(editable.width as f32, editable.height as f32),
            origin: shapes::RectangleOrigin::Center,
            ..default()
        };

        // Note: Cannot preserve Fill/Stroke due to bevy_prototype_lyon incompatibility with Bevy 0.18 Component trait query.
        // Using defaults.
        commands.entity(entity)
            .insert(
                ShapeBuilder::with(&shape)
                    .fill(Color::srgb(0.5, 0.5, 1.0))
                    .stroke(Stroke::new(Color::BLACK, 0.1))
                    .build()
            )
            .insert(Collider::rectangle(editable.width, editable.height));
    }
}

fn resize_circle(
    mut commands: Commands,
    query: Query<(Entity, &EditableCircle), Changed<EditableCircle>>,
) {
    for (entity, editable) in query.iter() {
        if editable.radius <= 0.0 {
            continue;
        }

        let shape = shapes::Circle {
            radius: editable.radius as f32,
            center: Vec2::ZERO,
        };

        commands.entity(entity)
            .insert(
                 ShapeBuilder::with(&shape)
                    .fill(Color::srgb(1.0, 0.5, 0.5))
                    .stroke(Stroke::new(Color::BLACK, 0.1))
                    .build()
            )
            .insert(Collider::circle(editable.radius));
    }
}
