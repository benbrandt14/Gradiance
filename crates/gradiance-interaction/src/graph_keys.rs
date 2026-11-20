//! Keys as node-graph inputs (brief N-2): a held key publishes a bus value
//! during play, and a graph-bound key takes its key from the editor while
//! the sim plays.
//!
//! A key input is a key node of the authored signal graph
//! (scene content: saved, one `settings` undo step per edit). Its expression
//! reads the bus inputs `key@<name>`; [`publish_graph_keys`] writes `1` or
//! `0` there each frame, just before the kernels run. Those values are
//! **derived**: never saved, never a command, never an undo step.
//!
//! **The gate** ([`keys_live`]): a key counts as held only while the
//! sim plays (`GameState::Playing`) and no UI field has the keyboard
//! ([`KeyboardCaptured`](crate::KeyboardCaptured): a focused text edit, a
//! hovered number field, the graph's own key-capture button). Paused, every
//! key input reads `0` and every key is the editor's.
//!
//! **Who wins a key:** while the gate is open, a key a graph input binds is
//! the graph's. The editor's bare-key uses of it yield: an [`EditorAction`]
//! bound to the key alone (`W` for the Weld tool, `Space` for pause), the
//! arrow-key camera pan, `Home`, and the `Enter`/`Escape`/`X`/`Y` a tool
//! gesture reads. A chord with a modifier (`Ctrl+S`) is not the bare key and
//! still reaches the editor. [`EditorKeyTable`] lists the clashes so the
//! input block can warn about them.
//!
//! **Cost per frame:** [`publish_graph_keys`] visits each key reference once
//! (a `KeyCode` read and a bus publish, which scans the bus names); the gate
//! is two resource reads; a yielding check is a shift and an `and` on a
//! `u64`. Nothing allocates: the reference list is rebuilt only when the
//! authored signals change.

use crate::input::EditorAction;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use gradiance_core::states::GameState;
use gradiance_domain::keys::{KEYS, KeySet, index_of_code, key_index};
use gradiance_domain::signal::key_of_input;
use gradiance_signal::{CompiledSignals, SignalBus};
use leafwing_input_manager::prelude::*;

/// The keys the node graph binds, and the bus inputs that carry them.
/// Derived from the compiled signal graph on change; never saved.
#[derive(Resource, Debug, Default)]
pub struct GraphKeys {
    /// Every key some key block binds.
    bound: KeySet,
    /// Each `key@<name>` bus input a kernel reads, with its key's table row.
    refs: Vec<(String, usize)>,
}

impl GraphKeys {
    /// Rebuilds the bound set and the reference list from the bus inputs
    /// the compiled signals read (cold: runs when they are recompiled). A
    /// key node lowers to reads of `key@<name>`, so every key it binds is
    /// among them, wherever it sits in the graph.
    pub fn rebuild<'a>(&mut self, inputs: impl IntoIterator<Item = &'a str>) {
        self.bound = KeySet::default();
        self.refs.clear();
        for input in inputs {
            let Some(index) = key_of_input(input).and_then(key_index) else {
                continue;
            };
            self.bound.insert(index);
            if !self.refs.iter().any(|(name, _)| name == input) {
                self.refs.push((input.to_owned(), index));
            }
        }
    }

    /// Every key a graph input binds.
    pub const fn bound(&self) -> KeySet {
        self.bound
    }
}

/// Whether the graph's keys are live: the sim plays and no UI field has the
/// keyboard.
pub fn keys_live(game: Option<&State<GameState>>, captured: bool) -> bool {
    !captured && game.is_some_and(|g| *g.get() == GameState::Playing)
}

/// Installs the graph keys: the bound set follows the authored signals; the
/// held states land on the bus between the frame's first publisher and the
/// kernels that read them.
pub(crate) fn install(app: &mut App) {
    app.init_resource::<GraphKeys>();
    app.add_systems(
        Update,
        (
            refresh_graph_keys.run_if(resource_exists_and_changed::<CompiledSignals>),
            publish_graph_keys.run_if(resource_exists::<SignalBus>),
        )
            .chain()
            .after(gradiance_signal::publish_sensor_refs)
            .before(gradiance_signal::evaluate_computed),
    );
}

/// Rebuilds [`GraphKeys`] when the signal graph is recompiled.
pub fn refresh_graph_keys(compiled: Res<CompiledSignals>, mut keys: ResMut<GraphKeys>) {
    keys.rebuild(compiled.inputs());
}

