//! The panel operations (`gradiance-op`, brief U-3b): `panel.show`,
//! `panel.hide`, `panel.toggle`, `panel.open` and `panel.list`. One
//! declaration each, so Lisp (`panel-show`, `panel-hide`, `panel-toggle`,
//! `panel-open?`, `panels`), the remote surface and later the CLI get them
//! with no code of their own.
//!
//! Panel visibility is editor state (CLAUDE.md invariant 4's exception), so
//! the writes are [`Seam::Editor`] and record no undo step. They do not touch
//! a panel: each lowers to a [`PanelRequest`] message, which
//! [`apply_panel_requests`] applies through
//! [`PanelToggle`](crate::panels::PanelToggle) — what a View-menu click does.
//! The reads, and the name check every operation makes, use a
//! [`PanelMirror`] built from [`Panels::named`](crate::menu::Panels::named)
//! when the context is built, so an unknown name is an error that lists the
//! known ones, at the call.
//!
//! The panel resources and the apply system are installed headless too, so a
//! headless run (tests, a remote client) has the same panels, only undrawn.

use crate::menu::Panels;
use bevy::prelude::*;
use gradiance_op::{
    OpContext, OpContextFillers, OpError, OpInvokeSet, OpMeta, OpOutput, OpRegistry, Operation,
    Seam, Value, json,
};
use serde::Deserialize;

/// Registers the panel operations, their mirror, the request message, the
/// panel resources and the system that applies the requests.
pub(crate) fn install(app: &mut App) {
    {
        let mut registry = app.world_mut().resource_mut::<OpRegistry>();
        registry.register::<PanelShow>();
        registry.register::<PanelHide>();
        registry.register::<PanelToggleOp>();
        registry.register::<PanelOpen>();
        registry.register::<PanelList>();
    }
    app.world_mut()
        .resource_mut::<OpContextFillers>()
        .add(fill_panel_mirror);
    app.add_message::<PanelRequest>();
    // Every resource `Panels` borrows, so the apply system and the mirror
    // work headless as well as in the window.
    app.init_resource::<crate::settings::SettingsWindow>();
    app.init_resource::<crate::inspector::InspectorPanel>();
    app.init_resource::<crate::depth_panel::DepthPanel>();
    app.init_resource::<crate::plot::PlotPanel>();
    app.init_resource::<crate::probe::ProbePanel>();
    app.init_resource::<crate::node_graph::NodeGraph>();
    app.init_resource::<crate::outliner::ObjectTreePanel>();
    app.init_resource::<crate::groups::GroupsPanel>();
    app.init_resource::<crate::console::ScriptConsole>();
    app.init_resource::<crate::array_panel::ArrayWindow>();
    app.init_resource::<crate::optimizer::OptimizerExpanded>();
    app.init_resource::<crate::optimizer::OptimizerRequests>();
    app.init_resource::<crate::review::ReviewWindow>();
    app.init_resource::<crate::search::SearchPanel>();
    app.init_resource::<crate::status_bar::StatusBar>();
    app.init_resource::<gradiance_domain::settings::DebugSettings>();
    // After the doorways that write requests, so a request applies the frame
    // it was made.
    app.add_systems(
        Update,
        apply_panel_requests
            .after(OpInvokeSet)
            .after(gradiance_script::bridge::run_scripts),
    );
}

/// A panel visibility change an operation asked for, applied by
/// [`apply_panel_requests`].
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct PanelRequest {
    /// The panel's name in [`Panels::named`](crate::menu::Panels::named).
    pub name: &'static str,
    /// `Some(true)` show, `Some(false)` hide, `None` flip.
    pub shown: Option<bool>,
}

