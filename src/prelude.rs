//! Common imports for Gradiance.
//!
//! Re-exports commonly used types from Bevy, Rapier, and internal modules.

// TODO: Structure prelude to re-export internal sub-modules (e.g. `pub mod physics { pub use crate::physics::prelude::*; }`)
// to create a self-documenting and easy-to-use API surface.
pub use bevy::prelude::*;
pub use bevy_rapier2d::prelude::Real;
pub use bevy_rapier2d::prelude::*;
pub use crate::GameState;

/// Strong ID wrapper for persistent references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub Entity);
