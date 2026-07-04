//! Keyboard shortcuts via leafwing-input-manager.
//!
//! Discrete editor commands only — continuous input (camera drag, tool
//! gestures) reads raw input/picking directly. All world mutation still
//! flows through intents.

use crate::command::intent::{DeleteIntent, RedoIntent, UndoIntent};
use crate::core::ids::StableId;
use crate::core::states::{GameState, ToolState};
use crate::domain::Body;
use crate::interaction::selection::Selection;
use bevy::prelude::*;
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
    /// Switch to the select tool (S).
    ToolSelect,
    /// Switch to the drag tool (D).
    ToolDrag,
    /// Switch to the box tool (B).
    ToolBox,
    /// Switch to the circle tool (C).
    ToolCircle,
    /// Switch to the polygon tool (P).
    ToolPolygon,
    /// Switch to the hinge tool (H).
    ToolHinge,
    /// Switch to the weld tool (W).
    ToolWeld,
    /// Switch to the slider ("rail") tool (R).
    ToolSlider,
    /// Switch to the ground tool (G).
    ToolGround,
    /// Switch to the cut tool (K).
    ToolCut,
}

impl EditorAction {
    fn tool(self) -> Option<ToolState> {
        match self {
            Self::ToolSelect => Some(ToolState::Select),
            Self::ToolDrag => Some(ToolState::Drag),
            Self::ToolBox => Some(ToolState::Box),
            Self::ToolCircle => Some(ToolState::Circle),
            Self::ToolPolygon => Some(ToolState::Polygon),
            Self::ToolHinge => Some(ToolState::Hinge),
            Self::ToolWeld => Some(ToolState::Weld),
            Self::ToolSlider => Some(ToolState::Slider),
            Self::ToolGround => Some(ToolState::Ground),
            Self::ToolCut => Some(ToolState::Cut),
            _ => None,
        }
    }

    const TOOLS: [Self; 10] = [
        Self::ToolSelect,
        Self::ToolDrag,
        Self::ToolBox,
        Self::ToolCircle,
        Self::ToolPolygon,
        Self::ToolHinge,
        Self::ToolWeld,
        Self::ToolSlider,
        Self::ToolGround,
        Self::ToolCut,
    ];
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
    map.insert(A::ToolSelect, KeyCode::KeyS);
    map.insert(A::ToolDrag, KeyCode::KeyD);
    map.insert(A::ToolBox, KeyCode::KeyB);
    map.insert(A::ToolCircle, KeyCode::KeyC);
    map.insert(A::ToolPolygon, KeyCode::KeyP);
    map.insert(A::ToolHinge, KeyCode::KeyH);
    map.insert(A::ToolWeld, KeyCode::KeyW);
    map.insert(A::ToolSlider, KeyCode::KeyR);
    map.insert(A::ToolGround, KeyCode::KeyG);
    map.insert(A::ToolCut, KeyCode::KeyK);
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

/// Translates just-pressed actions into intents / state changes.
pub fn apply_shortcuts(
    actions: Query<&ActionState<EditorAction>, With<EditorControls>>,
    mut undo: MessageWriter<UndoIntent>,
    mut redo: MessageWriter<RedoIntent>,
    mut delete: MessageWriter<DeleteIntent>,
    mut selection: ResMut<Selection>,
    ids: Query<&StableId>,
    bodies: Query<Entity, With<Body>>,
    game_state: Res<State<GameState>>,
    mut next_game: ResMut<NextState<GameState>>,
    mut next_tool: ResMut<NextState<ToolState>>,
    mut scale_frame: ResMut<crate::interaction::tools::handles::ScaleFrame>,
) {
    let Ok(actions) = actions.single() else {
        return;
    };

    if actions.just_pressed(&EditorAction::Undo) {
        undo.write(UndoIntent);
    }
    if actions.just_pressed(&EditorAction::Redo) {
        redo.write(RedoIntent);
    }
    if actions.just_pressed(&EditorAction::DeleteSelection) && !selection.is_empty() {
        let targets: Vec<StableId> = selection
            .iter()
            .filter_map(|e| ids.get(e).ok().copied())
            .collect();
        if !targets.is_empty() {
            delete.write(DeleteIntent { targets });
        }
    }
    if actions.just_pressed(&EditorAction::TogglePause) {
        next_game.set(match game_state.get() {
            GameState::Playing => GameState::Paused,
            GameState::Paused => GameState::Playing,
        });
    }
    if actions.just_pressed(&EditorAction::SelectAll) {
        selection.clear();
        for entity in &bodies {
            selection.add(entity);
        }
    }
    if actions.just_pressed(&EditorAction::Deselect) {
        selection.clear();
    }
    if actions.just_pressed(&EditorAction::ToggleScaleFrame) {
        use crate::interaction::tools::handles::ScaleFrame;
        *scale_frame = match *scale_frame {
            ScaleFrame::Global => ScaleFrame::Local,
            ScaleFrame::Local => ScaleFrame::Global,
        };
    }
    for tool_action in EditorAction::TOOLS {
        if actions.just_pressed(&tool_action)
            && let Some(tool) = tool_action.tool()
        {
            next_tool.set(tool);
        }
    }
}
