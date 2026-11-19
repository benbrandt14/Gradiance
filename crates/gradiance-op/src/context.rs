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

    /// The builder for a context that lowers no operation with a gated
    /// mirror ([`OpContextFillers::add_for`]): runs every ungated filler over
    /// the committed world. F3 search and the groups filter use it.
    pub fn build(world: &mut World) -> Self {
        Self::build_for(world, &|_| false)
    }

    /// The builder every invoking doorway calls: runs every ungated filler,
    /// plus each gated filler one of whose operations `invokes` names. A
    /// single call passes its own id; a script run the ids its source
    /// mentions. An operation lowered without its gated mirror fails with
    /// [`OpError::MissingMirror`], never with a stale value.
    pub fn build_for(world: &mut World, invokes: &dyn Fn(&str) -> bool) -> Self {
        let ids = world
            .get_resource::<OpIdSource>()
            .copied()
            .unwrap_or_default();
        let fillers = world
            .get_resource::<OpContextFillers>()
            .map_or_default(|f| f.0.clone());
        let mut ctx = Self::new(ids);
        for (gate, fill) in fillers {
            if gate.is_none_or(|ops| ops.iter().any(|id| invokes(id))) {
                fill(world, &mut ctx);
            }
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

/// The fillers [`OpContext::build_for`] runs, in registration order. A
/// gated filler names the operations that read its mirror.
#[derive(Resource, Default)]
pub struct OpContextFillers(pub(crate) Vec<(Option<&'static [&'static str]>, OpContextFiller)>);

impl OpContextFillers {
    /// Adds one layer's filler, run for every context.
    pub fn add(&mut self, filler: OpContextFiller) {
        self.0.push((None, filler));
    }

    /// Adds a filler run only for a context that may lower one of `ops`
    /// (operation ids): for a mirror too costly to build on every call
    /// (`scene.dump`'s whole-scene record, brief R-1).
    pub fn add_for(&mut self, ops: &'static [&'static str], filler: OpContextFiller) {
        self.0.push((Some(ops), filler));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Costly;

    #[test]
    fn a_gated_filler_runs_only_for_its_operations() {
        let mut world = World::new();
        let mut fillers = OpContextFillers::default();
        fillers.add(|_, ctx| ctx.insert(1_u8));
        fillers.add_for(&["scene.dump"], |_, ctx| ctx.insert(Costly));
        world.insert_resource(fillers);

        let plain = OpContext::build(&mut world);
        assert!(plain.get::<u8>().is_ok());
        assert!(plain.get::<Costly>().is_err());

        let other = OpContext::build_for(&mut world, &|id| id == "body.count");
        assert!(other.get::<u8>().is_ok());
        assert!(other.get::<Costly>().is_err());

        let dump = OpContext::build_for(&mut world, &|id| id == "scene.dump");
        assert_eq!(dump.get::<Costly>().ok(), Some(&Costly));
        assert!(dump.get::<u8>().is_ok());
    }

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
