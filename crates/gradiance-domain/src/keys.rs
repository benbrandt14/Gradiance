//! Key names: the one table between a key's written name (`"W"`, `"Left"`,
//! `"F5"`) and the physical [`KeyCode`] it reads (brief N-2).
//!
//! A node-graph **key input** (`signal::BlockOp::Key`) saves its key by
//! name, so a scene file reads `key: "E"`, not an engine enum. The same
//! table parses the key of an injected chord (`ui.key`), so a script and a
//! saved binding name keys one way. Names follow egui's `Key::name` (the
//! capture button in the graph reads them from there): letters and digits
//! are themselves, `F1`–`F12`, and `Enter`, `Escape`, `Tab`, `Space`,
//! `Backspace`, `Delete`, `Insert`, `Home`, `End`, `PageUp`, `PageDown`,
//! `Up`, `Down`, `Left`, `Right`. Lookup ignores case and takes a few
//! aliases (`Esc`, `Return`, `Del`, `ArrowUp`…).
//!
//! The table has at most 64 rows, so a set of keys is a `u64` bit mask over
//! row indices ([`KeySet`]): a per-frame key check is a shift and an `and`.

use bevy::input::keyboard::KeyCode;

/// One named key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedKey {
    /// The canonical name (as saved and shown).
    pub name: &'static str,
    /// The physical key it reads.
    pub code: KeyCode,
}

const fn key(name: &'static str, code: KeyCode) -> NamedKey {
    NamedKey { name, code }
}

/// Every key a graph input can bind, in a fixed order (a row's index is its
/// bit in a [`KeySet`]; append only, never reorder: nothing persists the
/// index, but tests pin a few).
pub const KEYS: [NamedKey; 64] = [
    key("A", KeyCode::KeyA),
    key("B", KeyCode::KeyB),
    key("C", KeyCode::KeyC),
    key("D", KeyCode::KeyD),
    key("E", KeyCode::KeyE),
    key("F", KeyCode::KeyF),
    key("G", KeyCode::KeyG),
    key("H", KeyCode::KeyH),
    key("I", KeyCode::KeyI),
    key("J", KeyCode::KeyJ),
    key("K", KeyCode::KeyK),
    key("L", KeyCode::KeyL),
    key("M", KeyCode::KeyM),
    key("N", KeyCode::KeyN),
    key("O", KeyCode::KeyO),
    key("P", KeyCode::KeyP),
    key("Q", KeyCode::KeyQ),
    key("R", KeyCode::KeyR),
    key("S", KeyCode::KeyS),
    key("T", KeyCode::KeyT),
    key("U", KeyCode::KeyU),
    key("V", KeyCode::KeyV),
    key("W", KeyCode::KeyW),
    key("X", KeyCode::KeyX),
    key("Y", KeyCode::KeyY),
    key("Z", KeyCode::KeyZ),
    key("0", KeyCode::Digit0),
    key("1", KeyCode::Digit1),
    key("2", KeyCode::Digit2),
    key("3", KeyCode::Digit3),
    key("4", KeyCode::Digit4),
    key("5", KeyCode::Digit5),
    key("6", KeyCode::Digit6),
    key("7", KeyCode::Digit7),
    key("8", KeyCode::Digit8),
    key("9", KeyCode::Digit9),
    key("F1", KeyCode::F1),
    key("F2", KeyCode::F2),
    key("F3", KeyCode::F3),
    key("F4", KeyCode::F4),
    key("F5", KeyCode::F5),
    key("F6", KeyCode::F6),
    key("F7", KeyCode::F7),
    key("F8", KeyCode::F8),
    key("F9", KeyCode::F9),
    key("F10", KeyCode::F10),
    key("F11", KeyCode::F11),
    key("F12", KeyCode::F12),
    key("Enter", KeyCode::Enter),
    key("Escape", KeyCode::Escape),
    key("Tab", KeyCode::Tab),
    key("Space", KeyCode::Space),
    key("Backspace", KeyCode::Backspace),
    key("Delete", KeyCode::Delete),
    key("Insert", KeyCode::Insert),
    key("Home", KeyCode::Home),
    key("End", KeyCode::End),
    key("PageUp", KeyCode::PageUp),
    key("PageDown", KeyCode::PageDown),
    key("Up", KeyCode::ArrowUp),
    key("Down", KeyCode::ArrowDown),
    key("Left", KeyCode::ArrowLeft),
    key("Right", KeyCode::ArrowRight),
    // The frame-selection key (brief S-9). The table is now full: a KeySet
    // is a u64.
    key(".", KeyCode::Period),
];