/// Publishes each key reference's held state on the bus (`1` held, `0` not)
/// for the kernels that read it. Runs after the frame's first publisher (it
/// stamps the time) and before `evaluate_computed`. Allocation-free once
/// each name is on the bus.
pub fn publish_graph_keys(
    graph: Res<GraphKeys>,
    keys: Res<ButtonInput<KeyCode>>,
    game: Res<State<GameState>>,
    captured: Res<crate::KeyboardCaptured>,
    step: Res<gradiance_physics::SimStep>,
    mut bus: ResMut<SignalBus>,
) {
    let live = keys_live(Some(&game), captured.0);
    let recording = *game.get() == GameState::Playing || step.active();
    for (name, index) in &graph.refs {
        let held = live && KEYS.get(*index).is_some_and(|k| keys.pressed(k.code));
        bus.publish(name, if held { 1.0 } else { 0.0 }, recording);
    }
}

/// The keyboard as the editor sees it: [`ButtonInput<KeyCode>`] less the
/// keys a graph input owns while the sim plays. Editor systems that read a
/// bare key read it through this, so a graph-bound key cannot also pan the
/// camera or cancel a gesture.
#[derive(SystemParam)]
pub struct EditorKeys<'w> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    graph: Option<Res<'w, GraphKeys>>,
    game: Option<Res<'w, State<GameState>>>,
    captured: Option<Res<'w, crate::KeyboardCaptured>>,
}

impl EditorKeys<'_> {
    /// Whether a graph input owns `code` this frame.
    pub fn claimed(&self, code: KeyCode) -> bool {
        let Some(graph) = &self.graph else {
            return false;
        };
        !graph.bound.is_empty()
            && graph.bound.contains_code(code)
            && keys_live(
                self.game.as_deref(),
                self.captured.as_ref().is_some_and(|c| c.0),
            )
    }

    /// `ButtonInput::pressed`, false for a key the graph owns.
    pub fn pressed(&self, code: KeyCode) -> bool {
        self.keys.pressed(code) && !self.claimed(code)
    }

    /// `ButtonInput::just_pressed`, false for a key the graph owns.
    pub fn just_pressed(&self, code: KeyCode) -> bool {
        self.keys.just_pressed(code) && !self.claimed(code)
    }

    /// Whether `action` yields to the graph this frame: one of its bare-key
    /// bindings is a graph-owned key that is down.
    pub fn action_yields(&self, table: &EditorKeyTable, action: EditorAction) -> bool {
        table.rows.iter().any(|row| {
            row.action == Some(action)
                && KEYS
                    .get(row.key)
                    .is_some_and(|k| self.keys.pressed(k.code) && self.claimed(k.code))
        })
    }
}

/// One editor use of a bare key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorKeyUse {
    /// The key's row in the key table.
    pub key: usize,
    /// The editor action bound to it (`None` for a key a system reads
    /// directly: the camera pan, `Home`, a gesture's `Enter`/`Escape`).
    pub action: Option<EditorAction>,
    /// What the key does in the editor, for the clash warning.
    pub label: String,
}

/// The editor's bare-key uses: built once from the editor's input map plus
/// the keys systems read directly. The node graph warns when an input binds
/// one of them; while the sim plays, the graph wins.
#[derive(Resource, Debug, Default, Clone)]
pub struct EditorKeyTable {
    /// Every use, in table order.
    pub rows: Vec<EditorKeyUse>,
}

impl EditorKeyTable {
    /// The table for `map` (its bare-key bindings) and the fixed keys.
    pub fn from_map(map: &InputMap<EditorAction>) -> Self {
        let mut rows = Vec::new();
        for (action, binding) in map.buttonlike_bindings() {
            for (key, named) in KEYS.iter().enumerate() {
                let bare: &dyn Buttonlike = &named.code;
                if binding == bare {
                    rows.push(EditorKeyUse {
                        key,
                        action: Some(*action),
                        label: action_label(*action),
                    });
                }
            }
        }
        let direct: [(KeyCode, &str); 9] = [
            (KeyCode::ArrowLeft, "pans the camera"),
            (KeyCode::ArrowRight, "pans the camera"),
            (KeyCode::ArrowUp, "pans the camera"),
            (KeyCode::ArrowDown, "pans the camera"),
            (KeyCode::Home, "resets the view"),
            (KeyCode::Enter, "confirms a tool gesture"),
            (KeyCode::Escape, "cancels a tool gesture"),
            (KeyCode::KeyX, "locks a drag to the grid's first axis"),
            (KeyCode::KeyY, "locks a drag to the grid's second axis"),
        ];
        for (code, label) in direct {
            if let Some(key) = index_of_code(code) {
                rows.push(EditorKeyUse {
                    key,
                    action: None,
                    label: label.to_owned(),
                });
            }
        }
        rows.sort_by_key(|row| row.key);
        Self { rows }
    }