/// Every panel's name and whether it is open, in View-menu order: the read
/// mirror the panel operations resolve names against.
#[derive(Debug, Clone, Default)]
pub struct PanelMirror(pub Vec<(&'static str, bool)>);

impl PanelMirror {
    /// The table name for `name` (trimmed, any case), or an error that lists
    /// the known names.
    fn resolve(&self, name: &str) -> Result<(&'static str, bool), OpError> {
        let key = name.trim().to_lowercase();
        self.0
            .iter()
            .find(|(known, _)| *known == key)
            .copied()
            .ok_or_else(|| {
                let known: Vec<&str> = self.0.iter().map(|(n, _)| *n).collect();
                OpError::Invalid(format!(
                    "no panel named `{name}`; the panels are {}",
                    known.join(", ")
                ))
            })
    }
}

/// Reads every panel's state through the one name table.
fn read_panels(mut panels: Panels) -> PanelMirror {
    PanelMirror(
        panels
            .named()
            .into_iter()
            .map(|(name, panel)| (name, panel.is_open()))
            .collect(),
    )
}

/// Publishes the panels' current state into a new operation context. Where
/// the panel resources are missing (an app without this plugin's install),
/// no mirror is published and the operations say so.
fn fill_panel_mirror(world: &mut World, ctx: &mut OpContext) {
    if let Ok(mirror) = world.run_system_cached(read_panels) {
        ctx.insert(mirror);
    }
}

/// Applies the panel requests the operations wrote, through
/// [`PanelToggle`](crate::panels::PanelToggle), as a menu click does. The
/// name was checked when the operation lowered; a name the table lost since
/// warns and is dropped.
pub fn apply_panel_requests(mut requests: MessageReader<PanelRequest>, mut panels: Panels) {
    for request in requests.read() {
        let mut table = panels.named();
        if let Some((_, panel)) = table.iter_mut().find(|(name, _)| *name == request.name) {
            match request.shown {
                Some(shown) => panel.set_open(shown),
                None => panel.toggle(),
            }
        } else {
            warn!("no panel named `{}`", request.name);
        }
    }
}

/// Lowers a visibility write for the panel `name`.
fn request(name: &str, shown: Option<bool>, ctx: &OpContext) -> Result<OpOutput, OpError> {
    let (name, _) = ctx.get::<PanelMirror>()?.resolve(name)?;
    Ok(OpOutput::none().message(PanelRequest { name, shown }))
}

/// Show an editor panel.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PanelShow {
    /// The panel's name, as `panel.list` gives it.
    pub name: String,
}

impl Operation for PanelShow {
    const META: OpMeta = OpMeta {
        id: "panel.show",
        lisp: "panel-show",
        doc: "Open an editor panel by name, as its View-menu row does (see panel.list for the names).",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "properties"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        request(&self.name, Some(true), ctx)
    }
}

/// Hide an editor panel.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PanelHide {
    /// The panel's name, as `panel.list` gives it.
    pub name: String,
}

impl Operation for PanelHide {
    const META: OpMeta = OpMeta {
        id: "panel.hide",
        lisp: "panel-hide",
        doc: "Close an editor panel by name.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "properties"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        request(&self.name, Some(false), ctx)
    }
}

/// Flip an editor panel's visibility. (Named so it does not shadow the
/// [`PanelToggle`](crate::panels::PanelToggle) trait.)
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PanelToggleOp {
    /// The panel's name, as `panel.list` gives it.
    pub name: String,
}

impl Operation for PanelToggleOp {
    const META: OpMeta = OpMeta {
        id: "panel.toggle",
        lisp: "panel-toggle",
        doc: "Flip an editor panel's visibility by name.",
        seam: Seam::Editor,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "properties"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        request(&self.name, None, ctx)
    }
}

/// Whether an editor panel is showing.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PanelOpen {
    /// The panel's name, as `panel.list` gives it.
    pub name: String,
}

impl Operation for PanelOpen {
    const META: OpMeta = OpMeta {
        id: "panel.open",
        lisp: "panel-open?",
        doc: "Whether an editor panel is shown, by name.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: r#"{"name": "properties"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let (_, open) = ctx.get::<PanelMirror>()?.resolve(&self.name)?;
        Ok(OpOutput::none().returning(open))
    }
}

/// Every editor panel and whether it is showing.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PanelList {}

impl Operation for PanelList {
    const META: OpMeta = OpMeta {
        id: "panel.list",
        lisp: "panels",
        doc: "Every editor panel in View-menu order: its name (what the panel operations take) and whether it is open.",
        seam: Seam::Query,
        menu: None,
        chord: None,
        undo: None,
        example: "{}",
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let rows: Vec<Value> = ctx
            .get::<PanelMirror>()?
            .0
            .iter()
            .map(|(name, open)| json!({"name": name, "open": open}))
            .collect();
        Ok(OpOutput::none().returning(Value::Array(rows)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mirror() -> PanelMirror {
        PanelMirror(vec![("outliner", false), ("properties", true)])
    }

    #[test]
    fn a_name_resolves_trimmed_and_in_any_case() {
        assert_eq!(
            mirror().resolve(" Properties ").ok(),
            Some(("properties", true))
        );
    }

    #[test]
    fn an_unknown_name_lists_the_known_ones() {
        let err = mirror().resolve("inspector").err().map(|e| e.to_string());
        assert!(
            err.as_deref()
                .is_some_and(|e| e.contains("`inspector`") && e.contains("outliner, properties")),
            "{err:?}"
        );
    }

    #[test]
    fn the_writes_lower_to_one_request_each() {
        let mut ctx = OpContext::new(gradiance_op::OpIdSource::Random);
        ctx.insert(mirror());
        let shown = PanelShow {
            name: "outliner".into(),
        }
        .lower(&ctx);
        assert!(shown.is_ok());
        let unknown = PanelToggleOp { name: "x".into() }.lower(&ctx);
        assert!(matches!(unknown, Err(OpError::Invalid(_))));
    }
}
