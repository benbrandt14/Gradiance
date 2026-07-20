//! The **object tree** (outliner): a Blender-style scene list of every authored
//! entity, grouped by kind — Bodies, Joints, Behavior nodes. Each row selects
//! its entity, and selection is **universal**: a row reflects (and drives) the
//! same [`Selection`]/[`SelectedJoint`] the viewport picking and the node-graph
//! canvas use, because they are the *same ECS entity*. Selecting in one view
//! highlights it in all three.
//!
//! Selection goes through the sanctioned [`SelectTransition`] seam (group
//! expansion + the joint-xor-bodies invariant), so the tree extends the
//! selection vocabulary instead of poking the resources. The pane renderer holds
//! no ECS queries: the host prebuilds an `OutlinerModel` snapshot and applies
//! the recorded click afterward — the same prepare/render/apply shape the
//! node-graph pane uses.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_core::ids::StableId;
use gradiance_domain::Body;
use gradiance_domain::Joint;
use gradiance_domain::group::SelectionGroup;
use gradiance_domain::joint::{JointDef, JointKind};
use gradiance_domain::node::{BehaviorNode, NodeKind};
use gradiance_domain::shape::ShapeDef;
use gradiance_interaction::selection::{SelectTransition, SelectedJoint, Selection};

/// Outliner visibility. Editor view-state; never persisted.
#[derive(Resource, Default)]
pub struct ObjectTreePanel {
    open: bool,
}

impl ObjectTreePanel {
    /// Whether the outliner is shown (read by the toolbar / menu toggles).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Flips the outliner's visibility.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// The outliner's ECS read set plus the selection-mutation handles, bundled as
/// one [`SystemParam`] so the dock host stays under Bevy's parameter cap.
#[derive(SystemParam)]
pub struct OutlinerParams<'w, 's> {
    pub(crate) panel: Res<'w, ObjectTreePanel>,
    pub(crate) bodies: Query<'w, 's, (Entity, &'static StableId, &'static ShapeDef), With<Body>>,
    pub(crate) joints: Query<'w, 's, (Entity, &'static StableId, &'static JointDef), With<Joint>>,
    pub(crate) nodes:
        Query<'w, 's, (Entity, &'static StableId, &'static NodeKind), With<BehaviorNode>>,
    pub(crate) selected_joint: ResMut<'w, SelectedJoint>,
    pub(crate) groups: Query<'w, 's, (Entity, &'static SelectionGroup), With<Body>>,
}

/// One row of the outliner — an entity with a display label and whether it's
/// currently selected. `is_joint` routes the click to the right selection
/// transition (joints select xor bodies).
pub(crate) struct OutlinerRow {
    entity: Entity,
    label: String,
    selected: bool,
    is_joint: bool,
}

/// The outliner's display snapshot, prebuilt by the host each frame so the pane
/// renderer holds no live queries.
#[derive(Default)]
pub(crate) struct OutlinerModel {
    bodies: Vec<OutlinerRow>,
    joints: Vec<OutlinerRow>,
    nodes: Vec<OutlinerRow>,
}

impl OutlinerModel {
    fn is_empty(&self) -> bool {
        self.bodies.is_empty() && self.joints.is_empty() && self.nodes.is_empty()
    }
}

/// A row click the pane recorded this frame, applied by the host via
/// [`apply_click`]. `toggle` = shift was held (additive/toggle selection).
pub(crate) struct OutlinerClick {
    entity: Entity,
    is_joint: bool,
    toggle: bool,
}

/// A short, stable id suffix so same-kind rows are distinguishable.
fn short_id(id: &StableId) -> String {
    id.to_string().chars().take(6).collect()
}

fn joint_label(kind: &JointKind) -> &'static str {
    match kind {
        JointKind::Hinge { .. } => "hinge",
        JointKind::Slider { .. } => "prismatic",
        JointKind::Spring { .. } => "spring",
    }
}

fn node_label(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Tracer(_) => "tracer",
    }
}

