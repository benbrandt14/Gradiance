//! Input injection (`ui.click`, `ui.drag`, `ui.key`, `ui.text`, `ui.wheel`):
//! a gesture becomes the input a person would give, entered **inside** the
//! app as Bevy input messages, so egui and the interaction tools both read
//! it as real input (`docs/unified-surface-plan.md` § 8).
//!
//! Each frame writes what `bevy_winit` writes for the same event: the specific
//! message (`CursorMoved`, `MouseButtonInput`, `KeyboardInput`, `MouseWheel`)
//! **and** the `WindowEvent` that wraps it (egui reads the pointer from the
//! latter), and a pointer move also sets the window's cursor, which is what
//! the tools read (`Window::cursor_position`). Never `ButtonInput` directly:
//! a press written there loses `just_pressed` before the tools run (the 3-5
//! hazard); a message goes through `InputSystems` like a real one.
//!
//! When the events play follows Craft's injection rules (`SolidCraft`
//! `crates/ui-egui/src/lib.rs` `synthetic_input` and `control.rs`
//! `ui_click`/`ui_drag`, at `a39b74e`; ported as rules, not as a file, because
//! upstream pushes egui events into eframe): **one pointer event per frame**,
//! a drag **positions a frame before it presses**, `steps` lies in 1–200,
//! and a non-finite point is refused before anything is queued (in
//! `ui::ops`). The rest is this module's: a key's modifiers go down a frame
//! before the key and up a frame after it, so egui's modifier state and
//! leafwing's chord both see them held; a text plays in one frame.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKeyCode};
use bevy::input::mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent};
use std::collections::VecDeque;

/// The most drag steps (frames of movement) one gesture may take (Craft's).
pub const MAX_STEPS: u32 = 200;
/// The most frames `ui.key` may hold a key down (about 10 s at 60 fps).
pub const MAX_HOLD: u32 = 600;
/// The most frames the queue holds (about 33 s at 60 fps). A gesture that
/// would pass it is dropped with a warning: input that far behind is no
/// longer what its caller meant.
pub const MAX_QUEUED_FRAMES: usize = 2_000;

/// One gesture, its points resolved to window coordinates (egui logical
/// points from the window's top left), written by the `ui.*` input
/// operations.
#[derive(Message, Debug, Clone, PartialEq)]
pub enum UiInput {
    /// Move there, press, release `button`.
    Click {
        /// Where.
        at: Vec2,
        /// Which button (`ui.click` is the left one).
        button: MouseButton,
    },
    /// Move to `from`, press, move to `to` in `steps` frames, release.
    Drag {
        /// Where the button goes down.
        from: Vec2,
        /// Where it comes up.
        to: Vec2,
        /// Frames of movement between them (1–[`MAX_STEPS`]).
        steps: u32,
        /// Which button drags (`left` for every existing caller; `right`
        /// drives the right-button gestures: rotate, pan, the menu's
        /// deadzone).
        button: MouseButton,
    },
    /// Press a key with its modifiers held, hold it, release it.
    Key {
        /// The chord.
        chord: Chord,
        /// Frames the key stays down (1 to [`MAX_HOLD`]): a held key drives
        /// a graph input for that many frames (brief N-2).
        hold: u32,
    },
    /// Type the text (as the keyboard's text, so a focused field takes it).
    Text(String),
    /// Move there, then turn the wheel by `lines` (> 0: up, away from you).
    Wheel {
        /// Where.
        at: Vec2,
        /// Wheel lines.
        lines: f32,
    },
}

/// A key and the modifiers held for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    /// Held first, released last, in this order.
    pub modifiers: Vec<KeyCode>,
    /// The physical key.
    pub key: KeyCode,
    /// What the key means (its text, for a character key).
    pub logical: Key,
}

/// What one frame of a gesture writes.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The pointer moves here.
    Move(Vec2),
    /// A mouse button goes down (`true`) or up.
    Button(MouseButton, bool),
    /// A key goes down (`true`) or up.
    Key {
        /// The physical key.
        code: KeyCode,
        /// Its meaning.
        logical: Key,
        /// Down or up.
        pressed: bool,
    },
    /// The wheel turns by this many lines.
    Wheel(f32),
}

