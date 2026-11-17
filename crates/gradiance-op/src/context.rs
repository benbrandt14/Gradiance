//! [`OpContext`]: what an operation may read while it lowers.

use crate::registry::OpError;
use crate::target::SelectionMirror;
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// Where new [`StableId`]s come from. An operation that creates an object
/// takes its id from the context ([`OpContext::new_id`]), never from
/// `StableId::new()`, so two doorways can lower one call to equal outputs.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpIdSource {
    /// Fresh random ids (the app's behaviour).
    #[default]
    Random,
    /// A fixed sequence that starts at this number and restarts with every
    /// context. For tests that compare the outputs of two doorways.
    Sequence(u64),
}

/// A type-keyed container of **read-only mirrors**, plus the source of new
/// ids. It names no mirror type: each layer publishes its own (the command
/// layer its scene snapshot, the UI its panel states) through
/// [`OpContextFillers`].
///
/// The Lisp doorway builds one context per run pass (scripting rule 3: one
/// snapshot per run). Every other doorway builds one per invocation.
pub struct OpContext {
    mirrors: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    ids: OpIdSource,
    minted: AtomicU64,
}

impl OpContext {
    /// An empty context that mints ids from `ids`.
    pub fn new(ids: OpIdSource) -> Self {
        Self {
            mirrors: HashMap::new(),
            ids,
            minted: AtomicU64::new(0),
        }
    }

    /// The one builder every doorway calls: runs each layer's filler over the
    /// committed world.
    pub fn build(world: &mut World) -> Self {
        let ids = world
            .get_resource::<OpIdSource>()
            .copied()
            .unwrap_or_default();
        let fillers = world
            .get_resource::<OpContextFillers>()
            .map_or_default(|f| f.0.clone());
        let mut ctx = Self::new(ids);
        for fill in fillers {
            fill(world, &mut ctx);
        }
        ctx
    }

    /// Publishes (or replaces) the mirror of type `T`.
    pub fn insert<T: Any + Send + Sync>(&mut self, mirror: T) {
        self.mirrors.insert(TypeId::of::<T>(), Arc::new(mirror));
    }

    /// Withdraws the mirror of type `T`: for a doorway that must not read it
    /// (the Lisp doorway withdraws the [`SelectionMirror`]).
    pub fn remove<T: Any + Send + Sync>(&mut self) {
        self.mirrors.remove(&TypeId::of::<T>());
    }

    /// The selection, for a [`Target::Selection`](crate::Target::Selection).
    /// Where no layer published one (the Lisp doorway), the error says how to
    /// name objects instead.
    pub fn selection(&self) -> Result<&SelectionMirror, OpError> {
        self.get::<SelectionMirror>()
            .map_err(|_| OpError::NoSelection)
    }

    /// The mirror of type `T`.
    pub fn get<T: Any + Send + Sync>(&self) -> Result<&T, OpError> {
        self.mirrors
            .get(&TypeId::of::<T>())
            .and_then(|mirror| mirror.downcast_ref::<T>())
            .ok_or(OpError::MissingMirror(type_name::<T>()))
    }

    /// The mirror of type `T`, shared: for a doorway that also serves reads
    /// outside the registry from the same value.
    pub fn get_shared<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.mirrors
            .get(&TypeId::of::<T>())
            .and_then(|mirror| Arc::clone(mirror).downcast::<T>().ok())
    }

    /// The id for an object this operation creates.
    pub fn new_id(&self) -> StableId {
        let n = self.minted.fetch_add(1, Ordering::Relaxed);
        match self.ids {
            OpIdSource::Random => StableId::new(),
            OpIdSource::Sequence(start) => {
                StableId(Uuid::from_u128(u128::from(start) + u128::from(n)))
            }
        }
    }
}

/// One layer's contribution to a new [`OpContext`].
pub type OpContextFiller = fn(&mut World, &mut OpContext);

/// The fillers [`OpContext::build`] runs, in registration order.
#[derive(Resource, Default)]
pub struct OpContextFillers(pub(crate) Vec<OpContextFiller>);

impl OpContextFillers {
    /// Adds one layer's filler.
    pub fn add(&mut self, filler: OpContextFiller) {
        self.0.push(filler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sequence_restarts_with_every_context() {
        let first = OpContext::new(OpIdSource::Sequence(40));
        let second = OpContext::new(OpIdSource::Sequence(40));
        assert_eq!(first.new_id(), second.new_id());
        assert_eq!(first.new_id(), StableId(Uuid::from_u128(41)));
    }

    #[test]
    fn a_withdrawn_selection_is_an_error_that_says_what_to_use() {
        let mut ctx = OpContext::new(OpIdSource::Random);
        ctx.insert(SelectionMirror::default());
        assert!(ctx.selection().is_ok());
        ctx.remove::<SelectionMirror>();
        let err = ctx.selection().err().map(|e| e.to_string());
        assert!(err.is_some_and(|text| text.contains("index or id")));
    }

    #[test]
    fn a_missing_mirror_is_an_error_that_names_the_type() {
        let mut ctx = OpContext::new(OpIdSource::Random);
        ctx.insert(7_u32);
        assert_eq!(ctx.get::<u32>().ok(), Some(&7));
        let err = ctx.get::<String>().err().map(|e| e.to_string());
        assert!(err.is_some_and(|text| text.contains("String")));
    }
}
