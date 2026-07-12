//! Registry validation — "the compiler the DSL doesn't have".
//!
//! The operation catalog (`script/registry.rs`) is data; nothing forces it to
//! stay consistent with the steel builtins, the intent bus, or the reflection
//! registry. These tests do: every advertised op must be a bound VM symbol,
//! every Edit op must map to a registered + reflected intent type, and every
//! documented reflect path must resolve on the real settings type. They run in
//! plain `cargo test`, so the catalog cannot drift once the script surface
//! grows.

use crate::harness::headless_app;
use bevy::prelude::*;
use gradiance::domain::settings::SimSettings;
use gradiance::script::bridge::{IntentDispatch, ScriptInputs, ScriptLog, edit_bindings};
use gradiance::script::registry::{OpCategory, OperationCatalog, SIM_SETTING_PATHS};
use std::collections::BTreeSet;

/// Every op the catalog advertises is actually bound in the VM: evaluating
/// the bare symbol must succeed (an unbound symbol is a steel eval error).
/// This is the direct catalog ↔ builtin drift check.
#[test]
fn every_catalog_op_is_a_bound_vm_symbol() {
    let mut app = headless_app();
    let catalog = OperationCatalog::builtin();
    for op in catalog.ops() {
        app.world_mut()
            .resource_mut::<ScriptInputs>()
            .submit(op.name);
        app.update();
        let log = app.world().resource::<ScriptLog>();
        let entry = log.0.last().expect("script log has the run");
        assert!(
            entry.ok,
            "catalog advertises `{}` but the VM rejects it: {}",
            op.name, entry.output
        );
    }
}

/// The catalog's Edit ops and the intent-binding table are the same set, and
/// every bound intent type is registered on the dispatch bus. An Edit verb
/// that would silently drop its ops fails here instead.
#[test]
fn every_edit_op_maps_to_a_registered_intent() {
    let app = headless_app();
    let dispatch = app.world().resource::<IntentDispatch>();

    let catalog_edits: BTreeSet<&str> = OperationCatalog::builtin()
        .ops()
        .iter()
        .filter(|o| o.category == OpCategory::Edit)
        .map(|o| o.name)
        .collect();
    let bound_edits: BTreeSet<&str> = edit_bindings().iter().map(|b| b.op).collect();
    assert_eq!(
        catalog_edits, bound_edits,
        "catalog Edit ops and bridge edit_bindings() diverge"
    );

    for binding in edit_bindings() {
        assert!(
            dispatch.is_registered(binding.intent),
            "edit op `{}` emits {} but that intent is not registered on the bus",
            binding.op,
            binding.intent_path
        );
    }
}

/// Every intent type an Edit op emits resolves in the app's reflection
/// registry (scripting binds operations by reflected type).
#[test]
fn every_edit_intent_resolves_in_the_type_registry() {
    let app = headless_app();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    for binding in edit_bindings() {
        assert!(
            registry.get_with_type_path(binding.intent_path).is_some(),
            "intent type `{}` (op `{}`) is not in the type registry — \
             missing register_type in CommandPlugin?",
            binding.intent_path,
            binding.op
        );
    }
}

/// Every documented `sim-get`/`sim-set` reflect path resolves on the real
/// `SimSettings` type, so a field rename cannot strand a documented path.
#[test]
fn every_documented_sim_path_resolves() {
    let settings = SimSettings::default();
    for path in SIM_SETTING_PATHS {
        assert!(
            gradiance::script::reflect_bridge::read_path(&settings, path).is_some(),
            "documented sim path `{path}` does not resolve on SimSettings"
        );
    }
}
