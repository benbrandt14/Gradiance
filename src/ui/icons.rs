//! Icon loading and management.
//!
//! This module handles loading game icons and making them available as a resource.

use bevy::prelude::*;

/// Resource containing handles to all loaded game icons.
#[derive(Resource, Default)]
pub struct GameIcons {
    /// Icon for the Select tool.
    pub select: Handle<Image>,
    /// Icon for the Drag tool.
    pub drag: Handle<Image>,
    /// Icon for the Box tool.
    pub box_tool: Handle<Image>,
    /// Icon for the Circle tool.
    pub circle_tool: Handle<Image>,
    /// Icon for the Polygon tool.
    pub polygon_tool: Handle<Image>,
    /// Icon for the Revolute Joint (Axle) tool.
    pub revolute_joint: Handle<Image>,
    /// Icon for the Weld (Fix) tool.
    pub weld: Handle<Image>,
    /// Icon for the Spring tool.
    pub spring: Handle<Image>,
    /// Icon for the Ground tool.
    pub ground: Handle<Image>,
    /// Icon for the Play button.
    pub play: Handle<Image>,
    /// Icon for the Pause button.
    pub pause: Handle<Image>,
    /// Icon for the Snap toggle.
    pub snap: Handle<Image>,
    /// Icon for the Delete action.
    pub delete: Handle<Image>,
    /// Icon for Properties/Settings.
    pub settings: Handle<Image>,
    /// Icon for Freeze/Unfreeze action.
    pub freeze: Handle<Image>,
}

/// Plugin that loads game icons at startup.
pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameIcons>();
        app.add_systems(Startup, load_icons);
    }
}

fn load_icons(mut icons: ResMut<GameIcons>, asset_server: Res<AssetServer>) {
    icons.select = asset_server.load("icons/arrow.png");
    icons.drag = asset_server.load("icons/move (3).png");
    icons.box_tool = asset_server.load("icons/box (2).png");
    icons.circle_tool = asset_server.load("icons/circle (2).png");
    icons.polygon_tool = asset_server.load("icons/polygon.png");
    icons.revolute_joint = asset_server.load("icons/hinge (3).png");
    icons.weld = asset_server.load("icons/fixjoint (2).png");
    // Placeholder for spring tool
    icons.spring = asset_server.load("icons/hinge (3).png");
    // Placeholder for ground tool
    icons.ground = asset_server.load("icons/box (2).png");

    icons.play = asset_server.load("icons/play.png");
    icons.pause = asset_server.load("icons/pause.png");
    icons.snap = asset_server.load("icons/dock_point.png");

    icons.delete = asset_server.load("icons/erase (2).png");
    icons.settings = asset_server.load("icons/settings.png");
    icons.freeze = asset_server.load("icons/locked.png");
}
