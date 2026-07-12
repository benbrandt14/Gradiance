# Recipe audit — files touched vs. the recipe's ideal

Measured from `git show --stat` of the most recent feature of each recipe
kind (`docs/agent-context.md` §6). `src/` files only; tests and roadmap
bookkeeping not counted against the ideal.

| Recipe | Most recent feature | Ideal (src) | Actual (src) | Verdict |
|---|---|---|---|---|
| New command | `MergeCommand` (`b648ba2`) | 4 (+1 emitter) | 5 | on budget |
| New tool | strut tool (`9553ab7`) | 2 (+hotkey +button) | 5 | **ceremony found**: `input.rs` needed 3 touches (variant/match/array) — removed in the de-smell pass (`EditorAction::Tool(ToolState)`); now 1 touch (the binding) |
| New joint | `JointKind::Spring` (`9553ab7`) | 5 | 6 | on budget (1-line debug hooks); its perceived weight is joint+tool stacked (two recipes) |
| New plottable | joint length/angle (`9729bb3`) | 2 | 1 | **seam drift**: signals computed in `ui/plot.rs`, not `physics::queries` — invisible to scripts/probes. Follow-up: move behind the facade when the next signal lands |
| New script verb | `spawn-ground` (`33ce055`) | 3 | 3 | on budget; now CI-guarded by `tests/it/registry_validation.rs` |

The recipes are honest. One ceremony fix (tool triple) executed; one seam
drift (plot signals) logged in `docs/desmell-log.md` rather than churned now.
