//! Saved dock layouts: **workspaces** (brief U-10).
//!
//! A workspace names which editors the docks show, how their tiles split,
//! and how large each dock is. The editors are named by their panel names
//! (`"outliner"`, `"console"`, … — the names `ui::menu::Panels::named` gives),
//! so this layer holds plain strings and never needs the UI crate. The UI
//! layer turns a [`DockLayout`] into an `egui_tiles` tree and back.
//!
//! [`Workspaces`] is **workstation config**, like `GridSettings`: it is saved
//! with the scene file (there is no per-user config file), and it is never
//! scene content, so undo never touches it.
//!
//! The tile tree is stored flat ([`DockLayout::tiles`], index 0 the root) so
//! the record has no recursive type: reflection and RON stay simple. Build
//! and read one through the recursive [`LayoutNode`].

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

/// The deepest tile tree [`DockLayout::to_node`] reads; deeper is malformed.
const MAX_DEPTH: usize = 16;

/// What one tile of a saved layout is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub enum TileKind {
    /// An editor, by panel name.
    Pane(String),
    /// Tabs over the children; `active` picks the shown one.
    Tabs,
    /// Children side by side, left to right.
    Row,
    /// Children stacked, top to bottom.
    Column,
}

/// One tile of a flat layout: its kind, its children (indices into
/// [`DockLayout::tiles`]) and, for a row or column, each child's share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct LayoutTile {
    /// What the tile is.
    pub kind: TileKind,
    /// Child tiles, by index.
    #[serde(default)]
    pub children: Vec<usize>,
    /// Each child's share of a row or column (any positive scale).
    #[serde(default)]
    pub shares: Vec<f32>,
    /// The shown child of a tab set, by position in `children`.
    #[serde(default)]
    pub active: usize,
}

/// One dock's saved arrangement.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct DockLayout {
    /// The tiles; index 0 is the root. Empty: the dock is closed.
    #[serde(default)]
    pub tiles: Vec<LayoutTile>,
    /// The dock's width (right dock) or height (bottom dock), in points.
    /// Zero or less keeps the current size.
    #[serde(default)]
    pub size: f32,
}

/// The recursive form of a layout, for building one and for reading it back.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutNode {
    /// An editor, by panel name.
    Pane(String),
    /// Tabs over the children, with the index of the shown one.
    Tabs(Vec<LayoutNode>, usize),
    /// Children side by side, each with its share.
    Row(Vec<(LayoutNode, f32)>),
    /// Children stacked, each with its share.
    Column(Vec<(LayoutNode, f32)>),
}

impl LayoutNode {
    /// A tab set over the named editors, the first shown.
    pub fn tabs(names: &[&str]) -> Self {
        Self::Tabs(
            names.iter().map(|n| Self::Pane((*n).to_owned())).collect(),
            0,
        )
    }

    /// The editor names under this node, depth first.
    pub fn panes(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<String>) {
        match self {
            Self::Pane(name) => out.push(name.clone()),
            Self::Tabs(children, _) => {
                for c in children {
                    c.collect(out);
                }
            }
            Self::Row(children) | Self::Column(children) => {
                for (c, _) in children {
                    c.collect(out);
                }
            }
        }
    }
}

impl DockLayout {
    /// Flattens `root` (or nothing: a closed dock) with the dock size.
    pub fn from_node(root: Option<&LayoutNode>, size: f32) -> Self {
        let mut tiles = Vec::new();
        if let Some(root) = root {
            push_node(root, &mut tiles);
        }
        Self { tiles, size }
    }

    /// The recursive form, or `None` for a closed dock or a malformed one
    /// (an index out of range, a tile reached twice, a tree too deep).
    pub fn to_node(&self) -> Option<LayoutNode> {
        if self.tiles.is_empty() {
            return None;
        }
        let mut seen = vec![false; self.tiles.len()];
        read_node(&self.tiles, 0, 0, &mut seen)
    }

    /// The editor names this dock shows, depth first.
    pub fn panes(&self) -> Vec<String> {
        self.to_node().map_or_default(|n| n.panes())
    }
}

/// Appends `node` and its subtree; returns its index.
fn push_node(node: &LayoutNode, tiles: &mut Vec<LayoutTile>) -> usize {
    let index = tiles.len();
    let (kind, active) = match node {
        LayoutNode::Pane(name) => (TileKind::Pane(name.clone()), 0),
        LayoutNode::Tabs(_, active) => (TileKind::Tabs, *active),
        LayoutNode::Row(_) => (TileKind::Row, 0),
        LayoutNode::Column(_) => (TileKind::Column, 0),
    };
    tiles.push(LayoutTile {
        kind,
        children: Vec::new(),
        shares: Vec::new(),
        active,
    });
    let (children, shares): (Vec<usize>, Vec<f32>) = match node {
        LayoutNode::Pane(_) => (Vec::new(), Vec::new()),
        LayoutNode::Tabs(children, _) => (
            children.iter().map(|c| push_node(c, tiles)).collect(),
            Vec::new(),
        ),
        LayoutNode::Row(children) | LayoutNode::Column(children) => children
            .iter()
            .map(|(c, share)| (push_node(c, tiles), *share))
            .unzip(),
    };
    if let Some(tile) = tiles.get_mut(index) {
        tile.children = children;
        tile.shares = shares;
    }
    index
}