impl UiInput {
    /// The frames this gesture plays, in order.
    pub fn frames(&self) -> Vec<Vec<Event>> {
        match self {
            Self::Click { at, button } => vec![
                vec![Event::Move(*at)],
                vec![Event::Button(*button, true)],
                vec![Event::Button(*button, false)],
            ],
            Self::Drag {
                from,
                to,
                steps,
                button,
            } => {
                let steps = (*steps).clamp(1, MAX_STEPS);
                let mut frames = vec![vec![Event::Move(*from)], vec![Event::Button(*button, true)]];
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "at most MAX_STEPS steps: exact in f32"
                )]
                frames.extend(
                    (1..=steps).map(|k| vec![Event::Move(from.lerp(*to, k as f32 / steps as f32))]),
                );
                frames.push(vec![Event::Button(*button, false)]);
                frames
            }
            Self::Key { chord, hold } => {
                let key = |pressed| Event::Key {
                    code: chord.key,
                    logical: chord.logical.clone(),
                    pressed,
                };
                let modifiers = |pressed| -> Vec<Event> {
                    chord
                        .modifiers
                        .iter()
                        .map(|&code| Event::Key {
                            code,
                            logical: modifier_logical(code),
                            pressed,
                        })
                        .collect()
                };
                let mut frames = Vec::new();
                if !chord.modifiers.is_empty() {
                    frames.push(modifiers(true));
                }
                frames.push(vec![key(true)]);
                // A held key: frames with no event while it stays down.
                for _ in 1..(*hold).clamp(1, MAX_HOLD) {
                    frames.push(Vec::new());
                }
                frames.push(vec![key(false)]);
                if !chord.modifiers.is_empty() {
                    let mut up = modifiers(false);
                    up.reverse();
                    frames.push(up);
                }
                frames
            }
            Self::Text(text) => vec![
                text.chars()
                    .flat_map(|c| {
                        let (code, logical) = text_key(c);
                        [true, false].map(|pressed| Event::Key {
                            code,
                            logical: logical.clone(),
                            pressed,
                        })
                    })
                    .collect(),
            ],
            Self::Wheel { at, lines } => vec![vec![Event::Move(*at)], vec![Event::Wheel(*lines)]],
        }
    }
}

/// The key a typed character comes from: a newline is Enter, anything else
/// is text with no physical key (as an input method's commit is), so typing
/// into a field cannot trip a tool's letter shortcut.
fn text_key(c: char) -> (KeyCode, Key) {
    match c {
        '\n' => (KeyCode::Enter, Key::Enter),
        _ => (
            KeyCode::Unidentified(NativeKeyCode::Unidentified),
            Key::Character(c.to_string().as_str().into()),
        ),
    }
}

fn modifier_logical(code: KeyCode) -> Key {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
        KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
        KeyCode::SuperLeft | KeyCode::SuperRight => Key::Super,
        _ => Key::Control,
    }
}

/// Reads a chord in the registry's `chord` text: modifiers and a key joined
/// by `+` (`Ctrl+Shift+Z`, `Delete`, `F1`, `Space`). Names are
/// case-insensitive; letters and digits are themselves.
pub fn parse_chord(text: &str) -> Result<Chord, String> {
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    let Some((key_name, modifier_names)) = parts.split_last() else {
        return Err("an empty key".to_owned());
    };
    let mut modifiers = Vec::new();
    for name in modifier_names {
        let code = match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => KeyCode::ControlLeft,
            "shift" => KeyCode::ShiftLeft,
            "alt" | "option" => KeyCode::AltLeft,
            "super" | "cmd" | "meta" | "win" => KeyCode::SuperLeft,
            _ => {
                return Err(format!(
                    "`{name}` is not a modifier (Ctrl, Shift, Alt, Super)"
                ));
            }
        };
        if !modifiers.contains(&code) {
            modifiers.push(code);
        }
    }
    let shift = modifiers.contains(&KeyCode::ShiftLeft);
    let (key, logical) =
        named_key(key_name, shift).ok_or_else(|| format!("`{key_name}` is not a key name"))?;
    Ok(Chord {
        modifiers,
        key,
        logical,
    })
}

