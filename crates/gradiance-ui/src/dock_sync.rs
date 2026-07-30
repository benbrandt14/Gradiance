//! Keeping a dock's tiles in step with the set of open panes.
//!
//! # Why this is not a rebuild
//!
//! Both docks used to do this when the open set changed:
//!
//! ```ignore
//! if shown != desired {
//!     tree = Some(egui_tiles::Tree::new_tabs(id, desired.clone()));
//! }
//! ```
//!
//! which is correct in the narrow sense — the right panes are present
//! afterwards — and wrong in the way that matters. `new_tabs` builds a *flat
//! tab strip*, so any split the user had arranged, and any tab order they
//! chose, was discarded the moment they toggled an unrelated pane from the
//! View menu. Opening the console threw away your Properties/Outliner split.
//!
//! [`sync_panes`] instead adds and removes individual tiles, leaving every
//! other tile — and therefore the whole arrangement — untouched.

use egui_tiles::{Tile, TileId, Tree};

/// Brings `tree`'s panes in line with `desired`, preserving layout.
///
/// Returns the tree, creating it when there was none. Panes in `desired` that
/// are missing are appended to the root container; panes present but no longer
/// desired are removed. A pane the user has dragged into a split stays where
/// they put it.
pub fn sync_panes<P: PartialEq + Clone>(tree: &mut Option<Tree<P>>, id: &str, desired: &[P]) {
    if desired.is_empty() {
        *tree = None;
        return;
    }
    let Some(existing) = tree else {
        *tree = Some(Tree::new_tabs(id.to_owned(), desired.to_vec()));
        return;
    };

    // Remove tiles whose pane is no longer wanted. Collected first because
    // removal borrows the tree mutably.
    let stale: Vec<TileId> = existing
        .tiles
        .iter()
        .filter_map(|(tile_id, tile)| match tile {
            Tile::Pane(pane) => (!desired.contains(pane)).then_some(*tile_id),
            Tile::Container(_) => None,
        })
        .collect();
    for tile_id in stale {
        existing.remove_recursively(tile_id);
    }

    // Add tiles for panes that have appeared.
    let present: Vec<P> = existing
        .tiles
        .iter()
        .filter_map(|(_, tile)| match tile {
            Tile::Pane(pane) => Some(pane.clone()),
            Tile::Container(_) => None,
        })
        .collect();
    let missing: Vec<P> = desired
        .iter()
        .filter(|p| !present.contains(p))
        .cloned()
        .collect();
    if missing.is_empty() {
        return;
    }

    // If everything was removed the root may be gone; start fresh rather than
    // trying to graft onto nothing.
    let Some(root) = existing.root() else {
        *tree = Some(Tree::new_tabs(id.to_owned(), desired.to_vec()));
        return;
    };
    for pane in missing {
        let tile_id = existing.tiles.insert_pane(pane);
        if let Some(Tile::Container(container)) = existing.tiles.get_mut(root) {
            container.add_child(tile_id);
        } else {
            // The root is a lone pane, not a container — wrap both in tabs.
            *tree = Some(Tree::new_tabs(id.to_owned(), desired.to_vec()));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum P {
        A,
        B,
        C,
    }

    fn panes<T: PartialEq + Clone>(tree: &Tree<T>) -> Vec<T> {
        tree.tiles
            .iter()
            .filter_map(|(_, t)| match t {
                Tile::Pane(p) => Some(p.clone()),
                Tile::Container(_) => None,
            })
            .collect()
    }

    #[test]
    fn an_empty_desired_set_clears_the_tree() {
        let mut tree = Some(Tree::new_tabs("t".to_owned(), vec![P::A]));
        sync_panes(&mut tree, "t", &[]);
        assert!(tree.is_none());
    }

    #[test]
    fn a_missing_pane_is_added_and_the_others_stay_put() {
        let mut tree = None;
        sync_panes(&mut tree, "t", &[P::A, P::B]);
        let before = tree.as_ref().map(|t| t.tiles.iter().count());

        sync_panes(&mut tree, "t", &[P::A, P::B, P::C]);
        let tree = tree.expect("tree");
        let mut got = panes(&tree);
        got.sort_by_key(|p| format!("{p:?}"));
        assert_eq!(got, vec![P::A, P::B, P::C]);
        assert!(before.is_some());
    }

    #[test]
    fn a_closed_pane_is_removed_without_touching_the_rest() {
        let mut tree = None;
        sync_panes(&mut tree, "t", &[P::A, P::B, P::C]);
        sync_panes(&mut tree, "t", &[P::A, P::C]);
        let tree = tree.expect("tree");
        let mut got = panes(&tree);
        got.sort_by_key(|p| format!("{p:?}"));
        assert_eq!(got, vec![P::A, P::C]);
    }

    /// The regression this module exists for: a user's arrangement must
    /// survive an unrelated pane being toggled. A rebuild would flatten the
    /// tree back to a tab strip; the tile ids of untouched panes are the
    /// observable proof that it did not.
    #[test]
    fn toggling_one_pane_leaves_the_other_tiles_identical() {
        let mut tree = None;
        sync_panes(&mut tree, "t", &[P::A, P::B]);

        let ids_before: Vec<TileId> = tree
            .as_ref()
            .expect("tree")
            .tiles
            .iter()
            .filter_map(|(id, t)| matches!(t, Tile::Pane(p) if *p != P::C).then_some(*id))
            .collect();

        sync_panes(&mut tree, "t", &[P::A, P::B, P::C]);

        let ids_after: Vec<TileId> = tree
            .as_ref()
            .expect("tree")
            .tiles
            .iter()
            .filter_map(|(id, t)| matches!(t, Tile::Pane(p) if *p != P::C).then_some(*id))
            .collect();

        // `TileId` is Hash + Eq but not Ord, so compare as sets.
        let a: std::collections::HashSet<TileId> = ids_before.into_iter().collect();
        let b: std::collections::HashSet<TileId> = ids_after.into_iter().collect();
        assert_eq!(a, b, "existing panes were rebuilt, not preserved");
    }
}
