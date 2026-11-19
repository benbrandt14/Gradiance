//! Packing as operations (brief R-4): `pack.run`, `pack.status`,
//! `pack.apply`, `pack.cancel`, `pack.get` and `pack.set`.
//!
//! `pack.run` resolves its targets against the context's scene snapshot and
//! writes one [`StartPackRequest`] holding their ids; the session in
//! [`crate::pack`] then runs over the next frames and commits exactly one
//! `commit-transform` step when it finishes (or at `pack.apply` when
//! `auto_apply` is off), as the Optimizer panel's runs always have. The
//! panel and the context menu invoke the same operation with
//! `"selection"`, so a panel run and a script run over the same bodies and
//! the same [`PackConfig`] build one problem and commit one layout.
//!
//! [`PackConfig`] is workstation config (never saved, never undone), so
//! `pack.set` writes it through the config seam like `camera.set`.

use bevy::prelude::*;
use gradiance_command::intent::name;
use gradiance_command::snapshot::SceneSnapshot;
use gradiance_op::reflect_path::read_path;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target,
    Value, json,
};
use gradiance_optimize::{Metrics, PackConfig, PackReport};
use serde::Deserialize;

use crate::pack::{PackControl, PackSession, StartPackRequest};

/// Registers the packing operations and their mirrors.
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<PackRun>();
        registry.register::<PackStatus>();
        registry.register::<PackApply>();
        registry.register::<PackCancel>();
        registry.register::<PackGet>();
        registry.register::<PackSet>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_pack_mirror);
}

/// The session as the operations read it: a context mirror, built only when
/// an operation context is (never per frame).
#[derive(Debug, Clone)]
pub struct PackMirror {
    /// `pack.status`'s value.
    pub status: Value,
}

/// Publishes the rulebook and the session's status.
fn fill_pack_mirror(world: &mut World, ctx: &mut OpContext) {
    if let Some(config) = world.get_resource::<PackConfig>() {
        ctx.insert(config.clone());
    }
    if let Some(session) = world.get_resource::<PackSession>() {
        ctx.insert(PackMirror {
            status: status_value(session),
        });
    }
}

/// One score as JSON.
fn metrics_value(m: &Metrics) -> Value {
    json!({
        "extent": m.extent,
        "fill": m.fill,
        "overlap": m.overlap,
        "violations": m.violations,
        "min_gap": m.min_gap,
        "bounds": [[m.bounds.0.x, m.bounds.0.y], [m.bounds.1.x, m.bounds.1.y]],
    })
}

/// A report as JSON, with the run's `state` first.
fn report_value(state: &str, report: &PackReport, bodies: usize) -> Value {
    json!({
        "state": state,
        "solver": report.solver,
        "status": report.status.label(),
        "bodies": bodies,
        "iteration": report.iteration,
        "total_iterations": report.total_iterations,
        "max_iterations": report.max_iterations,
        "progress": report.progress(),
        "shrinkage": report.shrinkage(),
        "start": metrics_value(&report.start),
        "best": metrics_value(&report.best),
    })
}

/// `pack.status`'s value: `state` is `idle` (no run since start-up or since
/// a start with fewer than two bodies), `running`, `finished` (done, held
/// for `pack.apply` because `auto_apply` is off), `applied` or
/// `cancelled`; the last three carry the run's report.
pub fn status_value(session: &PackSession) -> Value {
    if let Some(report) = session.report() {
        let state = if report.status.is_done() {
            "finished"
        } else {
            "running"
        };
        return report_value(state, &report, session.body_count());
    }
    match session.last_outcome() {
        Some(outcome) => {
            let state = if outcome.applied {
                "applied"
            } else {
                "cancelled"
            };
            report_value(state, &outcome.report, outcome.moved)
        }
        None => json!({"state": "idle"}),
    }
}

/// Arrange the listed bodies with the layout solver.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PackRun {
    /// The bodies: a list of targets (`(list i …)` in Lisp; `"selection"`
    /// from the panel).
    pub targets: Vec<Target>,
}

impl Operation for PackRun {
    const META: OpMeta = OpMeta {
        id: "pack.run",
        lisp: "pack",
        doc: "Start the layout solver on the listed bodies (id order; the Optimizer's rulebook, pack-set) and commit their new poses as one commit-transform step when it finishes (at pack-apply when auto_apply is off). Returns at once; read pack-status. Fewer than two distinct bodies is a no-op.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::COMMIT_TRANSFORM),
        example: r#"{"targets": [0, 1, 2]}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let targets = gradiance_command::ops::body_set(ctx, scene, &self.targets)?;
        if targets.len() < 2 {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(StartPackRequest { targets }))
    }
}

/// The packing run's state and report.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PackStatus {}

impl Operation for PackStatus {
    const META: OpMeta = OpMeta {
        id: "pack.status",
        lisp: "pack-status",
        doc: "The packing run as an object: state (idle, running, finished, applied, cancelled) and, past idle, the solver, status, bodies, iterations, progress, shrinkage and the start and best scores (extent, fill, overlap, violations, min_gap, bounds).",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let mirror = ctx.get::<PackMirror>()?;
        Ok(OpOutput::none().returning(mirror.status.clone()))
    }
}

/// Apply the best layout found so far.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PackApply {}

impl Operation for PackApply {
    const META: OpMeta = OpMeta {
        id: "pack.apply",
        lisp: "pack-apply",
        doc: "Apply the packing run's best layout now and end it (the Optimizer's Apply): that is pack's commit-transform step; nothing when no run is loaded.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(PackControl::Apply))
    }
}

/// Throw the packing run away.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PackCancel {}

impl Operation for PackCancel {
    const META: OpMeta = OpMeta {
        id: "pack.cancel",
        lisp: "pack-cancel",
        doc: "End the packing run and leave every body where it was (the Optimizer's Cancel, Escape). No undo step.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, _ctx: &OpContext) -> Result<OpOutput, OpError> {
        Ok(OpOutput::none().message(PackControl::Cancel))
    }
}

/// Read one number of the packing rulebook.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PackGet {
    /// The reflect-path of a scalar in `PackConfig`.
    pub path: String,
}

impl Operation for PackGet {
    const META: OpMeta = OpMeta {
        id: "pack.get",
        lisp: "pack-get",
        doc: "Read a number of the packing rulebook by reflect-path (\"clearance\", \"max_iterations\", \"weights.extent\", …); NaN if the path does not resolve to a number.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"path": "clearance"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let config = ctx.get::<PackConfig>()?;
        Ok(OpOutput::none().returning(read_path(config, &self.path).unwrap_or(f64::NAN)))
    }
}

/// Set one number or switch of the packing rulebook.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PackSet {
    /// The reflect-path of a scalar or bool in `PackConfig`.
    pub path: String,
    /// The new value (0 / 1 for a switch such as `auto_apply`).
    pub value: f64,
}

impl Operation for PackSet {
    const META: OpMeta = OpMeta {
        id: "pack.set",
        lisp: "pack-set",
        doc: "Set a number or switch (0/1) of the packing rulebook by reflect-path, as the Optimizer window does (workstation config, never saved or undone).",
        seam: Seam::Config,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"path": "clearance", "value": 0.05}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let config = ctx.get::<PackConfig>()?;
        Ok(OpOutput::none().setting(config, &self.path, self.value)?)
    }
}
