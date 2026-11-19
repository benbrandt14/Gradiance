//! Keyboard shortcuts via leafwing-input-manager.
//!
//! Discrete editor commands only — continuous input (camera drag, tool
//! gestures) reads raw input/picking directly. All world mutation still
//! flows through intents.
//!
//! A key that edits the scene invokes a registry operation by id
//! ([`KEY_OPS`], the key link table of plan § 13.4), as a menu row does; the
//! operation names `"selection"`, which resolves against the selection
//! mirror this layer publishes. The leafwing `InputMap` and [`EditorAction`]
//! are unchanged (keys are out of scope).

use crate::selection::{SelectTransition, Selection};
use bevy::prelude::*;
use gradiance_core::states::ToolState;
use gradiance_domain::Body;
use gradiance_op::{InvokeOp, Value, json};
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
    map.insert(A::Tool(ToolState::Spring), KeyCode::KeyT);
    map.insert(A::Tool(ToolState::Rod), KeyCode::KeyL);
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

/// One row of the key link table: pressing `action` invokes the operation
/// `id` with the params `params` builds.
#[derive(Debug, Clone, Copy)]
pub struct KeyOp {
    /// The key binding.
    pub action: EditorAction,
    /// The operation's registry id.
    pub id: &'static str,
    /// Builds the named params (a JSON object). A function, not JSON text, so
    /// a row cannot fail to parse at run time.
    pub params: fn() -> Value,
}

/// The key link table (plan § 13.4): each editing [`EditorAction`] and the
/// operation it invokes. A conformance test checks every id is registered and
/// every row lowers. The non-edit actions (pause, selection, files, tools)
/// are handled below and get operations in U-2b and U-3.
///
/// Delete has two rows: a selected joint and selected bodies are never
/// selected together, so exactly one of them has something to delete (a
/// selected joint takes the key, as before).
pub const KEY_OPS: &[KeyOp] = &[
    KeyOp {
        action: EditorAction::Undo,
        id: "edit.undo",
        params: || json!({}),
    },
    KeyOp {
        action: EditorAction::Redo,
        id: "edit.redo",
        params: || json!({}),
    },
    KeyOp {
        action: EditorAction::DeleteSelection,
        id: "joint.delete",
        params: || json!({"i": "selection"}),
    },
    KeyOp {
        action: EditorAction::DeleteSelection,
        id: "body.delete",
        params: || json!({"i": "selection"}),
    },
    KeyOp {
        action: EditorAction::Group,
        id: "group.create",
        params: || json!({"targets": ["selection"]}),
    },
    KeyOp {
        action: EditorAction::Ungroup,
        id: "group.ungroup",
        params: || json!({"targets": ["selection"]}),
    },
];

impl KeyOp {
    /// The invocation this row makes.
    pub fn invoke(&self) -> InvokeOp {
        InvokeOp::with(self.id, (self.params)())
    }
}

/// The invocations `action` makes, from [`KEY_OPS`]. The menus that repeat a
/// key's edit (Edit ▸ Redo, Delete, Group, Ungroup) write these too, so a key
/// and its menu row cannot differ.
pub fn key_invocations(action: EditorAction) -> impl Iterator<Item = InvokeOp> {
    KEY_OPS
        .iter()
        .filter(move |row| row.action == action)
        .map(KeyOp::invoke)
}

/// All the outgoing messages shortcuts can emit (grouped to stay under
/// the system-parameter limit).
#[derive(bevy::ecs::system::SystemParam)]
pub struct ShortcutWriters<'w> {
    /// Edits, by operation id ([`KEY_OPS`]).
    invoke: MessageWriter<'w, InvokeOp>,
    save: MessageWriter<'w, gradiance_persist::SaveSceneRequest>,
    transport: MessageWriter<'w, crate::transport::SimTransport>,
    snapshot: MessageWriter<'w, gradiance_persist::SnapshotRequest>,
}

/// Translates just-pressed actions into operation invocations / state
/// changes.
pub fn apply_shortcuts(
    actions: Query<&ActionState<EditorAction>, With<EditorControls>>,
    keys: Res<ButtonInput<KeyCode>>,
    keyboard_captured: Res<crate::KeyboardCaptured>,
    field_keys: Res<crate::FieldKeys>,
    mut writers: ShortcutWriters,
    mut selection: ResMut<Selection>,
    mut selected_joint: ResMut<crate::selection::SelectedJoint>,
    groups: Query<(Entity, &gradiance_domain::group::SelectionGroup), With<Body>>,
    bodies: Query<Entity, With<Body>>,
    mut next_tool: ResMut<NextState<ToolState>>,
    mut scale_frame: ResMut<crate::tools::handles::ScaleFrame>,
    mut last_path: ResMut<gradiance_persist::LastScenePath>,
) {
    // Typing in a UI field must never trigger editor shortcuts. A number
    // field under the pointer owns its own keys, but not the editor's Ctrl
    // chords (it takes only Ctrl+C and Ctrl+V, which no action uses).
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let on_field = keyboard_captured.0 && field_keys.0;
    if keyboard_captured.0 && !(on_field && ctrl) {
        return;
    }
    let Ok(actions) = actions.single() else {
        return;
    };

    // The edits: one table, rows in order. Ungroup's chord (Ctrl+Shift+G)
    // also satisfies Group's, so Group yields to it. Delete and Backspace
    // stay the field's.
    let ungrouping = actions.just_pressed(&EditorAction::Ungroup);
    for row in KEY_OPS {
        let shadowed = (row.action == EditorAction::Group && ungrouping)
            || (on_field && row.action == EditorAction::DeleteSelection);
        if actions.just_pressed(&row.action) && !shadowed {
            writers.invoke.write(row.invoke());
        }
    }
    if actions.just_pressed(&EditorAction::TogglePause) {
        // The transport's one path: `sim.play` / `sim.pause` write the same
        // request (brief R-1).
        writers.transport.write(crate::transport::SimTransport::Toggle);
    }
    if actions.just_pressed(&EditorAction::SelectAll) {
        let all = bodies.iter().collect();
        SelectTransition::SetBodies(all).apply(&mut selection, &mut selected_joint, &groups);
    }
    if actions.just_pressed(&EditorAction::Deselect) {
        SelectTransition::Clear.apply(&mut selection, &mut selected_joint, &groups);
    }
    if actions.just_pressed(&EditorAction::Save) {
        // An empty path: the remembered one, else the dialog.
        writers
            .invoke
            .write(InvokeOp::with("scene.save", json!({"path": ""})));
    }
    if actions.just_pressed(&EditorAction::SaveAs) {
        last_path.0 = None; // force the dialog
        writers
            .save
            .write(gradiance_persist::SaveSceneRequest { path: None });
    }
    if actions.just_pressed(&EditorAction::Open) {
        // An empty path: the dialog.
        writers
            .invoke
            .write(InvokeOp::with("scene.load", json!({"path": ""})));
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
    if ctrl {
        return;
    }
    for action in actions.get_just_pressed() {
        if let EditorAction::Tool(tool) = action {
            next_tool.set(tool);
        }
    }
}
