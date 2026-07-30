//! Keyboard shortcuts via leafwing-input-manager.
//!
//! Discrete editor commands only — continuous input (camera drag, tool
//! gestures) reads raw input/picking directly. All world mutation still
//! flows through intents.

use crate::selection::{SelectTransition, Selection};
use bevy::prelude::*;
use gradiance_command::intent::{DeleteIntent, GroupIntent, RedoIntent, UndoIntent, UngroupIntent};
use gradiance_core::ids::StableId;
use gradiance_core::states::{GameState, ToolState};
use gradiance_domain::Body;
use leafwing_input_manager::prelude::*;

/// Discrete editor actions with rebindable inputs.
#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum EditorAction {
    /// Undo the last command (Ctrl+Z).
    Undo,
    /// Redo (Ctrl+Shift+Z / Ctrl+Y).
    Redo,
    /// Delete the selection (Delete / Backspace).
    DeleteSelection,
    /// Toggle play/pause (Space).
    TogglePause,
    /// Select all bodies (Ctrl+A).
    SelectAll,
    /// Clear the selection (Escape).
    Deselect,
    /// Toggle the scale-handle frame between global and local axes (F).
    ToggleScaleFrame,
    /// Save the scene (Ctrl+S; remembered path or dialog).
    Save,
    /// Save the scene to a new file (Ctrl+Shift+S).
    SaveAs,
    /// Open a scene (Ctrl+O).
    Open,
    /// Dump a timestamped debug snapshot (F12).
    Snapshot,
    /// Group the selection (Ctrl+G).
    Group,
    /// Ungroup the selection (Ctrl+Shift+G).
    Ungroup,
    /// Switch to the given tool (see `default_input_map` for the keys).
    /// Carrying the [`ToolState`] directly means a new tool adds one key
    /// binding — no parallel variant, match arm, or tool list to update.
    Tool(ToolState),
}

/// Default key bindings.
fn default_input_map() -> InputMap<EditorAction> {
    use EditorAction as A;
    let mut map = InputMap::default();
    map.insert(A::Undo, ModifierKey::Control.with(KeyCode::KeyZ));
    map.insert(
        A::Redo,
        ButtonlikeChord::from_single(ModifierKey::Control)
            .with(ModifierKey::Shift)
            .with(KeyCode::KeyZ),
    );
    map.insert(A::Redo, ModifierKey::Control.with(KeyCode::KeyY));
    map.insert(A::DeleteSelection, KeyCode::Delete);
    map.insert(A::DeleteSelection, KeyCode::Backspace);
    map.insert(A::TogglePause, KeyCode::Space);
    map.insert(A::SelectAll, ModifierKey::Control.with(KeyCode::KeyA));
    map.insert(A::Deselect, KeyCode::Escape);
    map.insert(A::ToggleScaleFrame, KeyCode::KeyF);
    map.insert(A::Save, ModifierKey::Control.with(KeyCode::KeyS));
    map.insert(
        A::SaveAs,
        ButtonlikeChord::from_single(ModifierKey::Control)
            .with(ModifierKey::Shift)
            .with(KeyCode::KeyS),
    );
    map.insert(A::Open, ModifierKey::Control.with(KeyCode::KeyO));
    map.insert(A::Snapshot, KeyCode::F12);
    map.insert(A::Group, ModifierKey::Control.with(KeyCode::KeyG));
    map.insert(
        A::Ungroup,
        ButtonlikeChord::from_single(ModifierKey::Control)
            .with(ModifierKey::Shift)
            .with(KeyCode::KeyG),
    );
    map.insert(A::Tool(ToolState::Select), KeyCode::KeyS);
    map.insert(A::Tool(ToolState::Drag), KeyCode::KeyD);
    map.insert(A::Tool(ToolState::Box), KeyCode::KeyB);
    map.insert(A::Tool(ToolState::Circle), KeyCode::KeyC);
    map.insert(A::Tool(ToolState::Polygon), KeyCode::KeyP);
    map.insert(A::Tool(ToolState::Hinge), KeyCode::KeyH);
    map.insert(A::Tool(ToolState::Weld), KeyCode::KeyW);
    map.insert(A::Tool(ToolState::Slider), KeyCode::KeyR);
    map.insert(A::Tool(ToolState::Strut), KeyCode::KeyT);
    map.insert(A::Tool(ToolState::Ground), KeyCode::KeyG);
    map.insert(A::Tool(ToolState::Cut), KeyCode::KeyK);
    // `N` for node — a tracer is a `BehaviorNode`. The tool palette had
    // advertised `Y` for it, but `Y` is Redo and Tracer had no binding at all,
    // so the hint sent people to a different action entirely.
    map.insert(A::Tool(ToolState::Tracer), KeyCode::KeyN);
    map
}

/// Marker for the singleton entity carrying the editor `InputMap`.
#[derive(Component)]
pub struct EditorControls;

/// Installs the input manager and spawns the editor controls entity.
pub struct EditorInputPlugin;

impl Plugin for EditorInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<EditorAction>::default());
        app.world_mut().spawn((EditorControls, default_input_map()));
    }
}

