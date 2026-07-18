# UI shell decision: stay on egui, build an app shell

Status: **accepted** (2026-07-18). Prompted by the question of whether a growing
desktop app should move off egui or add other tooling to stay manageable.

## Decision

**Keep egui as the sole UI framework**, and invest in it as a proper *app shell*
rather than a scatter of windows: an `egui_tiles` dock/tab workspace, a
menu/action system, a view registry, and persisted layout. Reach for engineering
hygiene — not a framework swap — to manage growth: a workspace crate split
(`gradiance-core` / `-physics` / `-ui`) when compile times bite, which the
existing `tests/boundaries.rs` module discipline already prepares.

## Why

The central artifact is a **live wgpu-rendered viewport** (the Bevy scene). Any
UI framework must compose with in-process wgpu rendering and route input to it.

- **egui composes natively** via `bevy_egui` (already the whole editor). It is
  immediate-mode — weaker at deeply-styled/animated chrome — but strong at the
  dense inspector/tool/graph/plot UI this app is made of.
- **Rerun is the existence proof.** A large, professional, viewport-centric
  desktop app built on egui + `egui_tiles` + wgpu, with docking, timelines,
  plots, and 3D views. egui scales to exactly this shape of app; the ceiling we
  were feeling is *architecture* (ad-hoc windows), not the toolkit.
- **The alternatives all pay the same tax.** Web/Tauri, Slint, and Dioxus are
  capable app frameworks, but each would require **embedding a live GPU viewport
  inside a foreign UI runtime** — render-to-texture + stream, or a native child
  window, plus input/DPI/lifecycle bridging across two runtimes. That is the
  single hardest integration in this project and a full rewrite of the editor,
  with permanent two-runtime complexity. It is not justified by the current
  pain, which a shell architecture resolves.

## Scope / what changes

- Adopt `egui_tiles 0.16` (egui-0.35-compatible) for a dockable/tabbable
  workspace; a File/Edit/View/Help menu bar; bevy camera-viewport management so
  the scene keeps a central region under the dock. (Its own PR — see the roadmap
  "UI overhaul & desktop-app shell".)
- Treat panels as registered *views*, edits as *actions* (dovetails with the
  scripting operation registry — a menu action can be a registered op), and
  persist workspace layout as editor view-state (never in the scene RON).

## Revisit if

A concrete wall appears that egui + a shell cannot clear — e.g. rich text/vector
document editing, deep accessibility requirements, or a design system that egui's
immediate-mode styling genuinely cannot express. At that point the honest option
is a **hybrid** (a foreign-toolkit chrome around a native Bevy render pane), not
a from-scratch rewrite — and it would get its own decision record.
