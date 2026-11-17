//! Files ported from fast-moving upstreams, kept in **upstream shape**
//! (`docs/unified-surface-plan.md` § 6): upstream's file, item names and
//! layout, so `tools/upstream-diff.sh` can show what upstream changed since
//! the recorded revision. Each file's revision and local changes are listed
//! in `vendor/UPSTREAM.toml` at the repository root.
//!
//! Bevy-specific code goes in a thin adapter next to its user (for the
//! widget registry: [`crate::widget_registry`]), never in a ported file.
//! The files are excluded from rustfmt and from the pedantic lints, which
//! would otherwise rewrite them away from upstream's text.

pub mod solidcraft;
