//! Right-click context menu: grouping, pick-from-stack, and collision
//! layer operations.

use crate::command::intent::{
    CommitTransformIntent, GroupIntent, MergeIntent, PropertyEditIntent, UngroupIntent,
};
use crate::command::property::{PropertyChange, PropertyValue};
use crate::core::ids::{IdIndex, StableId};
use crate::domain::Body;
use crate::domain::appearance::{Appearance, Rgba};
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use crate::interaction::PointerOverUi;
use crate::interaction::align::{AlignItem, AlignOp, align_changes};
use crate::interaction::cursor::CursorWorldPos;
use crate::interaction::pointer::PointerButtons;
use crate::interaction::selection::Selection;
use crate::interaction::tools::bodies_at_sorted;
use crate::physics::queries::PhysicsQueries;
use crate::script::bridge::{ScriptActions, ScriptInputs};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

/// Script-action access, bundled into one `SystemParam` to keep `context_menu`
/// under Bevy's system-parameter count limit. Reads the registered actions and
/// submits an invoked one's source through the shared `ScriptInputs` queue.
#[derive(SystemParam)]
pub struct ScriptMenu<'w> {
    actions: Res<'w, ScriptActions>,
    inputs: ResMut<'w, ScriptInputs>,
}

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

/// Opens the menu on a right *click* — a release within a small screen
/// deadzone of the press. Gestures that need right-drag (selection
/// rotate, camera pan) use the same deadzone, so a click reaches the
/// menu and a drag never does; no cross-system ordering is involved.
pub fn open_context_menu(
    buttons: Res<PointerButtons>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cursor: Res<CursorWorldPos>,
    over_ui: Res<PointerOverUi>,
    physics: PhysicsQueries,
    bodies: Query<(&ShapeDef, &LayerMask32), With<Body>>,
    ids: Query<&StableId>,
    mut press_pos: Local<Option<Vec2>>,
    mut menu: ResMut<ContextMenu>,
) {
    let screen = windows.iter().next().and_then(Window::cursor_position);
    if buttons.just_pressed(MouseButton::Right) && !over_ui.0 {
        *press_pos = screen;
    }
    if buttons.just_released(MouseButton::Right)
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
#[expect(clippy::too_many_lines)] // one menu, plain sections in order
pub fn context_menu(
    mut contexts: EguiContexts,
    mut menu: ResMut<ContextMenu>,
    mut selection: ResMut<Selection>,
    mut selected_joint: ResMut<crate::interaction::selection::SelectedJoint>,
    index: Res<IdIndex>,
    ids: Query<&StableId>,
    layers_q: Query<&LayerMask32, With<Body>>,
    all_layers: Query<&LayerMask32, With<Body>>,
    groups: Query<(Entity, &crate::domain::group::SelectionGroup), With<Body>>,
    bodies_q: Query<(&ShapeDef, &Transform, &Appearance), With<Body>>,
    mut group: MessageWriter<GroupIntent>,
    mut ungroup: MessageWriter<UngroupIntent>,
    mut edits: MessageWriter<PropertyEditIntent>,
    mut merge: MessageWriter<MergeIntent>,
    mut moves: MessageWriter<CommitTransformIntent>,
    mut script: ScriptMenu,
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
                if ui
                    .add_enabled(
                        selected_ids.len() >= 2,
                        egui::Button::new("Merge into one body"),
                    )
                    .clicked()
                {
                    merge.write(MergeIntent {
                        targets: selected_ids.clone(),
                    });
                    close = true;
                }
                ui.separator();

                // Align & distribute (one undo step via the move command).
                if selected_ids.len() >= 2 {
                    ui.label(egui::RichText::new("Align").weak());
                    let items: Vec<AlignItem> = selection
                        .iter()
                        .filter_map(|e| {
                            let id = ids.get(e).ok().copied()?;
                            let (shape, transform, _) = bodies_q.get(e).ok()?;
                            if shape.contains_half_plane() {
                                return None;
                            }
                            Some((
                                id,
                                crate::core::units::PosRot::from_transform(transform),
                                world_bounds(shape, transform),
                            ))
                        })
                        .collect();
                    let mut emit = |op: AlignOp| {
                        let changes = align_changes(&items, op);
                        if !changes.is_empty() {
                            moves.write(CommitTransformIntent { changes });
                        }
                        close = true;
                    };
                    ui.horizontal_wrapped(|ui| {
                        for (label, op) in [
                            ("⏴ left", AlignOp::Left),
                            ("right ⏵", AlignOp::Right),
                            ("⏶ top", AlignOp::Top),
                            ("bottom ⏷", AlignOp::Bottom),
                            ("center ↕", AlignOp::CenterY),
                            ("center ↔", AlignOp::CenterX),
                            ("distribute ↔", AlignOp::DistributeX),
                            ("distribute ↕", AlignOp::DistributeY),
                        ] {
                            if ui.small_button(label).clicked() {
                                emit(op);
                            }
                        }
                    });
                    ui.separator();
                }

                // Pick from overlapping bodies under the click.
                if !menu.under.is_empty() {
                    ui.label(egui::RichText::new("Select from").weak());
                    for (i, id) in menu.under.clone().into_iter().enumerate() {
                        if ui.button(format!("· body {i} ({id:.8})")).clicked() {
                            if let Some(entity) = index.entity(id) {
                                crate::interaction::selection::SelectTransition::SetBodies(vec![
                                    entity,
                                ])
                                .apply(
                                    &mut selection,
                                    &mut selected_joint,
                                    &groups,
                                );
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
                if ui.button("No self-collisions (within selection)").clicked() {
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
                if ui.button("Reset collision layers").clicked() {
                    layer_edit(&selection, &ids, &layers_q, &mut edits, |_| {
                        LayerMask32::default()
                    });
                    close = true;
                }
                ui.label(egui::RichText::new("Depth").weak());
                ui.horizontal_wrapped(|ui| {
                    // Depth = layer bits (bit 0 front … bit 31 back).
                    let shift = |edits: &mut MessageWriter<PropertyEditIntent>,
                                 f: &dyn Fn(&LayerMask32) -> u32| {
                        layer_edit(&selection, &ids, &layers_q, edits, |old| LayerMask32 {
                            memberships: f(old).max(1),
                            filters: old.filters,
                        });
                    };
                    if ui.small_button("to front").clicked() {
                        shift(&mut edits, &|_| 1);
                        close = true;
                    }
                    if ui.small_button("forward").clicked() {
                        shift(&mut edits, &|old| old.memberships >> 1);
                        close = true;
                    }
                    if ui.small_button("backward").clicked() {
                        shift(&mut edits, &|old| {
                            if old.memberships & (1 << 31) == 0 {
                                old.memberships << 1
                            } else {
                                old.memberships
                            }
                        });
                        close = true;
                    }
                    if ui.small_button("to back").clicked() {
                        shift(&mut edits, &|_| 1 << 7);
                        close = true;
                    }
                });
                ui.separator();
                if ui.button("Random colors (per body)").clicked() {
                    let changes: Vec<PropertyChange> = selection
                        .iter()
                        .enumerate()
                        .filter_map(|(index, e)| {
                            let id = ids.get(e).ok().copied()?;
                            let (_, _, old) = bodies_q.get(e).ok()?;
                            // Golden-angle hue walk from each body's id.
                            let base = (id.0.as_u128() % 360) as f32;
                            let hue = base + 137.5 * (index as f32 + 1.0);
                            let new = Appearance {
                                fill: Rgba::from_hsl(hue, 0.65, 0.55),
                                ..*old
                            };
                            Some(PropertyChange {
                                id,
                                old: PropertyValue::Appearance(*old),
                                new: PropertyValue::Appearance(new),
                            })
                        })
                        .collect();
                    if !changes.is_empty() {
                        edits.write(PropertyEditIntent { changes });
                    }
                    close = true;
                }

                // User-registered script actions (added from `.scm` via
                // `register-action`). Invoking one submits its source through
                // the same `ScriptInputs` seam a REPL line uses.
                if !script.actions.0.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new("Scripts").weak());
                    for action in &script.actions.0 {
                        if ui.button(&action.label).clicked() {
                            script.inputs.submit(action.source.clone());
                            close = true;
                        }
                    }
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

/// A body's conservative world-space AABB.
fn world_bounds(shape: &ShapeDef, transform: &Transform) -> (Vec2, Vec2) {
    let (min, max) = crate::geometry::sdf::aabb(shape);
    let affine = transform.compute_affine();
    let corners = [
        Vec2::new(min.x, min.y),
        Vec2::new(max.x, min.y),
        Vec2::new(max.x, max.y),
        Vec2::new(min.x, max.y),
    ];
    let mut wmin = Vec2::splat(f32::MAX);
    let mut wmax = Vec2::splat(f32::MIN);
    for corner in corners {
        let w = affine.transform_point3(corner.extend(0.0)).truncate();
        wmin = wmin.min(w);
        wmax = wmax.max(w);
    }
    (wmin, wmax)
}