/// Reads the tile at `index`, refusing cycles, bad indices and deep trees.
fn read_node(
    tiles: &[LayoutTile],
    index: usize,
    depth: usize,
    seen: &mut [bool],
) -> Option<LayoutNode> {
    if depth > MAX_DEPTH || *seen.get(index)? {
        return None;
    }
    seen[index] = true;
    let tile = tiles.get(index)?;
    let mut children = Vec::with_capacity(tile.children.len());
    for &child in &tile.children {
        children.push(read_node(tiles, child, depth + 1, seen)?);
    }
    let shared = || -> Vec<(LayoutNode, f32)> {
        children
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let share = tile.shares.get(i).copied().unwrap_or(1.0);
                let share = if share.is_finite() && share > 0.0 {
                    share
                } else {
                    1.0
                };
                (c.clone(), share)
            })
            .collect()
    };
    Some(match &tile.kind {
        TileKind::Pane(name) => LayoutNode::Pane(name.clone()),
        TileKind::Tabs => {
            let active = tile.active.min(children.len().saturating_sub(1));
            LayoutNode::Tabs(children, active)
        }
        TileKind::Row => LayoutNode::Row(shared()),
        TileKind::Column => LayoutNode::Column(shared()),
    })
}

/// A saved workspace: a name and both docks' layouts.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct Workspace {
    /// The name on its tab.
    pub name: String,
    /// The right dock.
    #[serde(default)]
    pub right: DockLayout,
    /// The bottom dock.
    #[serde(default)]
    pub bottom: DockLayout,
}

impl Workspace {
    /// Every editor either dock shows.
    pub fn panes(&self) -> Vec<String> {
        let mut out = self.right.panes();
        out.extend(self.bottom.panes());
        out
    }
}

/// The person's saved workspaces and which one is active — workstation
/// config (saved with the scene, never undone). The presets are not stored
/// here: the UI layer owns them, so a new build's presets reach old files.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
#[serde(default)]
pub struct Workspaces {
    /// The workspaces the person saved, in tab order.
    pub saved: Vec<Workspace>,
    /// The active workspace's name (a preset's or a saved one's).
    pub active: String,
}

impl Default for Workspaces {
    fn default() -> Self {
        Self {
            saved: Vec::new(),
            active: "Layout".to_owned(),
        }
    }
}

impl Workspaces {
    /// The saved workspace called `name` (any case).
    pub fn find(&self, name: &str) -> Option<&Workspace> {
        self.saved
            .iter()
            .find(|w| w.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Saves `workspace`, replacing one of the same name (any case) in place
    /// or appending it.
    pub fn upsert(&mut self, workspace: Workspace) {
        match self
            .saved
            .iter_mut()
            .find(|w| w.name.eq_ignore_ascii_case(&workspace.name))
        {
            Some(slot) => *slot = workspace,
            None => self.saved.push(workspace),
        }
    }

    /// Removes the saved workspace called `name`; whether one was removed.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.saved.len();
        self.saved
            .retain(|w| !w.name.eq_ignore_ascii_case(name.trim()));
        self.saved.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LayoutNode {
        LayoutNode::Column(vec![
            (LayoutNode::tabs(&["outliner"]), 0.3),
            (
                LayoutNode::Row(vec![
                    (LayoutNode::tabs(&["console", "properties"]), 2.0),
                    (LayoutNode::Pane("plot".into()), 1.0),
                ]),
                0.7,
            ),
        ])
    }

    #[test]
    fn a_layout_round_trips_through_the_flat_form() {
        let flat = DockLayout::from_node(Some(&sample()), 320.0);
        assert_eq!(flat.to_node(), Some(sample()));
        assert_eq!(flat.panes(), ["outliner", "console", "properties", "plot"]);
    }

    #[test]
    fn a_closed_dock_is_empty() {
        let flat = DockLayout::from_node(None, 0.0);
        assert_eq!(flat.tiles.len(), 0);
        assert_eq!(flat.to_node(), None);
    }

    #[test]
    fn a_malformed_layout_reads_as_none() {
        let mut cyclic = DockLayout::from_node(Some(&LayoutNode::tabs(&["a"])), 0.0);
        cyclic.tiles[0].children.push(0);
        assert_eq!(cyclic.to_node(), None, "a cycle");
        let mut dangling = DockLayout::from_node(Some(&LayoutNode::tabs(&["a"])), 0.0);
        dangling.tiles[0].children.push(9);
        assert_eq!(dangling.to_node(), None, "an index out of range");
    }

    #[test]
    fn saved_workspaces_are_found_replaced_and_removed_by_name_in_any_case() {
        let mut all = Workspaces::default();
        all.upsert(Workspace {
            name: "Mine".into(),
            ..Default::default()
        });
        all.upsert(Workspace {
            name: "mine".into(),
            right: DockLayout {
                size: 1.0,
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(all.saved.len(), 1);
        assert_eq!(all.find(" MINE ").map(|w| w.right.size), Some(1.0));
        assert!(all.remove("Mine"));
        assert!(!all.remove("Mine"));
    }
}
