//! Click-through selection: with *any* tool active, a plain click that no
//! tool consumed selects the body under the cursor (Algodoo behavior —
//! feedback 2.1: "cleanly defaulted to selections even when the select tool
//! was not active").
//!
//! "Consumed" is decided by evidence, not tool-specific knowledge: the click
//! falls through only if the release ends a sub-deadzone gesture, the active
//! tool is not mid-draft (a polygon between vertex clicks must keep its
//! clicks), and no authoring intent (`SpawnBody`/`Cut`/`SpawnJoint`) was
//! emitted this frame. Selection is applied through [`SelectTransition`] —
//! the same seam the select tool uses — so the joint-xor-bodies invariant
//! and group expansion hold unchanged.

use crate::PointerOverUi;
use crate::joint_edit::SuppressSelectPress;
use crate::pointer::PointerButtons;
use crate::selection::{SelectTransition, SelectedJoint, Selection};
use crate::snap::SnappedCursor;
use crate::tools::ActiveGesture;
use crate::tools::context::ToolWorld;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use gradiance_command::intent::{CutIntent, SpawnBodyIntent, SpawnJointIntent};
use gradiance_core::states::ToolState;
use gradiance_domain::Body;
use gradiance_domain::group::SelectionGroup;

/// Screen-space distance (logical px) a press may travel and still count as
/// a click.
const CLICK_DEADZONE_PX: f32 = 4.0;

/// Tracks the in-flight left press for the click-through test.
#[derive(Resource, Default, Debug)]
pub struct ClickThrough {
    /// Raw world cursor at press (`None` when no canvas press is in flight).
    press: Option<Vec2>,
    /// Whether the cursor left the click deadzone since the press.
    moved: bool,
}

/// This frame's authoring-intent evidence: if any tool committed, the click
/// belonged to that tool. Readers are drained every frame so the release
/// frame observes exactly the intents written this frame.
#[derive(SystemParam)]
pub struct CommitEvidence<'w, 's> {
    spawns: MessageReader<'w, 's, SpawnBodyIntent>,
    cuts: MessageReader<'w, 's, CutIntent>,
    joints: MessageReader<'w, 's, SpawnJointIntent>,
}

impl CommitEvidence<'_, '_> {
    fn any_this_frame(&mut self) -> bool {
        self.spawns.read().count() + self.cuts.read().count() + self.joints.read().count() > 0
    }
}

/// Update system (after the tool drivers): applies the click-through
/// selection on a release that no tool consumed. Runs in every tool state so
/// the intent readers stay fresh; the select tool state is exempt from the
/// *application* (its own gesture machine owns selection there).
pub fn click_through_select(
    buttons: Res<PointerButtons>,
    keys: Res<ButtonInput<KeyCode>>,
    over_ui: Res<PointerOverUi>,
    suppress: Res<SuppressSelectPress>,
    snapped: Res<SnappedCursor>,
    active: Res<ActiveGesture>,
    tool_state: Res<State<ToolState>>,
    cam_scale: Res<crate::camera::CameraScale>,
    world: ToolWorld,
    mut track: ResMut<ClickThrough>,
    mut evidence: CommitEvidence,
    mut selection: ResMut<Selection>,
    mut joint_sel: ResMut<SelectedJoint>,
    groups: Query<(Entity, &SelectionGroup), With<Body>>,
) {
    let committed = evidence.any_this_frame();

    if buttons.just_pressed(MouseButton::Left) {
        track.press = (!over_ui.0 && !suppress.0).then_some(snapped.raw).flatten();
        track.moved = false;
    } else if buttons.pressed(MouseButton::Left)
        && let (Some(start), Some(p)) = (track.press, snapped.raw)
    {
        let cam_scale = cam_scale.0;
        if start.distance(p) > CLICK_DEADZONE_PX * cam_scale {
            track.moved = true;
        }
    }

    if !buttons.just_released(MouseButton::Left) {
        return;
    }
    let Some(press) = track.press.take() else {
        return;
    };
    // The select tool's own gesture machine owns selection in Select mode.
    if *tool_state.get() == ToolState::Select {
        return;
    }
    // Evidence the click belonged to the active tool: it moved, a draft is
    // still in progress (polygon vertices, connector endpoints), or a commit
    // intent was written this frame.
    if track.moved || active.0 || committed {
        return;
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let transition = match world.topmost_body_at(press) {
        Some(hit) => {
            if shift {
                SelectTransition::ToggleBodies(vec![hit])
            } else {
                SelectTransition::SetBodies(vec![hit])
            }
        }
        // Shift-click on empty space keeps the selection (additive habit);
        // a plain empty click clears it, like the select tool.
        None if shift => return,
        None => SelectTransition::Clear,
    };
    transition.apply(&mut selection, &mut joint_sel, &groups);
}
