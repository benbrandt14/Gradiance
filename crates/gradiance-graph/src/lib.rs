//! The one graph IR (`docs/graph-model-decision.md`, brief G-1).
//!
//! The signal graph and parametric shapes share one typed node graph: nodes
//! carry a [`StableId`](gradiance_core::ids::StableId), a [`NodeKind`], their
//! input bindings and a saved canvas position; an edge is
//! `(source node id, output port index)`. Each kind has one catalog row
//! ([`NodeMeta`], in Graphite's vocabulary: `id`, `category`, `name`, doc,
//! ports with `default` and `hard` range) that also declares its
//! [`Family`] and [`Rate`]:
//!
//! - the **signal family** runs at **frame rate**. It lowers to one
//!   [`SignalExpr`](gradiance_domain::signal::SignalExpr) per published node
//!   ([`lower_signal`]), the input the signal runtime's kernel compiler
//!   already takes. A wire to a published node reads the bus by its label,
//!   so the bus and the 16-input kernel limit stay as they are.
//! - the **shape family** runs at **edit rate**. It lowers to a
//!   [`ShapeDef`](gradiance_domain::shape::ShapeDef) ([`lower_shape`]), the
//!   one geometry type every consumer reads.
//!
//! The **rate rule**: an edit-rate input never takes a frame-rate output
//! ([`GraphError::RateMismatch`]). A param is an edit-rate scalar that also
//! publishes each frame, so a slider can size a box and scale a gain, but a
//! body's speed can never resize a collider every frame. Frame-rate cycles
//! are legal (they read last frame's value, as the bus does today);
//! [`Graph::validate`] reports where each one is broken in its [`Order`].
//!
//! # The plane-local rule
//!
//! **Every value in the graph is plane-local.** Scalars, points and shapes
//! carry no world axis, like intents and records in the frames decision
//! (`docs/frames-decision.md`). No node kind reads or writes a world axis;
//! a world-axis quantity reaches the graph only as an external bus read,
//! through the quantity catalog in one place. A future 3D or datum-plane
//! value is a new [`PortType`] (a frame), not a world axis on an old one.
//!
//! # Layering
//!
//! Pure, in the sense of `gradiance-geometry`: no systems, no queries, no
//! `World`, no intents. The Bevy surface is `Reflect` on the IR and
//! `Resource` on [`Graph`]; nothing is registered here. Since G-2 the graph
//! is scene content: the signal layer registers the resource and compiles
//! it, and every change goes through [`Graph::edit`] (one [`GraphEdit`],
//! validated before it is kept). [`from_signals`] converts the old by-name
//! signal lists into a graph (tests build fixtures with it).

mod catalog;
mod convert;
mod edit;
mod ir;
mod lower;
mod validate;

#[cfg(test)]
mod tests;

pub use catalog::{CATALOG, NodeMeta, PortDefault, PortMeta, meta, nodes_markdown};
pub use edit::{EditError, GraphEdit, NodeRef, PortRef, port_index};
pub use convert::from_signals;
pub use ir::{Family, Graph, Input, Node, NodeKind, PortType, Rate, Value};
pub use lower::{lower_shape, lower_signal, lower_signals};
pub use validate::{Feedback, GraphError, Order};
