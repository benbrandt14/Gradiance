//! Engineering units: typed SI quantities and the one world-scale seam.
//!
//! Every physical value the tool authors, simulates, or plots is a **typed
//! quantity** — a `#[repr(transparent)]` newtype over `f32` holding a value
//! in the base SI unit ([`Length`] in metres, [`Mass`] in kilograms,
//! [`Force`] in newtons, …). The unit stops being a doc comment and becomes a
//! type: a `m/s` value cannot flow into a `m/s²` slot, and the reviewer sees
//! the dimension.
//!
//! # Reflection
//!
//! The quantities derive [`Reflect`](bevy::reflect::Reflect) so they live
//! directly in authored components and read through the scripting reflection
//! bridge. That bridge unwraps a single-field newtype to its inner scalar
//! (units P0), so `(mass . 2.5)` is what a script or plotter sees — not an
//! opaque handle. This is why the crate carries a minimal `bevy` surface (like
//! `gradiance-geometry`) rather than being fully pure like `gradiance-kernel`.
//!
//! # The world seam
//!
//! The simulation and renderer work in **world units** (pixels); SI is the
//! authored/queried representation. [`world`] is the *single* conversion
//! point ([`PIXELS_PER_METER`](world::PIXELS_PER_METER)) — no other module
//! multiplies by the pixel scale.
//!
//! # 2D density
//!
//! [`Density`] is areal (kg/m²) this iteration, and mass is computed in one
//! place, [`mass_of`]. A future 3D jump is that one function plus one
//! [`dimension`] line — see `docs/units-decision.md`.

pub mod dimension;
pub mod mass;
pub mod quantity;
pub mod world;

pub use dimension::Dimension;
pub use mass::mass_of;
pub use quantity::{
    Acceleration, Acceleration2, Angle, AngularAcceleration, AngularImpulse, AngularMomentum,
    AngularVelocity, Area, Damping, Density, Displacement, Energy, Force, Force2, Frequency,
    Impulse, Impulse2, Length, Mass, MomentOfInertia, Momentum, Stiffness, Time, Torque, Velocity,
    Velocity2,
};
