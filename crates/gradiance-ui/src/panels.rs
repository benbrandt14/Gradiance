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