    /// What the key written `name` does in the editor (empty: nothing).
    pub fn uses(&self, name: &str) -> impl Iterator<Item = &str> {
        let key = key_index(name);
        self.rows
            .iter()
            .filter(move |row| Some(row.key) == key)
            .map(|row| row.label.as_str())
    }
}

/// A short phrase for an editor action, for the clash warning.
fn action_label(action: EditorAction) -> String {
    match action {
        EditorAction::Undo => "undoes".to_owned(),
        EditorAction::Redo => "redoes".to_owned(),
        EditorAction::DeleteSelection => "deletes the selection".to_owned(),
        EditorAction::TogglePause => "plays and pauses".to_owned(),
        EditorAction::SelectAll => "selects all".to_owned(),
        EditorAction::Deselect => "clears the selection".to_owned(),
        EditorAction::ToggleScaleFrame => "toggles the scale frame".to_owned(),
        EditorAction::Save | EditorAction::SaveAs => "saves".to_owned(),
        EditorAction::Open => "opens a scene".to_owned(),
        EditorAction::Snapshot => "writes a debug snapshot".to_owned(),
        EditorAction::Group => "groups".to_owned(),
        EditorAction::Ungroup => "ungroups".to_owned(),
        EditorAction::Copy => "copies".to_owned(),
        EditorAction::Paste => "pastes".to_owned(),
        EditorAction::Tool(tool) => format!("picks the {tool:?} tool"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_domain::signal::{BlockOp, SignalExpr};

    /// The bus inputs a key block's lowered expression reads.
    fn key_block(key: &str, negative: Option<&str>) -> Vec<String> {
        BlockOp::Key {
            key: key.into(),
            negative: negative.map(Into::into),
        }
        .to_expr()
        .inputs()
    }

    #[test]
    fn the_bound_set_and_references_follow_the_key_blocks() {
        let inputs: Vec<String> = [
            key_block("e", Some("Q")),
            key_block("E", None),
            key_block("Hyper", None),
            SignalExpr::parse_rpn("t sin").unwrap().inputs(),
        ]
        .concat();
        let mut graph = GraphKeys::default();
        graph.rebuild(inputs.iter().map(String::as_str));
        assert!(graph.bound().contains_code(KeyCode::KeyE));
        assert!(graph.bound().contains_code(KeyCode::KeyQ));
        assert!(!graph.bound().contains_code(KeyCode::KeyW));
        let names: Vec<&str> = graph.refs.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            ["key@E", "key@Q"],
            "one reference per key; none for an unknown name"
        );
        graph.rebuild([]);
        assert!(graph.bound().is_empty());
    }

    #[test]
    fn the_editor_table_names_bare_keys_only() {
        let mut map = InputMap::default();
        map.insert(
            EditorAction::Tool(gradiance_core::states::ToolState::Weld),
            KeyCode::KeyW,
        );
        map.insert(EditorAction::Save, ModifierKey::Control.with(KeyCode::KeyS));
        let table = EditorKeyTable::from_map(&map);
        assert_eq!(table.uses("w").collect::<Vec<_>>(), ["picks the Weld tool"]);
        assert_eq!(table.uses("S").count(), 0, "Ctrl+S is a chord, not S");
        assert_eq!(table.uses("Left").collect::<Vec<_>>(), ["pans the camera"]);
        assert_eq!(table.uses("E").count(), 0);
    }

    #[test]
    fn keys_are_live_only_in_play_with_the_keyboard_free() {
        let playing = State::new(GameState::Playing);
        let paused = State::new(GameState::Paused);
        assert!(keys_live(Some(&playing), false));
        assert!(!keys_live(Some(&playing), true), "a field has the keyboard");
        assert!(
            !keys_live(Some(&paused), false),
            "paused: the editor's keys"
        );
        assert!(!keys_live(None, false));
    }
}