/// A key by name, through the one key-name table
/// (`gradiance_domain::keys`, which graph inputs save by): a letter, a digit,
/// `F1`-`F12`, or a named key. The logical key is what the key types (its
/// character, shifted for a letter) or its named meaning.
fn named_key(name: &str, shift: bool) -> Option<(KeyCode, Key)> {
    let named = gradiance_domain::keys::named_key(name)?;
    let character = |c: char| Key::Character(c.to_string().as_str().into());
    let mut chars = named.name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let shown = if shift {
            c.to_ascii_uppercase()
        } else {
            c.to_ascii_lowercase()
        };
        return Some((named.code, character(shown)));
    }
    let logical = match named.code {
        KeyCode::F1 => Key::F1,
        KeyCode::F2 => Key::F2,
        KeyCode::F3 => Key::F3,
        KeyCode::F4 => Key::F4,
        KeyCode::F5 => Key::F5,
        KeyCode::F6 => Key::F6,
        KeyCode::F7 => Key::F7,
        KeyCode::F8 => Key::F8,
        KeyCode::F9 => Key::F9,
        KeyCode::F10 => Key::F10,
        KeyCode::F11 => Key::F11,
        KeyCode::F12 => Key::F12,
        KeyCode::Enter => Key::Enter,
        KeyCode::Escape => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Space => Key::Space,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        _ => return None,
    };
    Some((named.code, logical))
}

/// The frames waiting to play, one per update.
#[derive(Resource, Debug, Default)]
pub struct InputQueue {
    frames: VecDeque<Vec<Event>>,
    /// The last injected pointer position (a move's delta starts there).
    pointer: Option<Vec2>,
}

impl InputQueue {
    /// Queues a gesture's frames after those already waiting. Returns
    /// `false` (and queues nothing) when they would pass
    /// [`MAX_QUEUED_FRAMES`].
    pub fn push(&mut self, gesture: &UiInput) -> bool {
        let frames = gesture.frames();
        if self.frames.len() + frames.len() > MAX_QUEUED_FRAMES {
            return false;
        }
        self.frames.extend(frames);
        true
    }

    /// The next frame's events (empty when nothing waits).
    pub fn next_frame(&mut self) -> Vec<Event> {
        self.frames.pop_front().unwrap_or_default()
    }

    /// Frames still to play.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether nothing waits.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// The message writers one injected frame uses.
#[derive(bevy::ecs::system::SystemParam)]
pub struct InputWriters<'w> {
    window_events: MessageWriter<'w, WindowEvent>,
    cursor: MessageWriter<'w, CursorMoved>,
    buttons: MessageWriter<'w, MouseButtonInput>,
    keys: MessageWriter<'w, KeyboardInput>,
    wheel: MessageWriter<'w, MouseWheel>,
}

