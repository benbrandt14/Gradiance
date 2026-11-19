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

//!
//! # Workspaces
//!
//! A workspace (brief U-10) is the one place a tree is built whole:
//! [`tree_from_layout`] turns a saved [`LayoutNode`] into a tree, and
//! [`layout_from_tree`] reads the live tree back. Applying a workspace *is*
//! choosing a new arrangement, so nothing of the old one is meant to
//! survive; every pane's content lives in its own resource, not in the tree,
//! so no editor state is lost. Between applies, [`sync_panes`] and
//! [`swap_pane`] edit tiles in place as before.

use egui_tiles::{Container, LinearDir, Tile, TileId, Tiles, Tree};
use gradiance_domain::workspace::LayoutNode;

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

/// Builds a tree from a saved layout. `pane` maps a panel name to this
/// dock's pane; names it does not know, and a pane already placed, are
/// dropped, as are containers left empty. `None` when nothing remains.
pub fn tree_from_layout<P: PartialEq + Clone>(
    id: &str,
    layout: &LayoutNode,
    pane: impl Fn(&str) -> Option<P>,
) -> Option<Tree<P>> {
    let mut tiles = Tiles::default();
    let mut placed = Vec::new();
    let root = insert_node(&mut tiles, layout, &pane, &mut placed)?;
    Some(Tree::new(id.to_owned(), root, tiles))
}

fn insert_node<P: PartialEq + Clone>(
    tiles: &mut Tiles<P>,
    node: &LayoutNode,
    pane: &impl Fn(&str) -> Option<P>,
    placed: &mut Vec<P>,
) -> Option<TileId> {
    match node {
        LayoutNode::Pane(name) => {
            let p = pane(name)?;
            if placed.contains(&p) {
                return None;
            }
            placed.push(p.clone());
            Some(tiles.insert_pane(p))
        }
        LayoutNode::Tabs(children, active) => {
            let mut ids = Vec::new();
            let mut shown = None;
            for (i, child) in children.iter().enumerate() {
                if let Some(id) = insert_node(tiles, child, pane, placed) {
                    if i == *active {
                        shown = Some(id);
                    }
                    ids.push(id);
                }
            }
            if ids.is_empty() {
                return None;
            }
            let mut tabs = egui_tiles::Tabs::new(ids);
            if let Some(shown) = shown {
                tabs.set_active(shown);
            }
            Some(tiles.insert_container(tabs))
        }
        LayoutNode::Row(children) | LayoutNode::Column(children) => {
            let dir = if matches!(node, LayoutNode::Row(_)) {
                LinearDir::Horizontal
            } else {
                LinearDir::Vertical
            };
            let mut ids = Vec::new();
            let mut shares = Vec::new();
            for (child, share) in children {
                if let Some(id) = insert_node(tiles, child, pane, placed) {
                    ids.push(id);
                    shares.push(*share);
                }
            }
            if ids.is_empty() {
                return None;
            }
            let mut linear = egui_tiles::Linear::new(dir, ids.clone());
            for (id, share) in ids.into_iter().zip(shares) {
                linear.shares.set_share(id, share);
            }
            Some(tiles.insert_container(linear))
        }
    }
}

/// Reads a live tree back as a layout, naming each pane with `name`. A grid
/// container reads as tabs (layouts have no grids).
pub fn layout_from_tree<P>(tree: &Tree<P>, name: impl Fn(&P) -> &str) -> Option<LayoutNode> {
    read_tile(&tree.tiles, tree.root()?, &name, 0)
}

fn read_tile<P>(
    tiles: &Tiles<P>,
    id: TileId,
    name: &impl Fn(&P) -> &str,
    depth: usize,
) -> Option<LayoutNode> {
    // egui_tiles trees are acyclic; the bound only guards a corrupted one.
    if depth > 32 {
        return None;
    }
    let read_all = |children: &[TileId]| -> Vec<(TileId, LayoutNode)> {
        children
            .iter()
            .filter_map(|c| read_tile(tiles, *c, name, depth + 1).map(|n| (*c, n)))
            .collect()
    };
    match tiles.get(id)? {
        Tile::Pane(p) => Some(LayoutNode::Pane(name(p).to_owned())),
        Tile::Container(Container::Tabs(tabs)) => {
            let children = read_all(&tabs.children);
            let active = children
                .iter()
                .position(|(c, _)| Some(*c) == tabs.active)
                .unwrap_or(0);
            Some(LayoutNode::Tabs(
                children.into_iter().map(|(_, n)| n).collect(),
                active,
            ))
        }
        Tile::Container(Container::Linear(linear)) => {
            let children: Vec<(LayoutNode, f32)> = read_all(&linear.children)
                .into_iter()
                .map(|(c, n)| (n, linear.shares[c]))
                .collect();
            Some(match linear.dir {
                LinearDir::Horizontal => LayoutNode::Row(children),
                LinearDir::Vertical => LayoutNode::Column(children),
            })
        }
        Tile::Container(Container::Grid(grid)) => {
            let ids: Vec<TileId> = grid.children().copied().collect();
            let children = read_all(&ids);
            Some(LayoutNode::Tabs(
                children.into_iter().map(|(_, n)| n).collect(),
                0,
            ))
        }
    }
}

