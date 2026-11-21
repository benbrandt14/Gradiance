# Upstream watch

The upstreams Gradiance ports from (`docs/unified-surface-plan.md` § 6),
what to watch in each, and the last revision reviewed. Ported files and
their local changes are listed in `vendor/UPSTREAM.toml`;
`tools/upstream-diff.sh <name>` shows what upstream changed since. The
orchestrator runs it at each phase gate and writes a brief when a change is
worth taking.

| Upstream | Watch | Ported | Last reviewed |
|---|---|---|---|
| SolidCraft (`https://github.com/tangency-labs/solidcraft`) | `crates/ui-registry` (widget registry); `crates/control-egui/src/screenshot.rs` (settle rule, ported as a rule in `ui::host`, not as a file); `crates/ui-egui/src/control.rs` `ui_click`/`ui_drag` and `lib.rs` `synthetic_input` (injection rules, ported as rules in `ui::host::input`); `crates/control`, `crates/mcp` and `docs/control-protocol.md` for U-6; the `SOLIDCRAFT_REQUIRE_GPU` rule (`docs/render-path.md`; ported as a rule, `GRADIANCE_REQUIRE_GPU`, brief U-5) | `ui_registry` | `0f5a5e2` (2026-11-17, brief U-3); injection rules at `a39b74e` (2026-11-17, brief U-4); GPU-gate rule at `e424409` (2026-11-17, brief U-5) |
| craft-libs | `crates/ui` (`testing`: the `Screens` snapshot harness) | — | not reachable: `git ls-remote https://github.com/tangency-labs/craft-libs.git` asks for credentials (private or absent; 2026-11-17, briefs U-4 and U-5). U-5 built its snapshot harness natively on `egui_kittest` (`tests/it/ui_snapshots.rs`); compare with `Screens` when reachable |
| VectorCraft (`https://github.com/storytold/vectorcraft`, public; the `tangency-labs` path was wrong) | input injection rules and budgets (`docs/control-protocol.md`), `egui_kittest` UI tests (B-0); smart guides, path model and tools, corners, contour, dimensions, repeats, blend, task bar, history, ruler guides — the feature map in `docs/vectorcraft-survey.md` (phases C, E, H, T, B) | — | `9fff48d` (2026-11-21, survey; nothing ported yet) |
| Graphite | node-definition metadata (`node-macro`, `graph-craft`); G-0 adopted the names `category`, `name`, `default`, `hard` from `docs/unified-surface-plan.md` § 5–6 only; the source review is phase G-6 (`docs/graph-model-decision.md`) | — | not reviewed |

Licence note: VectorCraft's code is MIT OR Apache-2.0 except the ArtCraft
name, logos and `docs/brand/`, which are reserved. SolidCraft's code is MIT OR Apache-2.0, except its brand
material (`crates/ui-kit`, `crates/look`, `crates/theme`, branding assets),
which its `LICENSE-BRAND` reserves. Port nothing from those folders.

## Dependency notes (not ported from; behaviour to recheck on upgrade)

- **avian2d 0.8 — standing joint correction reported as velocity** (brief
  Q-7, 2026-11-21). A chain pinned to the world at both ends settles into
  its catenary, but the link at each pin keeps a steady *reported*
  `LinearVelocity`/`AngularVelocity` (0.44 m/s and 1.17 rad/s at the left
  pin) while its pose is frozen to 1e-7 m; that hinge stays stretched
  1.5 mm under the chain's weight. Ruled out on the Gradiance side: air
  drag (switching it off changes nothing), sleeping (not asleep), contacts
  (none), the world pin as `ChildOf` the joint (top-level: identical).
  Repro: `tests/it/chains.rs` `a_settled_chain_has_no_buzzing_link`
  (`#[ignore]`). Recheck on each avian upgrade; a reader of link speed
  (plots, readouts) sees the phantom value.
