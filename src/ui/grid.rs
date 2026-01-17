use crate::prelude::*;

pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GridSettings>();
        app.add_systems(Update, draw_grid.run_if(|settings: Res<GridSettings>| settings.show));
    }
}

#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub struct GridSettings {
    pub show: bool,
    pub snap: bool,
    pub spacing: f64,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            show: true,
            snap: true,
            spacing: 1.0,
        }
    }
}

fn draw_grid(
    settings: Res<GridSettings>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut gizmos: Gizmos,
    // window_query: Query<&Window>, // Unused
) {
    // Bevy 0.18: get_single returns Result, use iter().next() or match if single is expected but might fail
    let Ok((_, transform)) = camera_query.get_single() else {
        return;
    };

    // Calculate visible area
    // Simplified: Just draw a large enough grid around camera position.
    // Better: Viewport calculation.

    // For now, let's draw a grid around the camera center, covering typical screen size.
    let center = transform.translation().truncate();
    let zoom = transform.scale().x; // Assuming uniform scale

    // Estimate view bounds (rough)
    let width = 100.0 * zoom; // Arbitrary large number
    let height = 100.0 * zoom;

    let left = center.x - width;
    let right = center.x + width;
    let bottom = center.y - height;
    let top = center.y + height;

    let spacing = settings.spacing as f32;

    // Snap start to spacing
    let start_x = (left / spacing).floor() * spacing;
    let start_y = (bottom / spacing).floor() * spacing;

    let count_x = ((right - left) / spacing).ceil() as i32;
    let count_y = ((top - bottom) / spacing).ceil() as i32;

    let color = Color::srgba(1.0, 1.0, 1.0, 0.1);

    for i in 0..=count_x {
        let x = start_x + (i as f32) * spacing;
        gizmos.line_2d(Vec2::new(x, bottom), Vec2::new(x, top), color);
    }

    for i in 0..=count_y {
        let y = start_y + (i as f32) * spacing;
        gizmos.line_2d(Vec2::new(left, y), Vec2::new(right, y), color);
    }
}