/// Other spellings a lookup accepts, and the canonical name each means.
const ALIASES: [(&str, &str); 9] = [
    ("esc", "Escape"),
    ("return", "Enter"),
    ("del", "Delete"),
    ("arrowup", "Up"),
    ("arrowdown", "Down"),
    ("arrowleft", "Left"),
    ("arrowright", "Right"),
    ("spacebar", "Space"),
    ("period", "."),
];

/// The table row of the key written `name` (case-insensitive, aliases
/// allowed). Allocation-free: a scan of 64 short names.
pub fn key_index(name: &str) -> Option<usize> {
    let name = name.trim();
    let canonical = ALIASES
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(name))
        .map_or(name, |(_, canonical)| canonical);
    KEYS.iter()
        .position(|k| k.name.eq_ignore_ascii_case(canonical))
}

/// The named key written `name`, if it is one.
pub fn named_key(name: &str) -> Option<NamedKey> {
    key_index(name).and_then(|i| KEYS.get(i).copied())
}

/// The table row of the physical key `code`, if a name binds it.
pub fn index_of_code(code: KeyCode) -> Option<usize> {
    KEYS.iter().position(|k| k.code == code)
}

/// A set of table rows, one bit each.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeySet(pub u64);

impl KeySet {
    /// Adds row `index` (a row past the table is ignored).
    pub fn insert(&mut self, index: usize) {
        if index < KEYS.len() {
            self.0 |= 1 << index;
        }
    }

    /// Whether row `index` is in the set.
    pub const fn contains(self, index: usize) -> bool {
        index < KEYS.len() && self.0 & (1 << index) != 0
    }

    /// Whether the set holds the physical key `code`.
    pub fn contains_code(self, code: KeyCode) -> bool {
        self.0 != 0 && index_of_code(code).is_some_and(|i| self.contains(i))
    }

    /// Whether the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_fit_a_mask() {
        assert!(KEYS.len() <= 64, "a KeySet is a u64");
        for (i, a) in KEYS.iter().enumerate() {
            for b in &KEYS[i + 1..] {
                assert_ne!(a.name, b.name);
                assert_ne!(a.code, b.code);
            }
        }
    }

    #[test]
    fn lookup_ignores_case_and_takes_aliases() {
        assert_eq!(named_key("w").map(|k| k.code), Some(KeyCode::KeyW));
        assert_eq!(named_key("W").map(|k| k.name), Some("W"));
        assert_eq!(named_key("arrowleft").map(|k| k.name), Some("Left"));
        assert_eq!(named_key("ESC").map(|k| k.code), Some(KeyCode::Escape));
        assert_eq!(named_key("f12").map(|k| k.code), Some(KeyCode::F12));
        assert_eq!(named_key("7").map(|k| k.code), Some(KeyCode::Digit7));
        assert_eq!(named_key("Hyper"), None);
        assert_eq!(named_key(""), None);
    }

    #[test]
    fn a_set_holds_rows_by_code() {
        let mut set = KeySet::default();
        assert!(set.is_empty());
        set.insert(key_index("E").unwrap_or(usize::MAX));
        set.insert(usize::MAX);
        assert!(set.contains_code(KeyCode::KeyE));
        assert!(!set.contains_code(KeyCode::KeyQ));
        assert!(!set.contains_code(KeyCode::ControlLeft), "not in the table");
    }
}