/// Builds the outliner snapshot from the scene, marking rows selected against
/// the current [`Selection`] / [`SelectedJoint`]. Rows are sorted by label then
/// entity so the tree order is stable frame-to-frame.
pub(crate) fn build_model(op: &OutlinerParams, selection: &Selection) -> OutlinerModel {
    let mut bodies: Vec<OutlinerRow> = op
        .bodies
        .iter()
        .map(|(entity, id, shape)| OutlinerRow {
            entity,
            label: format!("{} · {}", shape.kind_label(), short_id(id)),
            selected: selection.contains(entity),
            is_joint: false,
        })
        .collect();
    let mut joints: Vec<OutlinerRow> = op
        .joints
        .iter()
        .map(|(entity, id, def)| OutlinerRow {
            entity,
            label: format!("{} · {}", joint_label(&def.kind), short_id(id)),
            selected: op.selected_joint.0 == Some(entity),
            is_joint: true,
        })
        .collect();
    let mut nodes: Vec<OutlinerRow> = op
        .nodes
        .iter()
        .map(|(entity, id, kind)| OutlinerRow {
            entity,
            label: format!("{} · {}", node_label(kind), short_id(id)),
            selected: selection.contains(entity),
            is_joint: false,
        })
        .collect();
    let by_label_then_entity =
        |a: &OutlinerRow, b: &OutlinerRow| a.label.cmp(&b.label).then(a.entity.cmp(&b.entity));
    bodies.sort_by(by_label_then_entity);
    joints.sort_by(by_label_then_entity);
    nodes.sort_by(by_label_then_entity);
    OutlinerModel {
        bodies,
        joints,
        nodes,
    }
}

/// Renders the outliner into `ui` (its dock pane): three collapsible groups,
/// each row a selectable label. A click is recorded into `click` (shift =
/// toggle) and applied by the host.
pub(crate) fn outliner_section(
    ui: &mut egui::Ui,
    model: &OutlinerModel,
    click: &mut Option<OutlinerClick>,
) {
    if model.is_empty() {
        ui.weak("Empty scene — nothing to list yet.");
        return;
    }
    let shift = ui.input(|i| i.modifiers.shift);
    group(ui, "Bodies", &model.bodies, shift, click);
    group(ui, "Joints", &model.joints, shift, click);
    group(ui, "Behavior", &model.nodes, shift, click);
}

/// One collapsible, always-open group of rows. Empty groups are skipped.
fn group(
    ui: &mut egui::Ui,
    header: &str,
    rows: &[OutlinerRow],
    shift: bool,
    click: &mut Option<OutlinerClick>,
) {
    if rows.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("{header} ({})", rows.len()))
        .default_open(true)
        .show(ui, |ui| {
            for row in rows {
                if ui.selectable_label(row.selected, &row.label).clicked() {
                    *click = Some(OutlinerClick {
                        entity: row.entity,
                        is_joint: row.is_joint,
                        toggle: shift,
                    });
                }
            }
        });
}

/// Applies a recorded outliner click through the [`SelectTransition`] seam:
/// a joint row selects the joint (clearing bodies); a body/node row sets or
/// toggles (shift) the body selection — group-expanded, joint-clearing.
pub(crate) fn apply_click(
    click: OutlinerClick,
    selection: &mut Selection,
    op: &mut OutlinerParams,
) {
    let transition = if click.is_joint {
        SelectTransition::SelectJoint(click.entity)
    } else if click.toggle {
        SelectTransition::ToggleBodies(vec![click.entity])
    } else {
        SelectTransition::SetBodies(vec![click.entity])
    };
    transition.apply(selection, &mut op.selected_joint, &op.groups);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_id_is_a_stable_six_char_prefix() {
        let id = StableId::new();
        let short = short_id(&id);
        assert_eq!(short.chars().count(), 6);
        assert!(id.to_string().starts_with(&short));
        // Same id → same suffix (stable across frames).
        assert_eq!(short_id(&id), short);
    }

    #[test]
    fn rows_sort_by_label_then_entity() {
        // Same label → tie-broken by entity, so order never jitters.
        let mut rows = [
            OutlinerRow {
                entity: Entity::from_raw_u32(7).unwrap(),
                label: "box · aaa".to_owned(),
                selected: false,
                is_joint: false,
            },
            OutlinerRow {
                entity: Entity::from_raw_u32(3).unwrap(),
                label: "box · aaa".to_owned(),
                selected: false,
                is_joint: false,
            },
            OutlinerRow {
                entity: Entity::from_raw_u32(1).unwrap(),
                label: "circle · bbb".to_owned(),
                selected: false,
                is_joint: false,
            },
        ];
        rows.sort_by(|a, b| a.label.cmp(&b.label).then(a.entity.cmp(&b.entity)));
        let order: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(order, ["box · aaa", "box · aaa", "circle · bbb"]);
        // The two equal-label rows are ordered by ascending entity.
        assert!(rows[0].entity < rows[1].entity);
    }
}
