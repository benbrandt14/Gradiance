//! One way to ask a panel whether it is showing.
//!
//! Panel visibility grew two idioms: a `pub open: bool` field (Properties,
//! Settings, Depth, Array…) and a private field behind hand-written
//! `is_open()`/`toggle()` (Outliner, Console, Plot, Probe, Signals, Node
//! Graph). Every consumer that wants to treat panels uniformly — the View
//! menu, the transport strip, the dock's close buttons — then needed two code
//! paths and could not be written as a table.
//!
//! [`PanelToggle`] is that one path. It is deliberately three methods wide:
//! reading, setting, and the flip that is just the other two. Panels keep
//! whatever field layout suits them; [`crate::impl_panel_toggle!`] writes the
//! implementation for the common case of a `bool` field.
//!
//! # The registry
//!
//! [`Panels::named`](crate::menu::Panels::named) gives every panel a stable lower-case name, which turns
//! the trait into an actual registry: the View menu is a table over it, and so
//! are the `panel-show` / `panel-hide` / `panel-toggle` script verbs
//! ([`apply_panel_requests`]) and the `panel-open?` read
//! ([`publish_panel_states`]). "A menu item is a registered op" —
//! `docs/ui-shell-decision.md`'s no-regret item — is that one function.
//!
//! Scripting adds no mutation path here: a verb queues a request and the UI
//! calls `set_open`, which is what a menu click does. The `EditorState` seam is
//! unchanged.

/// A panel whose visibility can be read and written.
///
/// Implemented by every panel resource in this crate, so chrome that offers
/// panels to the user (the View menu, dock tab close buttons, the transport
/// toggles) can hold them as `&mut dyn PanelToggle` and treat them alike.
pub trait PanelToggle {
    /// Whether the panel is currently showing.
    fn is_open(&self) -> bool;

    /// Shows or hides the panel.
    fn set_open(&mut self, open: bool);

    /// Flips visibility — what a menu checkbox or a keyboard shortcut does.
    fn toggle(&mut self) {
        self.set_open(!self.is_open());
    }
}

/// Implements [`PanelToggle`] for a type whose visibility is one `bool`.
///
/// `impl_panel_toggle!(SettingsWindow, open);` for a named field,
/// `impl_panel_toggle!(OptimizerExpanded, 0);` for a newtype.
#[macro_export]
macro_rules! impl_panel_toggle {
    ($ty:ty, $field:tt) => {
        impl $crate::panels::PanelToggle for $ty {
            fn is_open(&self) -> bool {
                self.$field
            }

            fn set_open(&mut self, open: bool) {
                self.$field = open;
            }
        }
    };
}

/// The two systems that make panels scriptable, plus the name table they share.
///
/// Both live here rather than in `menu.rs` because the *name* is the registry
/// key: adding a panel means adding one row to `Panels::named`, and the View
/// menu, the verbs, and the read mirror all pick it up.
impl crate::menu::Panels<'_> {
    /// Every panel with its script-facing name, in View-menu order.
    ///
    /// Names are lower-case and hyphen-free where possible, so a script reads
    /// as prose: `(panel-show "properties")`. They are deliberately *not* the
    /// menu labels — a label may be retitled for clarity, but a name is an API.
    pub fn named(&mut self) -> Vec<(&'static str, &mut dyn PanelToggle)> {
        vec![
            ("outliner", &mut *self.outliner),
            ("properties", &mut *self.inspector),
            ("depth", &mut *self.depth),
            ("plot", &mut *self.plot),
            ("nodes", &mut *self.node_graph),
            ("console", &mut *self.console),
            ("probe", &mut *self.probe),
            ("array", &mut *self.array),
            ("optimizer", &mut *self.optimizer),
            ("settings", &mut *self.settings),
        ]
    }
}

/// Applies the panel changes scripts asked for, then clears the queue.
///
/// An unknown name warns once and is dropped: a typo should say so rather than
/// fail silently, and it must not abort the rest of the run.
pub fn apply_panel_requests(
    mut requests: bevy::prelude::ResMut<gradiance_script::bridge::PanelRequests>,
    mut panels: crate::menu::Panels,
) {
    if requests.0.is_empty() {
        return;
    }
    for request in std::mem::take(&mut requests.0) {
        let mut table = panels.named();
        if let Some((_, panel)) = table.iter_mut().find(|(name, _)| *name == request.name) {
            match request.shown {
                Some(shown) => panel.set_open(shown),
                None => panel.toggle(),
            }
        } else {
            let known: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
            bevy::log::warn!("no panel named `{}` — try one of {known:?}", request.name);
        }
    }
}

/// Publishes which panels are open, for the `panel-open?` read.
///
/// Panel state lives in this crate and the script layer sits below it, so a
/// read crosses the boundary as a mirror rather than a dependency edge.
pub fn publish_panel_states(
    mut states: bevy::prelude::ResMut<gradiance_script::bridge::PanelStates>,
    mut panels: crate::menu::Panels,
) {
    states.0.clear();
    for (name, panel) in panels.named() {
        states.0.push((name.to_owned(), panel.is_open()));
    }
}

#[cfg(test)]
mod tests {
    use super::PanelToggle;

    #[derive(Default)]
    struct Named {
        open: bool,
    }
    crate::impl_panel_toggle!(Named, open);

    #[derive(Default)]
    struct Tuple(bool);
    crate::impl_panel_toggle!(Tuple, 0);

    #[test]
    fn toggle_is_the_flip_of_the_current_state() {
        let mut p = Named::default();
        assert!(!p.is_open());
        p.toggle();
        assert!(p.is_open());
        p.toggle();
        assert!(!p.is_open());
    }

    #[test]
    fn set_open_is_idempotent_unlike_toggle() {
        let mut p = Tuple::default();
        p.set_open(true);
        p.set_open(true);
        assert!(p.is_open(), "setting twice is still open");
    }

    /// A registry name must resolve to a menu label. A panel added to the table
    /// without a label would silently render as "Panel", which is the kind of
    /// drift a table is supposed to prevent.
    #[test]
    fn every_registry_name_has_a_menu_label() {
        // The names, duplicated deliberately: this test is the contract, so it
        // must not read the same list the code does.
        for name in [
            "outliner",
            "properties",
            "depth",
            "plot",
            "nodes",
            "console",
            "probe",
            "array",
            "optimizer",
            "settings",
        ] {
            assert_ne!(
                crate::menu::menu_label(name),
                "Panel",
                "`{name}` has no menu label"
            );
        }
        // …and an unknown name falls back rather than panicking.
        assert_eq!(crate::menu::menu_label("nope"), "Panel");
    }

    /// The names are an API: lower-case, no spaces, so `(panel-show "…")`
    /// reads as prose and stays stable when a label is retitled.
    #[test]
    fn registry_names_are_script_shaped() {
        for name in [
            "outliner",
            "properties",
            "depth",
            "plot",
            "nodes",
            "console",
            "probe",
            "array",
            "optimizer",
            "settings",
        ] {
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase()),
                "`{name}` is not lower-case ASCII"
            );
        }
    }

    /// The one thing the trait buys: a heterogeneous list of panels driven
    /// through one code path, which is what the View menu becomes.
    #[test]
    fn panels_of_different_shapes_share_one_code_path() {
        let mut named = Named::default();
        let mut tuple = Tuple::default();
        let all: Vec<&mut dyn PanelToggle> = vec![&mut named, &mut tuple];
        for panel in all {
            panel.set_open(true);
        }
        assert!(named.is_open() && tuple.is_open());
    }
}
