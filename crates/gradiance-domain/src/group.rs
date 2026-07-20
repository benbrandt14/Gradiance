//! Selection grouping — **hierarchical**.
//!
//! A body carries a *stack* of group ids, innermost first:
//!
//! ```text
//! group(A, B, C)            A[1]      B[1]      C[1]
//! group(that, D)            A[1,2]    B[1,2]    C[1,2]    D[2]
//! ungroup(selection)        A[1]      B[1]      C[1]      D
//!                           └── the inner group survives ──┘
//! ```
//!
//! Selecting a grouped body selects everything sharing its **outermost**
//! id; ungrouping pops only the outermost id, so nested groups peel like
//! an onion instead of dissolving.
//!
//! ```
//! use gradiance_domain::group::SelectionGroup;
//!
//! let nested = SelectionGroup(vec![1, 2]); // in group 1, inside group 2
//! assert_eq!(nested.outermost(), Some(2));
//! ```

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// The group stack a body belongs to (innermost first, outermost last).
///
/// Never empty while attached — commands remove the component instead of
/// leaving an empty stack.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SelectionGroup(pub Vec<u32>);

impl SelectionGroup {
    /// The outermost group id — the one selection expands by and
    /// ungroup pops.
    pub fn outermost(&self) -> Option<u32> {
        self.0.last().copied()
    }
}