/// Queues each [`UiInput`] and plays the next frame of the queue
/// (`PreUpdate`, before `InputSystems`). Headless (no window) the messages
/// name a placeholder window: keys and buttons still reach `ButtonInput`,
/// a pointer move has no cursor to set.
pub fn play_input(
    mut gestures: MessageReader<UiInput>,
    mut queue: ResMut<InputQueue>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut out: InputWriters,
) {
    for gesture in gestures.read() {
        if !queue.push(gesture) {
            warn!("ui input: queue full ({MAX_QUEUED_FRAMES} frames); dropped {gesture:?}");
        }
    }
    let frame = queue.next_frame();
    if frame.is_empty() {
        return;
    }
    let mut window = windows.iter_mut().next();
    let id = window.as_ref().map_or(Entity::PLACEHOLDER, |(id, _)| *id);
    for event in frame {
        match event {
            Event::Move(position) => {
                let delta = queue.pointer.map(|last| position - last);
                queue.pointer = Some(position);
                if let Some((_, window)) = window.as_mut() {
                    window.set_cursor_position(Some(position));
                }
                let moved = CursorMoved {
                    window: id,
                    position,
                    delta,
                };
                out.window_events.write(moved.clone().into());
                out.cursor.write(moved);
            }
            Event::Button(button, pressed) => {
                let message = MouseButtonInput {
                    button,
                    state: state(pressed),
                    window: id,
                };
                out.window_events.write(message.into());
                out.buttons.write(message);
            }
            Event::Key {
                code,
                logical,
                pressed,
            } => {
                let text = match &logical {
                    Key::Character(c) if pressed => Some(c.clone()),
                    Key::Space if pressed => Some(" ".into()),
                    _ => None,
                };
                let message = KeyboardInput {
                    key_code: code,
                    logical_key: logical,
                    state: state(pressed),
                    text,
                    repeat: false,
                    window: id,
                };
                out.window_events.write(message.clone().into());
                out.keys.write(message);
            }
            Event::Wheel(lines) => {
                let message = MouseWheel {
                    unit: MouseScrollUnit::Line,
                    x: 0.0,
                    y: lines,
                    window: id,
                    phase: TouchPhase::Moved,
                };
                out.window_events.write(message.into());
                out.wheel.write(message);
            }
        }
    }
}

