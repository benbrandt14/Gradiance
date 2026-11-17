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
| VectorCraft | input injection rules and budgets | — | not reachable: `tangency-labs/vectorcraft` asks for credentials (2026-11-17, brief U-4); U-4 took SolidCraft's injection rules instead. Compare when reachable |
| Graphite | node-definition metadata, for G-0 | — | not reviewed |

Licence note: SolidCraft's code is MIT OR Apache-2.0, except its brand
material (`crates/ui-kit`, `crates/look`, `crates/theme`, branding assets),
which its `LICENSE-BRAND` reserves. Port nothing from those folders.
