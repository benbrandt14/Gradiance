//! Registry validation — the compiler the DSL doesn't have: every advertised
//! op must be a bound VM symbol and every documented reflect path must
//! resolve. (What an operation writes is checked from its own output by
//! `op_conformance`.)

use crate::harness::headless_app;
use gradiance::domain::settings::{CameraSettings, SimSettings};
use gradiance::script::bridge::{OperationRegistry, ScriptInputs, ScriptLog};
use gradiance::script::registry::{CAMERA_SETTING_PATHS, SIM_SETTING_PATHS};

/// Every op the catalog advertises — the built-in rows and the registry
/// operations it projects — is actually bound in the VM: evaluating the bare
/// symbol must succeed (an unbound symbol is a steel eval error). This is the
/// direct catalog ↔ builtin drift check.
#[test]
fn every_catalog_op_is_a_bound_vm_symbol() {
    let mut app = headless_app();
    let catalog = app.world().resource::<OperationRegistry>().0.clone();
    assert!(
        catalog.find("spawn-box").is_some(),
        "the surfaced catalog lists the registry operations"
    );
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

/// Every documented `camera-get`/`camera-set` path resolves on the real
/// `CameraSettings` type.
#[test]
fn every_documented_camera_path_resolves() {
    let settings = CameraSettings::default();
    for path in CAMERA_SETTING_PATHS {
        assert!(
            gradiance::script::reflect_bridge::read_path(&settings, path).is_some(),
            "documented camera path `{path}` does not resolve on CameraSettings"
        );
    }
}
