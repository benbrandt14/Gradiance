# UI shell decision: options for a growing desktop app

Status: **under evaluation** (reopened 2026-07-18). Prompted by: "I am open to
other options for UI other than purely egui, or other assistance to manage what
is becoming a larger desktop application." This doc is deliberately *not* a
foregone conclusion — it lays out the real options, their honest costs, and a
way to decide with evidence rather than assertion.

Two separable questions:

1. **Framework** — keep egui, or move (partly/wholly) to something richer?
2. **Manage the growth** — what makes a larger app tractable *regardless* of the
   framework answer?

## The one hard constraint

The app's heart is a **live wgpu-rendered viewport** (Bevy's render graph owns a
`wgpu::Device`/`Queue` and a winit window). Every option is really a question of
*how the UI toolkit coexists with that*. That — not widget richness — is what
separates "a weekend" from "an R&D project."

## Framework options (honest trade-offs)

- **A. egui as an app shell (lowest risk).** Keep `bevy_egui`; add `egui_tiles`
  (Rerun's docking, egui-0.35-ready), a menu/action system, a view registry,
  persisted layout. *Pros:* zero new integration risk, the whole editor already
  runs on it, and Rerun proves egui scales to a large viewport-centric desktop
  tool. *Cons:* immediate-mode ceilings — deeply-styled/animated chrome,
  theming, accessibility, and very complex custom widgets are more work than in
  a retained toolkit.

- **B. Hybrid: a retained native shell hosting the Bevy viewport (richest, real
  R&D).** The serious "move off egui" path.
  - **Slint + wgpu** is the most promising: Slint 1.x exposes custom-wgpu
    rendering (underlay/overlay via a rendering notifier, and an existing-device
    path), so compositing a Bevy-rendered texture into a styled, declarative,
    animated Slint UI is *conceivable* — but it means sharing/compositing across
    two render loops and routing input between them. Weeks, not days, plus
    ongoing maintenance as both projects move.
  - **Web frontend (Tauri/wry) or Dioxus** gives the best ergonomics for dense
    tooling and things like curve editors (SVG/canvas), but embedding a live GPU
    viewport in a webview is the hardest variant (native child surface or
    frame-streaming) and adds a permanent two-runtime data-sync cost.
  - **gpui** (Zed) is GPU-native and capable but young and has no established
    Bevy integration.

- **C. `bevy_ui` (no second runtime).** ECS-native, no integration seam at all.
  *But* it lacks docking and rich editor widgets today and is verbose for
  inspectors/node graphs — not ready for this tool's chrome. Worth re-checking
  each Bevy release.

## Recommended way to decide (not a bet)

If the pull toward a richer shell is real, **de-risk it with a time-boxed spike**
(≈2–3 days): stand up a minimal **Slint window compositing one Bevy-rendered
texture** with basic input routing. If that comes up clean, a phased migration
(shell first, panels ported behind a `View` trait) becomes credible. If it
fights the two render loops, we have concrete evidence to stay on the egui shell
— and we lost only the spike. I can run this spike on request.

Absent that signal, **option A is the low-risk default** and unblocks everything
else now.

## Manage the growth — no-regret, framework-independent

These pay off under *any* framework answer and are the highest-leverage
"manage the larger app" moves:

- **Split into a Cargo workspace** — `gradiance-core` (domain/geometry/pure),
  `-physics`, `-script`, `-ui`, and a thin app binary. **This is the biggest
  immediate win:** it cuts incremental compile times (the real drag on a growing
  app), enforces the module boundaries at the *crate* level (stronger than
  today's `tests/boundaries.rs`), and lets a future UI shell be swapped without
  touching core. The existing boundary discipline means the seams are already
  where crate edges would go.
- **App-shell architecture inside the UI** — a `View`/`Panel` trait + registry,
  an action/command layer (dovetails with the scripting **operation registry** —
  a menu item is a registered op), and persisted workspace layout. Makes chrome
  extensible instead of ad-hoc, and is what makes any later framework swap a
  port rather than a rewrite.
- **Iteration + regression safety** — keep the `dev` dynamic-linking loop; widen
  `egui_kittest` panel snapshot coverage so UI logic has headless guards (the
  environment can't screenshot).

## Standing recommendation

Pursue the **no-regret growth work now** (start with the workspace split), keep
building features on the **egui shell**, and **spike the Slint hybrid** before
committing to any framework move. Revisit this doc when the spike returns or a
concrete wall (rich-text/document editing, deep accessibility, a design system
egui can't express) forces the hybrid.

## Progress on option A (the egui app shell)

Being built by accretion, not big-bang:

- **Menu/action bar** — File/Edit/View/Help routing to existing intents/toggles.
- **Right dock → `egui_tiles`** (`src/ui/dock.rs`) — Depth/Signals/Script as
  re-arrangeable tabs; the first `egui_tiles` surface, proving the docking lib
  composes with `bevy_egui`.
- **Bottom dock → `egui_tiles`** (`src/ui/bottom_dock.rs`) — Node Graph + Live
  Plot as tabs, routed to their self-contained section renderers; the plot's
  floating window and the node graph's fixed bottom panel are unified into one
  workspace. Same open-set/persist-layout pattern as the right dock.

Both docks overlay the full-window scene on the background layer and feed
`PanelRects` (no camera-viewport change yet). **Next:** the scene itself becomes
a dockable pane with the Bevy camera rendering into that pane's rect — the
riskiest stage (picking/gizmo/scale + `PanelRects` all move), best done with
visual verification.
