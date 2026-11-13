# A runaway script can't hang the editor
Phase 3 · Scripting P0/P1 remainder · "a fuel/step budget for runaway
authoring scripts" · status: done

## Goal

A script that never ends (`(define (f) (f)) (f)`, a `while #t` loop, a huge
`range`) is stopped after a bounded wall-clock budget. The console reports
it as a script error ("stopped after N ms: budget exceeded"), and the editor
keeps running. Nothing the stopped run queued is applied: a run is all or
nothing, like its one undo batch.

## Read first

- `crates/gradiance-script/src/bridge.rs` ≈L700–725 (`ScriptEngine::run`:
  `catch_unwind` around `engine.run`), the exclusive `run_scripts` system
  (it drains the edit / config / label / panel queues after each run), and
  `ScriptError`.
- steel-core 0.8.2 (`~/.cargo/registry/src/*/steel-core-0.8.2/`):
  `src/steel_vm/engine.rs` ≈L1546 `Engine::with_interrupted(Arc<AtomicBool>)`,
  `src/steel_vm/interrupt.rs` (`InterruptHandler::new(engine, timeout)`, a
  watchdog thread on the engine's `ThreadStateController`), and `vm.rs`
  ≈L400 / L923 (where the flag is checked).
- CLAUDE.md § Scripting, the two-tier rule: the VM is cold-path only, so a
  budget is a guard, not a scheduler.
- `docs/scripting.md` § Running scripts.

## Facts (orchestrator, 2026-11-12, main `f1bd6ca`)

- steel exposes two mechanisms. `with_interrupted` takes a shared
  `AtomicBool` the VM polls; `InterruptHandler` spawns a thread that trips
  the engine after a timeout. *Unverified:* how often the VM polls the flag
  (per instruction, per call, per loop back-edge?), whether a tight native
  builtin loop is interruptible, and whether the engine is reusable after an
  interrupt. Test all three before choosing.
- Today a run that panics becomes `ScriptError::Panicked`. Steel errors
  become `ScriptError::Eval`. An interrupt should be a distinct variant so
  the console can say what happened.
- Queued effects: the verbs push intents and config writes into shared
  queues that `run_scripts` drains **after** `engine.run` returns.
  *Unverified:* whether a failed run's partial queue is drained today. If it
  is, a half-run script commits half its edits. Decide all-or-nothing
  explicitly and test it; the goal above says all-or-nothing.
- `--script` startup files and hot reloads go through the same `run`, so
  they get the budget too.

## Approach

1. Prove the mechanism with tests on a bare steel `Engine` (no ECS): an
   infinite tail loop, deep non-tail recursion, and a long `map` over a big
   list each stop within the budget; the engine still evaluates `(+ 1 2)`
   afterwards.
2. `ScriptEngine::run` arms the budget per run; a new
   `ScriptError::Budget { ms }`. The budget is a constant (suggest 2 s) or a
   field on an existing editor-config resource; don't add a settings panel.
3. All-or-nothing: on any error (eval, panic, budget) the run's queued
   edits, config writes, labels, panel requests and signal publishes are
   discarded, not drained. Test it.
4. The console shows the error like any other script error.

Invariants: 1 (intents still flow only from a successful run, through the
existing seams); the two-tier rule (nothing new in the frame loop beyond
arming and clearing a flag).

## Out of scope

Running scripts off the main thread, async or cooperative scheduling,
per-verb quotas, memory limits, `docs/state.md`.

## Done when

- Unit tests on the bare engine (the three loop shapes; engine reusable).
- Integration tests: an infinite script submitted through `ScriptInputs`
  yields a budget error and leaves the scene unchanged; a script that spawns
  then loops forever spawns nothing.
- Gates green (workspace tests via CI if disk is tight).
- `docs/scripting.md` mentions the budget; roadmap P1 "Remaining: a
  fuel/step budget" marked landed. Outcome filled.

## Outcome

**Status: done** (PR #225).

Landed:
- `crates/gradiance-script/src/budget.rs`: `SCRIPT_BUDGET` (2 s, a
  constant) and `run_with_budget(controller, budget, f)`, a scoped watchdog
  thread per run that trips the engine's `ThreadStateController` on timeout.
  Seven bare-engine tests: tail loop, named-let loop, deep non-tail recursion,
  `map` over a 10M `range` with a slow callback, engine reuse after one and
  two trips, a quick run untouched, a late trip not poisoning the next run.
- `ScriptEngine::run` arms it; a trip is `ScriptError::Budget { ms }`
  ("stopped after 2000 ms: budget exceeded"), shown in the console log like
  any error. `--script` files and hot reloads go through the same `run`.
- All or nothing: `Shared::mark`/`rollback` truncate all seven write queues
  (ops, config, actions, signal writes, signal defs, labels, panel requests)
  back to their pre-run length on any error (eval, panic, budget), so an
  earlier successful run in the same frame keeps its effects.
- Integration tests in `tests/it/scripting.rs` (runaway stopped + scene
  unchanged + engine reusable; spawn-then-loop spawns nothing; a failed run
  commits none of seven effect kinds; per-run rollback within one frame).
- `docs/scripting.md` § The run budget; roadmap P1 marked landed.

The three unverified facts:
1. **Poll frequency.** `Engine::with_interrupted(Arc<AtomicBool>)` is
   **inert** in steel 0.8.2: the flag is stored (`vm.rs` L924) and never
   read. The live mechanism is `ThreadStateController::interrupt()`, checked
   by `safepoint_or_interrupt` once per bytecode dispatch (top of the
   instruction loop, `vm.rs` ~L2518): per instruction.
2. **Native loops.** A single native builtin is **not** interruptible
   mid-call: `(length (range 0 20000000))` took 728 ms unguarded and 808 ms
   with a 1 ms budget, then failed with the trip. So a huge native call
   stalls the editor for its own duration (or exhausts memory), then is
   reported as a budget error. Native higher-order builtins (`map`) are
   stopped in their bytecode callback. No workaround built (memory limits
   out of scope).
3. **Engine reuse.** Reusable: after a trip and `resume()` the same engine
   evaluates `(+ 1 2)` = 3, also after a second trip. steel's own
   `InterruptHandler` is not used because its watchdog can `interrupt()`
   after `run_with_timeout` has `resume()`d (a run ending at the deadline),
   leaving the engine tripped; ours joins the watchdog before resetting.

All-or-nothing finding: **yes, failed runs committed partial edits before.**
`run_scripts` drained every queue after the loop regardless of the result.
With rollback disabled, the new tests fail (a spawn-then-loop script spawned
2 bodies; a spawn-then-error script spawned 1).

Note: `define`s a failed run made before stopping remain in the VM
(interpreter state, not scene state); documented in `docs/scripting.md`.