/// Puts `to` in the tile that shows `from` — an area switching its editor.
/// If `to` is already in another tile the two exchange places, so nothing
/// closes. Returns whether `from` was found.
pub fn swap_pane<P: PartialEq + Clone>(tree: &mut Tree<P>, from: &P, to: &P) -> bool {
    let find = |tree: &Tree<P>, want: &P| {
        tree.tiles.iter().find_map(|(id, tile)| match tile {
            Tile::Pane(p) if p == want => Some(*id),
            _ => None,
        })
    };
    let Some(from_tile) = find(tree, from) else {
        return false;
    };
    if let Some(to_tile) = find(tree, to)
        && let Some(Tile::Pane(p)) = tree.tiles.get_mut(to_tile)
    {
        *p = from.clone();
    }
    if let Some(Tile::Pane(p)) = tree.tiles.get_mut(from_tile) {
        *p = to.clone();
    }
    true
}

/// The panes of `tree` in depth-first tile order (what a layout lists).
pub fn pane_order<P: Clone>(tree: &Tree<P>) -> Vec<P> {
    fn walk<P: Clone>(tiles: &Tiles<P>, id: TileId, out: &mut Vec<P>, depth: usize) {
        if depth > 32 {
            return;
        }
        match tiles.get(id) {
            Some(Tile::Pane(p)) => out.push(p.clone()),
            Some(Tile::Container(c)) => {
                for child in c.children_vec() {
                    walk(tiles, child, out, depth + 1);
                }
            }
            None => {}
        }
    }
    let mut out = Vec::new();
    if let Some(root) = tree.root() {
        walk(&tree.tiles, root, &mut out, 0);
    }
    out
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

    fn by_name(name: &str) -> Option<P> {
        match name {
            "a" => Some(P::A),
            "b" => Some(P::B),
            "c" => Some(P::C),
            _ => None,
        }
    }

    fn name_of(p: &P) -> &str {
        match p {
            P::A => "a",
            P::B => "b",
            P::C => "c",
        }
    }

    #[test]
    fn a_layout_builds_a_tree_that_reads_back_the_same() {
        let layout = LayoutNode::Column(vec![
            (LayoutNode::tabs(&["a"]), 1.0),
            (
                LayoutNode::Tabs(
                    vec![LayoutNode::Pane("b".into()), LayoutNode::Pane("c".into())],
                    1,
                ),
                3.0,
            ),
        ]);
        let tree = tree_from_layout("t", &layout, by_name).expect("tree");
        assert_eq!(pane_order(&tree), vec![P::A, P::B, P::C]);
        assert_eq!(layout_from_tree(&tree, name_of), Some(layout));
    }

    #[test]
    fn unknown_and_repeated_names_are_dropped() {
        let layout = LayoutNode::Row(vec![
            (LayoutNode::tabs(&["a", "zz", "a"]), 1.0),
            (LayoutNode::tabs(&["zz"]), 1.0),
        ]);
        let tree = tree_from_layout("t", &layout, by_name).expect("tree");
        assert_eq!(pane_order(&tree), vec![P::A]);
        assert!(tree_from_layout("t", &LayoutNode::tabs(&["zz"]), by_name).is_none());
    }

    #[test]
    fn a_swap_replaces_in_place_or_exchanges() {
        let layout = LayoutNode::Column(vec![
            (LayoutNode::tabs(&["a"]), 1.0),
            (LayoutNode::tabs(&["b"]), 1.0),
        ]);
        let mut tree = tree_from_layout("t", &layout, by_name).expect("tree");
        assert!(swap_pane(&mut tree, &P::A, &P::C));
        assert_eq!(pane_order(&tree), vec![P::C, P::B]);
        assert!(swap_pane(&mut tree, &P::C, &P::B));
        assert_eq!(pane_order(&tree), vec![P::B, P::C]);
        assert!(!swap_pane(&mut tree, &P::A, &P::B), "a is not shown");
    }
}
