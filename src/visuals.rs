//! Visual effects module.
//!
//! Handles lighting, shadows, and tracers.

use crate::input::cursor::CursorWorldPos;
use crate::input::editable_shape::EditableShape;
use crate::prelude::*;
use bevy_rapier2d::prelude::RigidBody;
use std::collections::VecDeque;

/// Resource for visual settings.
#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub struct RenderSettings {
    /// Intensity of the mouse light.
    pub point_light_intensity: f32,
    /// Radius of the mouse light.
    pub point_light_radius: f32,
    /// Color of the mouse light.
    pub point_light_color: Color,
    /// Global ambient light brightness.
    pub ambient_light_brightness: f32,
    /// Whether to cast shadows.
    pub cast_shadows: bool,
    /// Whether to show tracers on moving objects.
    pub show_tracers: bool,
    /// Number of frames to keep for tracers.
    pub tracer_length: usize,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            point_light_intensity: 100_000.0,
            point_light_radius: 1000.0,
            point_light_color: Color::WHITE,
            ambient_light_brightness: 200.0,
            cast_shadows: true,
            show_tracers: false,
            tracer_length: 50,
        }
    }
}

/// Component for the mouse light.
#[derive(Component)]
pub struct MouseLight;

/// Component for tracers.
#[derive(Component, Default)]
pub struct Tracer {
    /// History of positions for the tracer tail.
    pub history: VecDeque<Vec2>,
}

/// Plugin for visual effects.
pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderSettings>()
            .register_type::<RenderSettings>()
            .insert_resource(AmbientLight {
                color: Color::WHITE,
                brightness: 500.0,
            })
            .add_systems(Startup, setup_visuals)
            .add_systems(
                Update,
                (
                    update_mouse_light,
                    sync_render_settings,
                    manage_tracers,
                    update_and_draw_tracers,
                ),
            );
    }
}

fn setup_visuals(mut commands: Commands) {
    commands.spawn((
        PointLight {
            intensity: 100_000.0,
            range: 1000.0,
            color: Color::WHITE,
            shadows_enabled: true,
            ..default()
        },
        MouseLight,
    ));
}

fn update_mouse_light(
    cursor_pos: Res<CursorWorldPos>,
    mut query: Query<&mut Transform, With<MouseLight>>,
) {
    if let Some(pos) = cursor_pos.0 {
        for mut transform in &mut query {
            // Lift light well above plane for 3D effect
            transform.translation = pos.extend(50.0);
        }
    }
}

fn sync_render_settings(
    settings: Res<RenderSettings>,
    mut ambient_light: ResMut<AmbientLight>,
    mut light_query: Query<&mut PointLight, With<MouseLight>>,
) {
    ambient_light.brightness = settings.ambient_light_brightness;

    for mut light in &mut light_query {
        light.intensity = settings.point_light_intensity;
        light.range = settings.point_light_radius;
        light.color = settings.point_light_color;
        light.shadows_enabled = settings.cast_shadows;
    }
}

fn manage_tracers(
    mut commands: Commands,
    settings: Res<RenderSettings>,
    query: Query<Entity, (With<RigidBody>, With<EditableShape>, Without<Tracer>)>,
    query_disable: Query<Entity, (With<Tracer>, With<EditableShape>)>,
) {
    if settings.show_tracers {
        for entity in &query {
            commands.entity(entity).insert(Tracer::default());
        }
    } else {
        for entity in &query_disable {
            commands.entity(entity).remove::<Tracer>();
        }
    }
}

fn update_and_draw_tracers(
    mut gizmos: Gizmos,
    mut query: Query<(&GlobalTransform, &mut Tracer)>,
    settings: Res<RenderSettings>,
) {
    if !settings.show_tracers {
        return;
    }

    for (transform, mut tracer) in &mut query {
        let pos = transform.translation().truncate();

        // Add current position
        tracer.history.push_back(pos);

        // Limit history
        while tracer.history.len() > settings.tracer_length {
            tracer.history.pop_front();
        }

        // Draw
        if tracer.history.len() >= 2 {
            // Use 3D linestrip but on Z=0 (or slightly above to avoid z-fighting)
            let points = tracer.history.iter().map(|p| p.extend(1.0));
            gizmos.linestrip(
                points,
                Color::srgb(1.0, 1.0, 0.0),
            );
        }
    }
}
