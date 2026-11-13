//! The wall-clock budget for one script run: a guard against a runaway
//! authoring script (`(define (f) (f)) (f)`) hanging the editor.
//!
//! The VM is the cold authoring path (CLAUDE.md, the two-tier rule), so this is
//! a *guard*, not a scheduler: a run either finishes inside [`SCRIPT_BUDGET`] or
//! is stopped and reported, and nothing it queued is applied.
//!
//! # How steel 0.8.2 is stopped (verified by the tests below)
//!
//! - **The working lever is the engine's `ThreadStateController`.**
//!   `controller.interrupt()` makes the VM fail with an "Interrupted" error at
//!   its next safepoint; `controller.resume()` re-arms it. The VM checks the
//!   safepoint once per bytecode dispatch (`vm.rs`, the top of the
//!   instruction loop), so any loop or recursion that executes bytecode — tail
//!   loops, deep non-tail recursion, the closure a native `map` calls back
//!   into — is stopped within one instruction of the trip.
//! - **`Engine::with_interrupted(Arc<AtomicBool>)` is inert in 0.8.2**: the
//!   flag is stored on the VM and never read. Don't use it.
//! - **steel's own `InterruptHandler`** has a race: its watchdog thread can
//!   call `interrupt()` *after* `run_with_timeout` has called `resume()` (when
//!   the run ends right at the deadline), leaving the engine tripped and the
//!   next run failing at once. So this module uses a scoped watchdog thread
//!   per run that is **joined before** `resume()` — the trip can never land
//!   after the reset.
//! - **A single native builtin is not interruptible mid-call** (no safepoint
//!   inside Rust code): `(range 0 n)` for a huge `n` runs to completion (or
//!   exhausts memory) before the VM sees the trip. The run still fails as a
//!   budget error at the next instruction, so the editor stalls for as long as
//!   that one builtin takes, not forever. Memory limits are out of scope.
//! - **The engine is reusable after a trip**: a tripped run unwinds the VM
//!   stack, and after `resume()` the next run evaluates normally (pinned by
//!   `tests::engine_is_reusable_after_a_trip`).

use std::sync::mpsc;
use std::time::Duration;

use steel::steel_vm::ThreadStateController;

/// How long one script run (a REPL line, a `--script` file, a hot reload) may
/// run before it is stopped. Generous for authoring work — a scene-building
/// script finishes in milliseconds — and short enough that a runaway loop
/// costs one noticeable hitch, not a hung editor.
pub const SCRIPT_BUDGET: Duration = Duration::from_secs(2);

/// Runs `f` (a call into the engine `controller` belongs to) with a watchdog
/// that trips the engine once `budget` elapses. Returns `f`'s result and
/// whether the watchdog fired.
///
/// The watchdog is joined before the controller is reset, so a trip can never
/// outlive the run it was meant for. If the OS refuses a thread, `f` runs
/// unguarded rather than not at all (reported via `log`).
pub fn run_with_budget<R>(
    controller: &ThreadStateController,
    budget: Duration,
    f: impl FnOnce() -> R,
) -> (R, bool) {
    let (done, finished) = mpsc::channel::<()>();
    let (result, fired) = std::thread::scope(|scope| {
        let watchdog = std::thread::Builder::new()
            .name("script-budget".into())
            .spawn_scoped(scope, move || {
                // A disconnect (the run returned and `done` was dropped) means
                // "finished in time"; only a timeout trips the engine.
                match finished.recv_timeout(budget) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        controller.interrupt();
                        true
                    }
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => false,
                }
            });
        if let Err(err) = &watchdog {
            bevy::log::warn!("script budget unavailable, running unguarded: {err}");
        }
        let result = f();
        drop(done);
        let fired = watchdog.is_ok_and(|handle| handle.join().unwrap_or(false));
        (result, fired)
    });
    // Always reset: a trip that landed just as `f` returned must not poison
    // the next run.
    controller.resume();
    (result, fired)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use steel::steel_vm::engine::Engine;

    use super::*;

    const TEST_BUDGET: Duration = Duration::from_millis(200);
    /// Upper bound on how long a stopped run may take: the budget plus slack
    /// for a loaded CI runner.
    const STOP_WITHIN: Duration = Duration::from_secs(5);

    /// Runs `source` on `engine` under [`TEST_BUDGET`]; returns `(is_err,
    /// fired, elapsed)`.
    fn run(engine: &mut Engine, source: &'static str) -> (bool, bool, Duration) {
        let controller = engine.get_thread_state_controller();
        let start = Instant::now();
        let (result, fired) = run_with_budget(&controller, TEST_BUDGET, || engine.run(source));
        (result.is_err(), fired, start.elapsed())
    }

    fn assert_stopped(source: &'static str) -> Engine {
        let mut engine = Engine::new();
        let (is_err, fired, elapsed) = run(&mut engine, source);
        assert!(is_err, "the runaway script must fail");
        assert!(fired, "the failure must be the budget trip");
        assert!(elapsed < STOP_WITHIN, "stopped too late: {elapsed:?}");
        engine
    }

    fn assert_reusable(engine: &mut Engine) {
        let controller = engine.get_thread_state_controller();
        let (result, fired) = run_with_budget(&controller, TEST_BUDGET, || engine.run("(+ 1 2)"));
        assert!(!fired);
        let values = result.expect("engine evaluates after a trip");
        assert_eq!(values.last().map(ToString::to_string).as_deref(), Some("3"));
    }

    #[test]
    fn infinite_tail_loop_is_stopped() {
        assert_stopped("(define (spin) (spin)) (spin)");
    }

    #[test]
    fn while_style_named_let_is_stopped() {
        assert_stopped("(let loop ((i 0)) (loop (+ i 1)))");
    }

    #[test]
    fn deep_non_tail_recursion_is_stopped() {
        // Non-tail: every call leaves a pending `+`, so the VM stack grows.
        assert_stopped("(define (down n) (+ 1 (down n))) (down 0)");
    }

    #[test]
    fn long_map_over_a_big_list_is_stopped() {
        // A native higher-order builtin calling back into bytecode: the trip
        // lands in the callback.
        assert_stopped(
            "(define (slow x) (let loop ((i 0)) (if (< i 1000) (loop (+ i 1)) x))) \
             (map slow (range 0 10000000))",
        );
    }

    #[test]
    fn engine_is_reusable_after_a_trip() {
        let mut engine = assert_stopped("(define (spin) (spin)) (spin)");
        assert_reusable(&mut engine);
        // And again: a second trip on the same engine also works.
        let (is_err, fired, _) = run(&mut engine, "(let loop () (loop))");
        assert!(is_err && fired);
        assert_reusable(&mut engine);
    }

    #[test]
    fn a_quick_run_is_untouched() {
        let mut engine = Engine::new();
        let (is_err, fired, _) = run(&mut engine, "(define x 41) (+ x 1)");
        assert!(!is_err && !fired);
        assert_reusable(&mut engine);
    }

    #[test]
    fn a_trip_after_the_run_finished_does_not_poison_the_engine() {
        // The race steel's own `InterruptHandler` has: a trip landing after
        // the run returned. Simulate it directly, then confirm the reset.
        let mut engine = Engine::new();
        let controller = engine.get_thread_state_controller();
        let (result, _) = run_with_budget(&controller, TEST_BUDGET, || {
            let r = engine.run("(+ 1 1)");
            controller.interrupt();
            r
        });
        assert!(result.is_ok());
        assert_reusable(&mut engine);
    }
}
