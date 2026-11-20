//! The schedule dump, `gradiance schedule-dump` (brief 6-3).
//!
//! ```text
//! gradiance schedule-dump [schedule]
//! ```
//!
//! Builds the headless app (the stack `gradiance run` and the integration
//! tests use: no window, no GPU), initialises every schedule and prints each
//! one's systems in the order its executor holds them: a topological order
//! of the schedule's constraints, with the `ApplyDeferred` sync points
//! where commands land. That makes the deferred pipeline readable without a
//! debugger: which system's commands a later system can see is the question
//! the order answers.
//!
//! It uses Bevy's own introspection (`Schedule::systems` after
//! `Schedule::initialize`), no extra crate. The order is **one valid
//! order**: systems with no constraint between them may run in parallel or
//! the other way round. Schedules follow the frame: `Main`'s order, the
//! fixed loop's schedules after `RunFixedMainLoop`, then every other
//! schedule (avian's `PhysicsSchedule` and `SubstepSchedule`, start-up,
//! state transitions) by name. Render-world and window systems are absent
//! headless.
//!
//! Text, not DOT: a list diffs line by line between two commits and greps;
//! a graph of a few hundred systems does not read without a viewer.

use bevy::app::{FixedMainScheduleOrder, MainScheduleOrder};
use bevy::ecs::schedule::{InternedScheduleLabel, ScheduleLabel, Schedules};
use bevy::prelude::*;
use std::fmt::Write as _;

/// The subcommand word.
pub const SUBCOMMAND: &str = "schedule-dump";

/// The usage text.
pub const USAGE: &str = "\
usage: gradiance schedule-dump [schedule]

Prints the system order of the headless app's schedules as text: one
section per schedule (`# Update (N systems)`), one system per line in the
order the executor holds them (a topological order; unconstrained systems
may run in parallel). With a schedule name (Update, PostUpdate,
FixedUpdate, PhysicsSchedule, ...) only that schedule is printed.

exit codes: 0 done, 1 no such schedule, 2 usage";

/// The `gradiance schedule-dump` entry point: `None` when `args` is not
/// this subcommand, else the exit code.
pub fn main(args: &[String]) -> Option<i32> {
    if args.first().map(String::as_str) != Some(SUBCOMMAND) {
        return None;
    }
    let rest = &args[1..];
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return Some(0);
    }
    if rest.len() > 1 {
        eprintln!("gradiance schedule-dump: at most one schedule name\n{USAGE}");
        return Some(2);
    }
    let mut app = crate::run::headless_app_with(|_| {});
    Some(
        match dump(app.world_mut(), rest.first().map(String::as_str)) {
            Ok(text) => {
                print!("{text}");
                0
            }
            Err(e) => {
                eprintln!("gradiance schedule-dump: {e}");
                1
            }
        },
    )
}

/// The schedules of `world` in frame order: `Main`'s labels, the fixed
/// loop's after `RunFixedMainLoop`, then the rest by name. Labels with no
/// schedule (nothing added to them) are left out.
pub fn schedule_order(world: &World) -> Vec<InternedScheduleLabel> {
    let mut order: Vec<InternedScheduleLabel> = Vec::new();
    let fixed: Vec<InternedScheduleLabel> = world
        .get_resource::<FixedMainScheduleOrder>()
        .map_or_else(Vec::new, |o| o.labels.clone());
    if let Some(main) = world.get_resource::<MainScheduleOrder>() {
        for label in &main.labels {
            order.push(*label);
            if *label == RunFixedMainLoop.intern() {
                order.extend(fixed.iter().copied());
            }
        }
    }
    let mut rest: Vec<(String, InternedScheduleLabel)> = world
        .get_resource::<Schedules>()
        .map_or_else(Vec::new, |s| {
            s.iter()
                .map(|(_, schedule)| schedule.label())
                .filter(|label| !order.contains(label))
                .map(|label| (format!("{label:?}"), label))
                .collect()
        });
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    order.extend(rest.into_iter().map(|(_, label)| label));
    // A label with no system has no schedule (`FixedPreUpdate` here).
    order.retain(|label| {
        world
            .get_resource::<Schedules>()
            .is_some_and(|s| s.contains(*label))
    });
    order
}

/// The dump of every schedule, or of the one named `only` (its label as
/// `Debug` prints it, any case). Initialises each schedule first, as its
/// first run would.
///
/// # Errors
///
/// A message when `only` names no schedule or a schedule fails to build.
pub fn dump(world: &mut World, only: Option<&str>) -> Result<String, String> {
    let labels: Vec<InternedScheduleLabel> = schedule_order(world)
        .into_iter()
        .filter(|label| only.is_none_or(|name| format!("{label:?}").eq_ignore_ascii_case(name)))
        .collect();
    if labels.is_empty() {
        let known: Vec<String> = schedule_order(world)
            .iter()
            .map(|l| format!("{l:?}"))
            .collect();
        return Err(format!(
            "no schedule `{}` (schedules: {})",
            only.unwrap_or_default(),
            known.join(", ")
        ));
    }
    let mut out = String::new();
    for label in labels {
        let section = world
            .try_schedule_scope(label, |world, schedule| {
                schedule
                    .initialize(world)
                    .map_err(|e| format!("{label:?}: {e}"))?;
                let names: Vec<String> = schedule
                    .systems()
                    .map_err(|e| format!("{label:?}: {e}"))?
                    .map(|(_, system)| system.name().to_string())
                    .collect();
                Ok::<_, String>(names)
            })
            .map_err(|e| e.to_string())??;
        let _ = writeln!(out, "# {label:?} ({} systems)", section.len());
        for (i, name) in section.iter().enumerate() {
            let _ = writeln!(out, "{:>4}  {name}", i + 1);
        }
        out.push('\n');
    }
    Ok(out)
}
