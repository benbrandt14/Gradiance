//! Selection grouping.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// Bodies sharing a group id are selected together.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SelectionGroup(pub u32);