fn state(pressed: bool) -> ButtonState {
    if pressed {
        ButtonState::Pressed
    } else {
        ButtonState::Released
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_moves_then_presses_then_releases_one_frame_each() {
        let frames = UiInput::Click {
            at: Vec2::new(32.0, 141.0),
            button: MouseButton::Left,
        }
        .frames();
        assert_eq!(
            frames,
            [
                vec![Event::Move(Vec2::new(32.0, 141.0))],
                vec![Event::Button(MouseButton::Left, true)],
                vec![Event::Button(MouseButton::Left, false)],
            ]
        );
    }

    #[test]
    fn a_drag_positions_before_it_presses_and_moves_in_steps() {
        let frames = UiInput::Drag {
            from: Vec2::ZERO,
            to: Vec2::new(100.0, 0.0),
            steps: 4,
            button: MouseButton::Left,
        }
        .frames();
        assert_eq!(frames.len(), 1 + 1 + 4 + 1);
        assert_eq!(frames[0], [Event::Move(Vec2::ZERO)]);
        assert_eq!(frames[1], [Event::Button(MouseButton::Left, true)]);
        assert_eq!(frames[2], [Event::Move(Vec2::new(25.0, 0.0))]);
        assert_eq!(frames[5], [Event::Move(Vec2::new(100.0, 0.0))]);
        assert_eq!(
            frames.last(),
            Some(&vec![Event::Button(MouseButton::Left, false)])
        );
        for frame in &frames {
            assert_eq!(frame.len(), 1, "one pointer event per frame");
        }

        let clamp = |steps| {
            UiInput::Drag {
                from: Vec2::ZERO,
                to: Vec2::ONE,
                steps,
                button: MouseButton::Left,
            }
            .frames()
            .len()
        };
        assert_eq!(clamp(0), 4, "at least one step");
        assert_eq!(clamp(10_000), 3 + MAX_STEPS as usize, "at most MAX_STEPS");
    }

    #[test]
    fn a_named_button_drag_presses_that_button() {
        let frames = UiInput::Drag {
            from: Vec2::ZERO,
            to: Vec2::new(60.0, 0.0),
            steps: 2,
            button: MouseButton::Right,
        }
        .frames();
        assert_eq!(frames[1], [Event::Button(MouseButton::Right, true)]);
        assert_eq!(
            frames.last(),
            Some(&vec![Event::Button(MouseButton::Right, false)])
        );
        assert!(
            !frames
                .iter()
                .any(|f| f.contains(&Event::Button(MouseButton::Left, true))),
            "the left button stays up: {frames:?}"
        );
    }

    #[test]
    fn a_chord_holds_its_modifiers_around_the_key() {
        let chord = parse_chord("Ctrl+Shift+Z").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(chord.modifiers, [KeyCode::ControlLeft, KeyCode::ShiftLeft]);
        assert_eq!(chord.key, KeyCode::KeyZ);
        assert_eq!(chord.logical, Key::Character("Z".into()));
        let frames = UiInput::Key { chord, hold: 1 }.frames();
        assert_eq!(frames.len(), 4);
        let down = |e: &Event| matches!(e, Event::Key { pressed: true, .. });
        assert!(frames[0].iter().all(down) && frames[0].len() == 2);
        assert!(matches!(
            frames[1].as_slice(),
            [Event::Key {
                code: KeyCode::KeyZ,
                pressed: true,
                ..
            }]
        ));
        assert!(matches!(
            frames[3].as_slice(),
            [
                Event::Key {
                    code: KeyCode::ShiftLeft,
                    pressed: false,
                    ..
                },
                Event::Key {
                    code: KeyCode::ControlLeft,
                    pressed: false,
                    ..
                }
            ]
        ));

        let plain = UiInput::Key {
            chord: parse_chord("delete").unwrap_or_else(|e| panic!("{e}")),
            hold: 1,
        };
        assert_eq!(plain.frames().len(), 2, "no modifier frames");
    }

    #[test]
    fn a_held_key_stays_down_for_its_frames() {
        let chord = parse_chord("E").unwrap_or_else(|e| panic!("{e}"));
        let frames = UiInput::Key {
            chord: chord.clone(),
            hold: 30,
        }
        .frames();
        assert_eq!(frames.len(), 31, "down, 29 quiet frames, up");
        assert!(matches!(
            frames[0].as_slice(),
            [Event::Key { pressed: true, .. }]
        ));
        assert!(frames[1..30].iter().all(Vec::is_empty));
        assert!(matches!(
            frames[30].as_slice(),
            [Event::Key { pressed: false, .. }]
        ));
        let capped = UiInput::Key {
            chord,
            hold: u32::MAX,
        };
        assert_eq!(capped.frames().len(), MAX_HOLD as usize + 1);
    }

    #[test]
    fn chord_names_parse_and_unknown_ones_are_refused() {
        for (text, key) in [
            ("a", KeyCode::KeyA),
            ("7", KeyCode::Digit7),
            ("F12", KeyCode::F12),
            ("Escape", KeyCode::Escape),
            ("Space", KeyCode::Space),
            ("Ctrl+ArrowLeft", KeyCode::ArrowLeft),
        ] {
            assert_eq!(parse_chord(text).map(|c| c.key), Ok(key), "{text}");
        }
        for bad in ["", "F13", "Hyper+A", "Ctrl+", "Shift+Nope"] {
            assert!(parse_chord(bad).is_err(), "{bad} parsed");
        }
    }

    #[test]
    fn text_plays_in_one_frame_and_a_newline_is_enter() {
        let frames = UiInput::Text("ab\n".into()).frames();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].len(), 6, "a press and a release per character");
        assert!(matches!(
            frames[0][4],
            Event::Key {
                code: KeyCode::Enter,
                pressed: true,
                ..
            }
        ));
    }

    #[test]
    fn the_queue_plays_gestures_in_order_and_refuses_past_its_budget() {
        let mut queue = InputQueue::default();
        assert!(queue.push(&UiInput::Click {
            at: Vec2::ZERO,
            button: MouseButton::Left,
        }));
        assert!(queue.push(&UiInput::Text("x".into())));
        assert_eq!(queue.len(), 4);
        assert_eq!(queue.next_frame(), [Event::Move(Vec2::ZERO)]);
        let long = UiInput::Drag {
            from: Vec2::ZERO,
            to: Vec2::ONE,
            steps: MAX_STEPS,
            button: MouseButton::Left,
        };
        while queue.push(&long) {}
        assert!(queue.len() <= MAX_QUEUED_FRAMES);
        while !queue.is_empty() {
            queue.next_frame();
        }
        assert_eq!(queue.next_frame(), []);
    }
}
