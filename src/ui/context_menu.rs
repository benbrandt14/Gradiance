//! Right-click context menu: grouping, pick-from-stack, and collision
//! layer operations.

use crate::command::intent::{GroupIntent, PropertyEditIntent, UngroupIntent};
use crate::command::property::{PropertyChange, PropertyValue};
use crate::core::ids::{IdIndex, StableId};
use crate::domain::Body;
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use crate::interaction::PointerOverUi;
use crate::interaction::cursor::CursorWorldPos;
use crate::interaction::pointer::PointerButtons;
use crate::interaction::selection::Selection;
use crate::interaction::tools::{ActiveGesture, bodies_at_sorted};
use crate::physics::queries::PhysicsQueries;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

/// Open context menu state.
#[derive(Resource, Default, Debug)]
pub struct ContextMenu {
    /// Whether the menu is showing.
    pub open: bool,
    /// Screen position (logical px) to anchor at.
    pub screen: Vec2,
    /// Bodies under the click, topmost first.
    pub under: Vec<StableId>,
}

/// Opens the menu on a right *click* (release without drag, no rotate
/// gesture in progress).
pub fn open_context_menu(
    buttons: Res<PointerButtons>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cursor: Res<CursorWorldPos>,
    over_ui: Res<PointerOverUi>,
    active: Res<ActiveGesture>,
    physics: PhysicsQueries,
    bodies: Query<(&ShapeDef, &LayerMask32), With<Body>>,
    ids: Query<&StableId>,
    mut press_pos: Local<Option<Vec2>>,
    mut menu: ResMut<ContextMenu>,
) {
    let screen = windows.iter().next().and_then(Window::cursor_position);
    if buttons.just_pressed(MouseButton::Right) && !over_ui.0 && !active.0 {
        *press_pos = screen;
    }
    if buttons.just_released(MouseButton::Right)
        && !active.0
        && let (Some(start), Some(now)) = (press_pos.take(), screen)
        && start.distance(now) < 4.0
        && let Some(world) = cursor.0
    {
        {
            menu.under = bodies_at_sorted(world, &physics, &bodies)
                .into_iter()
                .filter_map(|e| ids.get(e).ok().copied())
                .collect();
            menu.screen = now;
            menu.open = true;
        }
    }
}

/// Renders the menu and emits intents for its actions.
pub fn context_menu(
    mut contexts: EguiContexts,
    mut menu: ResMut<ContextMenu>,
    mut selection: ResMut<Selection>,
    index: Res<IdIndex>,
    ids: Query<&StableId>,
    layers_q: Query<&LayerMask32, With<Body>>,
    all_layers: Query<&LayerMask32, With<Body>>,
    mut group: MessageWriter<GroupIntent>,
    mut ungroup: MessageWriter<UngroupIntent>,
    mut edits: MessageWriter<PropertyEditIntent>,
) -> Result {
    if !menu.open {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let selected_ids: Vec<StableId> = selection
        .iter()
        .filter_map(|e| ids.get(e).ok().copied())
        .collect();

    let mut close = false;
    let response = egui::Area::new(egui::Id::new("context-menu"))
        .fixed_pos(egui::pos2(menu.screen.x, menu.screen.y))
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);

                if ui
                    .add_enabled(selected_ids.len() >= 2, egui::Button::new("Group"))
                    .clicked()
                {
                    group.write(GroupIntent {
                        targets: selected_ids.clone(),
                    });
                    close = true;
                }
                if ui
                    .add_enabled(!selected_ids.is_empty(), egui::Button::new("Ungroup"))
                    .clicked()
                {
                    ungroup.write(UngroupIntent {
                        targets: selected_ids.clone(),
                    });
                    close = true;
                }
                ui.separator();

                // Pick from overlapping bodies under the click.
                if !menu.under.is_empty() {
                    ui.label(egui::RichText::new("Select from").weak());
                    for (i, id) in menu.under.clone().into_iter().enumerate() {
                        if ui.button(format!("· body {i} ({id:.8})")).clicked() {
                            if let Some(entity) = index.entity(id) {
                                selection.set(entity);
                            }
                            close = true;
                        }
                    }
                    ui.separator();
                }

                // Layer operations on the selection.
                ui.label(egui::RichText::new("Layers").weak());
                ui.horizontal_wrapped(|ui| {
                    for bit in 0..8u32 {
                        if ui.small_button(format!("{bit}")).clicked() {
                            layer_edit(&selection, &ids, &layers_q, &mut edits, |old| {
                                LayerMask32 {
                                    memberships: 1 << bit,
                                    filters: old.filters,
                                }
                            });
                            close = true;
                        }
                    }
                });
                if ui.button("Isolate collisions within selection").clicked() {
                    // Move the selection to a free layer bit that ignores
                    // itself: members stop colliding with each other but
                    // still collide with everything else.
                    let used: u32 = all_layers
                        .iter()
                        .map(|l| l.memberships)
                        .fold(0, |a, b| a | b);
                    let free = (0..32u32).find(|b| used & (1 << b) == 0).unwrap_or(31);
                    layer_edit(&selection, &ids, &layers_q, &mut edits, |_| LayerMask32 {
                        memberships: 1 << free,
                        filters: !(1 << free),
                    });
                    close = true;
                }
                if ui.button("Reset layers (self-collide on)").clicked() {
                    layer_edit(&selection, &ids, &layers_q, &mut edits, |_| {
                        LayerMask32::default()
                    });
                    close = true;
                }
            });
        })
        .response;

    if close || response.clicked_elsewhere() {
        menu.open = false;
    }
    Ok(())
}

fn layer_edit(
    selection: &Selection,
    ids: &Query<&StableId>,
    layers: &Query<&LayerMask32, With<Body>>,
    edits: &mut MessageWriter<PropertyEditIntent>,
    make_new: impl Fn(&LayerMask32) -> LayerMask32,
) {
    let changes: Vec<PropertyChange> = selection
        .iter()
        .filter_map(|e| {
            let id = ids.get(e).ok().copied()?;
            let old = *layers.get(e).ok()?;
            Some(PropertyChange {
                id,
                old: PropertyValue::Layers(old),
                new: PropertyValue::Layers(make_new(&old)),
            })
        })
        .collect();
    if !changes.is_empty() {
        edits.write(PropertyEditIntent { changes });
    }
}
