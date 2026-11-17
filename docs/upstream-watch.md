# Upstream watch

The upstreams Gradiance ports from (`docs/unified-surface-plan.md` § 6),
what to watch in each, and the last revision reviewed. Ported files and
their local changes are listed in `vendor/UPSTREAM.toml`;
`tools/upstream-diff.sh <name>` shows what upstream changed since. The
orchestrator runs it at each phase gate and writes a brief when a change is
worth taking.

| Upstream | Watch | Ported | Last reviewed |
|---|---|---|---|
| SolidCraft (`https://github.com/tangency-labs/solidcraft`) | `crates/ui-registry` (widget registry); `crates/control-egui/src/screenshot.rs` (settle rule, ported as a rule in `ui::host`, not as a file); `crates/control`, `crates/mcp` and `docs/control-protocol.md` for U-4 and U-6 | `ui_registry` | `0f5a5e2` (2026-11-17, brief U-3) |
| craft-libs | `crates/ui` (`testing`, snapshots) for U-5 | — | repository not confirmed (a GitHub search on 2026-11-17 found only unrelated `craft-libs` repos; check the `tangency-labs` organisation's other repos) |
| VectorCraft | input injection rules and budgets, for U-4 | — | not searched yet (U-3 probed SolidCraft only) |
| Graphite | node-definition metadata, for G-0 | — | not reviewed |

Licence note: SolidCraft's code is MIT OR Apache-2.0, except its brand
material (`crates/ui-kit`, `crates/look`, `crates/theme`, branding assets),
which its `LICENSE-BRAND` reserves. Port nothing from those folders.