/// All the outgoing messages shortcuts can emit (grouped to stay under
/// the system-parameter limit).
#[derive(bevy::ecs::system::SystemParam)]
pub struct ShortcutWriters<'w> {
    undo: MessageWriter<'w, UndoIntent>,
    redo: MessageWriter<'w, RedoIntent>,
    delete: MessageWriter<'w, DeleteIntent>,
    group: MessageWriter<'w, GroupIntent>,
    ungroup: MessageWriter<'w, UngroupIntent>,
    delete_joint: MessageWriter<'w, gradiance_command::intent::DeleteJointIntent>,
    save: MessageWriter<'w, gradiance_persist::SaveSceneRequest>,
    load: MessageWriter<'w, gradiance_persist::LoadSceneRequest>,
    snapshot: MessageWriter<'w, gradiance_persist::SnapshotRequest>,
}

/// Translates just-pressed actions into intents / state changes.
pub fn apply_shortcuts(
    actions: Query<&ActionState<EditorAction>, With<EditorControls>>,
    keys: Res<ButtonInput<KeyCode>>,
    keyboard_captured: Res<crate::KeyboardCaptured>,
    mut writers: ShortcutWriters,
    mut selection: ResMut<Selection>,
    mut selected_joint: ResMut<crate::selection::SelectedJoint>,
    groups: Query<(Entity, &gradiance_domain::group::SelectionGroup), With<Body>>,
    ids: Query<&StableId>,
    bodies: Query<Entity, With<Body>>,
    game_state: Res<State<GameState>>,
    mut next_game: ResMut<NextState<GameState>>,
    mut next_tool: ResMut<NextState<ToolState>>,
    mut scale_frame: ResMut<crate::tools::handles::ScaleFrame>,
    mut last_path: ResMut<gradiance_persist::LastScenePath>,
) {
    // Typing in a UI field must never trigger editor shortcuts.
    if keyboard_captured.0 {
        return;
    }
    let Ok(actions) = actions.single() else {
        return;
    };
    let selected_ids = |selection: &Selection| -> Vec<StableId> {
        selection
            .iter()
            .filter_map(|e| ids.get(e).ok().copied())
            .collect()
    };

    if actions.just_pressed(&EditorAction::Undo) {
        writers.undo.write(UndoIntent);
    }
    if actions.just_pressed(&EditorAction::Redo) {
        writers.redo.write(RedoIntent);
    }
    if actions.just_pressed(&EditorAction::DeleteSelection) {
        // A selected joint takes delete priority (it consumed selection).
        if let Some(joint) = selected_joint.0
            && let Ok(&id) = ids.get(joint)
        {
            writers
                .delete_joint
                .write(gradiance_command::intent::DeleteJointIntent { id });
        } else if !selection.is_empty() {
            let targets = selected_ids(&selection);
            if !targets.is_empty() {
                writers.delete.write(DeleteIntent { targets });
            }
        }
    }
    // Ungroup first: its chord (Ctrl+Shift+G) also satisfies Group's.
    if actions.just_pressed(&EditorAction::Ungroup) {
        let targets = selected_ids(&selection);
        if !targets.is_empty() {
            writers.ungroup.write(UngroupIntent { targets });
        }
    } else if actions.just_pressed(&EditorAction::Group) {
        let targets = selected_ids(&selection);
        if targets.len() >= 2 {
            writers.group.write(GroupIntent { targets });
        }
    }
    if actions.just_pressed(&EditorAction::TogglePause) {
        next_game.set(match game_state.get() {
            GameState::Playing => GameState::Paused,
            GameState::Paused => GameState::Playing,
        });
    }
    if actions.just_pressed(&EditorAction::SelectAll) {
        let all = bodies.iter().collect();
        SelectTransition::SetBodies(all).apply(&mut selection, &mut selected_joint, &groups);
    }
    if actions.just_pressed(&EditorAction::Deselect) {
        SelectTransition::Clear.apply(&mut selection, &mut selected_joint, &groups);
    }
    if actions.just_pressed(&EditorAction::Save) {
        writers
            .save
            .write(gradiance_persist::SaveSceneRequest { path: None });
    }
    if actions.just_pressed(&EditorAction::SaveAs) {
        last_path.0 = None; // force the dialog
        writers
            .save
            .write(gradiance_persist::SaveSceneRequest { path: None });
    }
    if actions.just_pressed(&EditorAction::Open) {
        writers
            .load
            .write(gradiance_persist::LoadSceneRequest { path: None });
    }
    if actions.just_pressed(&EditorAction::Snapshot) {
        writers
            .snapshot
            .write(gradiance_persist::SnapshotRequest::default());
    }
    if actions.just_pressed(&EditorAction::ToggleScaleFrame) {
        use crate::tools::handles::ScaleFrame;
        *scale_frame = match *scale_frame {
            ScaleFrame::Global => ScaleFrame::Local,
            ScaleFrame::Local => ScaleFrame::Global,
        };
    }
    // Bare-letter hotkeys (tools, frame toggle) must not fire as part of
    // a Ctrl chord — Ctrl+S is "save", not "save and switch to select".
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if ctrl {
        return;
    }
    for action in actions.get_just_pressed() {
        if let EditorAction::Tool(tool) = action {
            next_tool.set(tool);
        }
    }
}
