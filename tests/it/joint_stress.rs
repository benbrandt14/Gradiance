//! Joint stress tests: joints authored over overlapping bodies, across
//! depth bands, onto the background, and through cut / delete / undo, then
//! simulated. Each scene is a few lines of Lisp (the verbs the tools emit)
//! or a tool click; [`Watch`] plays the run and records what must never
//! happen: a non-finite pose, an explosion (a speed far past what gravity
//! gives the scene), a hinge whose two anchors pull apart, or a contact
//! between two bodies that share a joint.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use crate::harness::{headless_app, joint_count, paused_app, redo, undo};
use avian2d::prelude::{ContactGraph, LinearVelocity};
use bevy::prelude::*;
use gradiance::core::ids::IdIndex;
use gradiance::prelude::*;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};
use std::collections::HashSet;

/// Submits `source` and runs two frames: the script's intents dispatch on
/// the first, derived joints and colliders exist after the second. A body
/// a run spawns is addressable by its handle (`define`) in the next run.
fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
    app.update();
    let log = app.world().resource::<ScriptLog>();
    if let Some(entry) = log.0.last() {
        assert!(
            entry.ok,
            "script failed: {} -> {}",
            entry.input, entry.output
        );
    }
}

/// The body a script bound to `name` (its handle is the id's UUID text).
fn handle(app: &mut App, name: &str) -> StableId {
    app.world_mut().resource_mut::<ScriptInputs>().submit(name);
    app.update();
    let out = app
        .world()
        .resource::<ScriptLog>()
        .0
        .last()
        .map_or_else(String::new, |e| e.output.clone());
    let mut q = app.world_mut().query_filtered::<&StableId, With<Body>>();
    q.iter(app.world())
        .copied()
        .find(|id| out.contains(&id.0.to_string()))
        .unwrap_or_else(|| panic!("no body for `{name}`: {out:?}"))
}

fn set_state(app: &mut App, state: GameState) {
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(state);
    app.update();
}

/// What a run did, worst case over every frame.
#[derive(Debug, Default)]
struct Watch {
    /// Fastest body speed (m/s).
    max_speed: f32,
    /// Largest distance between a hinge's two anchor points (m).
    max_hinge_gap: f32,
    /// Whether any body pose went non-finite.
    non_finite: bool,
    /// Frames on which two bodies that share a non-colliding joint touched.
    jointed_contacts: usize,
}

/// The world point of a body-local anchor.
fn anchor_world(app: &App, id: StableId, local: Vec2) -> Option<Vec2> {
    let entity = app.world().resource::<IdIndex>().entity(id)?;
    let pose = PosRot::from_transform(app.world().get::<Transform>(entity)?);
    Some(pose.pos + Vec2::from_angle(pose.rot).rotate(local))
}

fn pose(app: &App, id: StableId) -> PosRot {
    let entity = app.world().resource::<IdIndex>().entity(id).unwrap();
    PosRot::from_transform(app.world().get::<Transform>(entity).unwrap())
}

fn joints(app: &mut App) -> Vec<JointDef> {
    app.world_mut()
        .query::<&JointDef>()
        .iter(app.world())
        .cloned()
        .collect()
}

impl Watch {
    /// Plays `frames` frames and records the worst of each measure.
    fn run(app: &mut App, frames: usize) -> Self {
        let mut watch = Self::default();
        for _ in 0..frames {
            app.update();
            watch.sample(app);
        }
        watch
    }

    fn sample(&mut self, app: &mut App) {
        let mut q = app
            .world_mut()
            .query_filtered::<(&Transform, Option<&LinearVelocity>), With<Body>>();
        for (transform, velocity) in q.iter(app.world()) {
            if !transform.translation.is_finite() || !transform.rotation.is_finite() {
                self.non_finite = true;
            }
            if let Some(v) = velocity {
                self.max_speed = self.max_speed.max(v.0.length());
            }
        }
        let defs = joints(app);
        let index = app.world().resource::<IdIndex>();
        let mut pairs = HashSet::new();
        for def in &defs {
            let Some(b) = def.body_b else { continue };
            if def.common.collide_connected || def.common.broken {
                continue;
            }
            if let (Some(ea), Some(eb)) = (index.entity(def.body_a), index.entity(b)) {
                pairs.insert((ea.min(eb), ea.max(eb)));
            }
        }
        for def in &defs {
            if !matches!(def.kind, JointKind::Hinge { .. }) || def.common.broken {
                continue;
            }
            let Some(on_a) = anchor_world(app, def.body_a, def.anchor_a.value()) else {
                continue;
            };
            let on_b = match def.body_b {
                Some(b) => anchor_world(app, b, def.anchor_b.value()),
                None => Some(def.anchor_b.value()),
            };
            if let Some(on_b) = on_b {
                self.max_hinge_gap = self.max_hinge_gap.max(on_a.distance(on_b));
            }
        }
        let graph = app.world().resource::<ContactGraph>();
        let touching = graph.iter_active_touching().any(|pair| {
            let (a, b) = (pair.collider1, pair.collider2);
            pairs.contains(&(a.min(b), a.max(b)))
        });
        if touching {
            self.jointed_contacts += 1;
        }
    }

    /// The common verdict: nothing blew up, no hinge pulled apart and no two
    /// jointed bodies touched. `speed_cap` is a speed the scene's falls and
    /// swings cannot reach (m/s).
    fn assert_calm(&self, what: &str, speed_cap: f32) {
        assert!(!self.non_finite, "{what}: a pose went non-finite");
        assert!(
            self.max_speed < speed_cap,
            "{what}: a body reached {} m/s (cap {speed_cap})",
            self.max_speed
        );
        assert!(
            self.max_hinge_gap < 0.02,
            "{what}: a hinge pulled {} m apart",
            self.max_hinge_gap
        );
        assert_eq!(
            self.jointed_contacts, 0,
            "{what}: two jointed bodies touched"
        );
    }
}

/// The relative angle `b − a`, wrapped into (−π, π].
fn relative_angle(app: &App, a: StableId, b: StableId) -> f32 {
    let d = pose(app, b).rot - pose(app, a).rot;
    (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Plays `frames` frames and returns the range of `b`'s angle against `a`
/// (max − min): how far the hinge between them turned.
fn swing(app: &mut App, a: StableId, b: StableId, frames: usize, watch: &mut Watch) -> f32 {
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for _ in 0..frames {
        app.update();
        watch.sample(app);
        let r = relative_angle(app, a, b);
        lo = lo.min(r);
        hi = hi.max(r);
    }
    hi - lo
}

// ---------- Overlap ----------

#[test]
fn two_fully_overlapping_bars_hinged_and_pinned_swing_freely() {
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 -1 0)
         (define a (spawn-box 0 1 1 0.1)) (define b (spawn-box 0 1 1 0.1))",
    );
    run(&mut app, "(hinge a -1 -0.45 1) (hinge a b 0.45 1)");
    assert_eq!(joint_count(&mut app), 2);
    let (a, b) = (handle(&mut app, "a"), handle(&mut app, "b"));
    let mut watch = Watch::default();
    let turned = swing(&mut app, a, b, 300, &mut watch);
    watch.assert_calm("overlapping bars", 10.0);
    assert!(
        turned > 1.0,
        "the hinged bar turns against its parent ({turned} rad)"
    );
}

#[test]
fn a_third_body_under_a_hinge_does_not_stop_the_pair_turning() {
    // Three bars overlapping in a row; the hinge joins the first two where
    // all three overlap. The third is free and falls through the pair to
    // the ground; the pair keeps swinging.
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 -1 0)
         (define a (spawn-box 0 1 1 0.1)) (define b (spawn-box 0.3 1 1 0.1))
         (define c (spawn-box 0.6 1 1 0.1))",
    );
    run(&mut app, "(hinge a -1 -0.45 1) (hinge a b 0.4 1)");
    let (a, b) = (handle(&mut app, "a"), handle(&mut app, "b"));
    let mut watch = Watch::default();
    let turned = swing(&mut app, a, b, 300, &mut watch);
    watch.assert_calm("three overlapping", 10.0);
    assert!(turned > 0.5, "the pair still turns ({turned} rad)");
}

#[test]
fn three_bars_on_one_pivot_each_turn_freely() {
    // Scissors: A pinned to the world at the pivot, B hinged to A and C
    // hinged to B at the same point. A and C share no joint: they may touch
    // (as in Algodoo), but nothing may lock or blow up.
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 -1 0)
         (define a (spawn-box 0.45 1 1 0.1)) (define b (spawn-box-rot 0.4 1.2 1 0.1 0.5))
         (define c (spawn-box-rot 0.4 0.8 1 0.1 -0.5))",
    );
    run(&mut app, "(hinge a -1 0 1) (hinge a b 0 1) (hinge b c 0 1)");
    let (a, b, c) = (
        handle(&mut app, "a"),
        handle(&mut app, "b"),
        handle(&mut app, "c"),
    );
    let mut watch = Watch::default();
    let ab = swing(&mut app, a, b, 300, &mut watch);
    let bc = swing(&mut app, b, c, 300, &mut watch);
    watch.assert_calm("three-bar pivot", 10.0);
    assert!(ab > 0.5 && bc > 0.5, "every bar turns ({ab}, {bc} rad)");
}

#[test]
fn a_wheel_hinged_at_its_centre_rolls_a_cart_down_a_slope() {
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 0 -0.2) (define car (spawn-box 0 0.6 1.2 0.2))
         (define w1 (spawn-circle -0.5 0.45 0.25)) (define w2 (spawn-circle 0.5 0.45 0.25))",
    );
    run(&mut app, "(hinge car w1 -0.5 0.45) (hinge car w2 0.5 0.45)");
    let (car, w1) = (handle(&mut app, "car"), handle(&mut app, "w1"));
    let start = pose(&app, car).pos.x;
    let mut watch = Watch::default();
    let turned = swing(&mut app, car, w1, 240, &mut watch);
    watch.assert_calm("cart", 15.0);
    assert!(
        turned > 3.0,
        "the wheel turns against the chassis ({turned} rad)"
    );
    assert!(
        pose(&app, car).pos.x > start + 0.5,
        "the cart rolls downhill"
    );
}

#[test]
fn a_four_bar_linkage_closed_by_hinges_moves_and_holds() {
    // A closed loop of hinges (world, crank, coupler, rocker, world).
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 -2 0)
         (define crank (spawn-box -0.5 0.5 0.1 1)) (define rocker (spawn-box 0.5 0.5 0.1 1))
         (define coupler (spawn-box 0 1 1 0.1))",
    );
    run(
        &mut app,
        "(hinge crank -1 -0.5 0) (hinge rocker -1 0.5 0)
         (hinge crank coupler -0.5 1) (hinge rocker coupler 0.5 1)",
    );
    let (crank, coupler) = (handle(&mut app, "crank"), handle(&mut app, "coupler"));
    // A sideways gravity swings the parallelogram.
    run(&mut app, "(sim-set \"gravity.x\" 4)");
    let mut watch = Watch::default();
    let turned = swing(&mut app, crank, coupler, 300, &mut watch);
    watch.assert_calm("four-bar", 10.0);
    assert!(turned > 0.3, "the linkage moves ({turned} rad)");
}

// ---------- Depth ----------

#[test]
fn bodies_on_different_depth_bands_hinge_and_turn() {
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 -1 0)
         (define a (spawn-box 0 1 1 0.1)) (define b (spawn-box 0.4 1 1 0.3))
         (define d (spawn-box 3 3 0.1 0.1))",
    );
    run(&mut app, "(hinge a -1 -0.45 1) (hinge a b 0.4 1)");
    // Then move the pair to other layers mid-run: a stays in front, b goes
    // two layers back (a group edit, one undo step, as the Depth panel).
    let (a, b) = (handle(&mut app, "a"), handle(&mut app, "b"));
    let mut watch = Watch::default();
    swing(&mut app, a, b, 30, &mut watch);
    run(&mut app, "(group-set (group (list b d)) \"depth\" 0.2)");
    run(&mut app, "(group-set (group (list a b)) \"depth\" 0.3)");
    let turned = swing(&mut app, a, b, 240, &mut watch);
    watch.assert_calm("depth bands", 10.0);
    assert!(turned > 0.5, "the pair turns ({turned} rad)");
}

// ---------- Tools ----------

fn set_cursor(app: &mut App, p: Vec2) {
    app.world_mut().insert_resource(CursorWorldPos(Some(p)));
    app.world_mut().insert_resource(SnappedCursor {
        raw: Some(p),
        position: Some(p),
        kind: None,
    });
}

fn use_tool(app: &mut App, tool: ToolState) {
    app.world_mut().resource_mut::<SnapConfig>().objects_enabled = false;
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(tool);
    app.update();
}

fn drag(app: &mut App, from: Vec2, to: Vec2) {
    set_cursor(app, from);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    set_cursor(app, to);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();
}

fn click(app: &mut App, p: Vec2) {
    drag(app, p, p);
}

#[test]
fn a_hinge_on_a_snapped_corner_inside_another_body_joins_both() {
    // Object snap puts the anchor *exactly* on a vertex or an edge. Bar a's
    // corner and edge sit inside block b: the click is over both bodies, so
    // the hinge must join them, not pin one to the world.
    let mut app = paused_app();
    run(
        &mut app,
        "(define a (spawn-box 0 1 1 0.1)) (define b (spawn-box 0.5 1 0.4 0.4))",
    );
    let (a, b) = (handle(&mut app, "a"), handle(&mut app, "b"));
    use_tool(&mut app, ToolState::Hinge);
    for point in [
        Vec2::new(0.5, 1.05),
        Vec2::new(0.5, 0.95),
        Vec2::new(0.5, 1.0),
        Vec2::new(0.4, 1.05),
    ] {
        click(&mut app, point);
    }
    let defs = joints(&mut app);
    assert_eq!(defs.len(), 4);
    for def in defs {
        let mut ends = [Some(def.body_a.0), def.body_b.map(|id| id.0)];
        ends.sort();
        let mut want = [Some(a.0), Some(b.0)];
        want.sort();
        assert_eq!(ends, want, "a boundary click joins both bodies");
    }
}

#[test]
fn a_hinge_click_on_empty_background_does_nothing() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 1 1 0.1)");
    use_tool(&mut app, ToolState::Hinge);
    click(&mut app, Vec2::new(3.0, 3.0));
    assert_eq!(joint_count(&mut app), 0);
}

#[test]
fn a_spring_dragged_from_the_background_to_a_body_pins_its_start() {
    // Algodoo: a spring joins whatever is under each end, the background
    // included, in either drag direction.
    let mut app = paused_app();
    run(&mut app, "(define a (spawn-box 0 0 0.4 0.4))");
    let a = handle(&mut app, "a");
    use_tool(&mut app, ToolState::Spring);
    drag(&mut app, Vec2::new(0.0, 1.5), Vec2::new(0.0, 0.1));
    let defs = joints(&mut app);
    assert_eq!(defs.len(), 1, "a spring from the background is made");
    let def = &defs[0];
    assert_eq!(def.body_a, a);
    assert_eq!(def.body_b, None);
    assert!((def.anchor_b.value() - Vec2::new(0.0, 1.5)).length() < 1e-4);
    set_state(&mut app, GameState::Playing);
    let watch = Watch::run(&mut app, 240);
    watch.assert_calm("background spring", 10.0);
    assert!(pose(&app, a).pos.y > -3.0, "the spring holds the box");
}

#[test]
fn a_slider_on_overlapping_bodies_slides_calmly() {
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 -1 0)
         (define rail (spawn-box 0 1 2 0.1)) (define cart (spawn-box 0.2 1 0.3 0.3))",
    );
    run(
        &mut app,
        "(hinge rail -1 -0.9 1) (hinge rail -1 0.9 1) (slider cart rail 0.2 1 1 -0.3)",
    );
    let watch = Watch::run(&mut app, 240);
    watch.assert_calm("slider", 15.0);
}

// ---------- Cut / delete / undo ----------

#[test]
fn deleting_and_restoring_a_middle_link_mid_run_stays_calm() {
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 -2 0)
         (define a (spawn-box 0 1 0.6 0.1)) (define b (spawn-box 0.5 1 0.6 0.1))
         (define c (spawn-box 1 1 0.6 0.1))",
    );
    run(
        &mut app,
        "(hinge a -1 -0.25 1) (hinge a b 0.25 1) (hinge b c 0.75 1)",
    );
    Watch::run(&mut app, 60).assert_calm("chain", 10.0);
    set_state(&mut app, GameState::Paused);
    let b = handle(&mut app, "b");
    let index = {
        let mut q = app.world_mut().query_filtered::<&StableId, With<Body>>();
        let mut ids: Vec<StableId> = q.iter(app.world()).copied().collect();
        ids.sort_by_key(|id| id.0);
        ids.iter().position(|id| *id == b).unwrap()
    };
    run(&mut app, &format!("(delete {index})"));
    assert_eq!(joint_count(&mut app), 1, "b's two hinges go with it");
    undo(&mut app);
    assert_eq!(joint_count(&mut app), 3);
    redo(&mut app);
    undo(&mut app);
    set_state(&mut app, GameState::Playing);
    Watch::run(&mut app, 240).assert_calm("restored chain", 10.0);
}

#[test]
fn cutting_through_a_hinged_pair_mid_run_stays_calm() {
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 -2 0)
         (define a (spawn-box 0 1 1 0.1)) (define b (spawn-box 0.5 1 1 0.1))",
    );
    run(&mut app, "(hinge a -1 -0.4 1) (hinge a b 0.3 1)");
    Watch::run(&mut app, 30).assert_calm("before cut", 10.0);
    set_state(&mut app, GameState::Paused);
    run(&mut app, "(cut 0.1 3 0.1 -1.5 0.02)");
    set_state(&mut app, GameState::Playing);
    Watch::run(&mut app, 240).assert_calm("after cut", 10.0);
    set_state(&mut app, GameState::Paused);
    undo(&mut app);
    undo(&mut app);
    set_state(&mut app, GameState::Playing);
    Watch::run(&mut app, 240).assert_calm("cut undone", 10.0);
}

#[test]
fn a_chain_pinned_at_both_ends_stays_calm() {
    let mut app = headless_app();
    run(
        &mut app,
        "(spawn-ground 0 -3 0)
         (chain (list -1 1 0 0 1 1) 0.2 0.05 -1 -1 \"world\")",
    );
    Watch::run(&mut app, 300).assert_calm("chain", 10.0);
}

// ---------- Sliders ----------

/// The cart's distance from its slider's line, and how far along the line
/// it sits from the authored anchor (both in metres), read off the rail's
/// live pose and the authored joint.
fn off_and_along(app: &mut App, rail: StableId, cart: StableId) -> (f32, f32) {
    let def = joints(app)
        .into_iter()
        .find(|d| matches!(d.kind, JointKind::Slider { .. }))
        .unwrap();
    let JointKind::Slider { axis, .. } = def.kind else {
        unreachable!()
    };
    let r = pose(app, rail);
    let origin = anchor_world(app, rail, def.anchor_a.value()).unwrap();
    let on_cart = anchor_world(app, cart, def.anchor_b.value()).unwrap();
    let axis = Vec2::from_angle(r.rot).rotate(axis);
    let d = on_cart - origin;
    (axis.perp_dot(d).abs(), axis.dot(d))
}

/// A rail pinned to the world at its centre and a cart on it, authored
/// paused (nothing falls before the joints exist), pulled sideways.
fn rail_and_cart(slider: &str) -> (App, StableId, StableId) {
    let mut app = paused_app();
    run(
        &mut app,
        "(define rail (spawn-box 0 1 2 0.1)) (define cart (spawn-box 0.2 1 0.3 0.3))
         (sim-set \"gravity.x\" 5)",
    );
    run(&mut app, &format!("(hinge rail -1 0 1) {slider}"));
    let (rail, cart) = (handle(&mut app, "rail"), handle(&mut app, "cart"));
    (app, rail, cart)
}

#[test]
fn a_cart_run_far_along_a_pinned_rail_stays_on_it() {
    // A rail pinned at its centre turns freely; a cart on it slides out
    // under a sideways pull, far past the rail's end (the slider is
    // unlimited). Before the fix the cart flew off at thousands of m/s once
    // it was about 4 m out, at any substep count.
    // Either body may be the slider's first: the rail …
    let (mut app, rail, cart) = rail_and_cart("(slider rail cart 0.2 1 1 0)");
    set_state(&mut app, GameState::Playing);
    let mut watch = Watch::default();
    let mut worst_off = 0.0_f32;
    let mut furthest = 0.0_f32;
    for _ in 0..240 {
        app.update();
        watch.sample(&mut app);
        let (off, along) = off_and_along(&mut app, rail, cart);
        worst_off = worst_off.max(off);
        furthest = furthest.max(along.abs());
    }
    watch.assert_calm("cart on a pinned rail", 100.0);
    assert!(furthest > 6.0, "the cart ran far out ({furthest} m)");
    assert!(
        worst_off < 0.02,
        "the cart stays on the slide line ({worst_off} m)"
    );
    // … or the cart.
    let (mut app, _, _) = rail_and_cart("(slider cart rail 0.2 1 1 0)");
    set_state(&mut app, GameState::Playing);
    Watch::run(&mut app, 240).assert_calm("cart-first slider", 100.0);
}

#[test]
fn a_spinning_body_stops_turning_when_a_world_slider_locks_it() {
    // A world-pin slider rotation-locks its body (the guide rule). avian's
    // lock keeps any spin the body already had, which the slider then
    // cannot remove: the body turned forever on its guide.
    let mut app = headless_app();
    run(
        &mut app,
        "(define cart (spawn-box 0 1 0.3 0.3)) (sim-set \"gravity.y\" 0)",
    );
    let cart = handle(&mut app, "cart");
    let entity = app.world().resource::<IdIndex>().entity(cart).unwrap();
    app.world_mut()
        .get_mut::<avian2d::prelude::AngularVelocity>(entity)
        .unwrap()
        .0 = 2.0;
    run(&mut app, "(slider cart -1 0 1 1 0)");
    let before = pose(&app, cart).rot;
    Watch::run(&mut app, 60).assert_calm("world slider", 5.0);
    let turned = (pose(&app, cart).rot - before).abs();
    assert!(
        turned < 1e-3,
        "the guided body holds its angle ({turned} rad)"
    );
}

#[test]
fn slider_limits_still_bound_the_travel_on_a_pinned_rail() {
    // The engine anchor follows the cart; the authored limits must still
    // measure from the authored anchor.
    let (mut app, rail, cart) = rail_and_cart("(slider rail cart 0.2 1 1 0)");
    let mut q = app.world_mut().query::<&mut JointDef>();
    for mut def in q.iter_mut(app.world_mut()) {
        if let JointKind::Slider { limits, .. } = &mut def.kind {
            *limits = Some([
                gradiance::units::Length(-0.1),
                gradiance::units::Length(0.6),
            ]);
        }
    }
    set_state(&mut app, GameState::Playing);
    let mut watch = Watch::default();
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for _ in 0..240 {
        app.update();
        watch.sample(&mut app);
        let along = off_and_along(&mut app, rail, cart).1;
        lo = lo.min(along);
        hi = hi.max(along);
    }
    watch.assert_calm("limited cart", 15.0);
    assert!(
        lo > -0.12 && hi < 0.62,
        "travel stays in [-0.1, 0.6] ({lo}, {hi})"
    );
    assert!(hi > 0.55, "the pull takes the cart to its far limit ({hi})");
}

// ---------- Ground ----------

#[test]
fn a_hinge_click_on_the_ground_alone_does_nothing() {
    let mut app = paused_app();
    run(&mut app, "(spawn-ground 0 0 0)");
    use_tool(&mut app, ToolState::Hinge);
    click(&mut app, Vec2::new(0.0, -1.0));
    assert_eq!(
        joint_count(&mut app),
        0,
        "a pin on a fixed half-plane is no joint"
    );
}

#[test]
fn a_hinge_on_a_body_edge_resting_on_the_ground_joins_body_to_ground() {
    let mut app = paused_app();
    run(
        &mut app,
        "(define g (spawn-ground 0 0 0)) (define a (spawn-box 0 0.05 1 0.1))",
    );
    let (g, a) = (handle(&mut app, "g"), handle(&mut app, "a"));
    use_tool(&mut app, ToolState::Hinge);
    // On the bottom edge, which is also the ground's surface.
    click(&mut app, Vec2::new(0.3, 0.0));
    let defs = joints(&mut app);
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].body_a, a, "the body, not the ground, is body a");
    assert_eq!(defs[0].body_b, Some(g));
}

#[test]
fn a_spring_drawn_between_two_empty_points_makes_nothing() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 0 0.4 0.4)");
    use_tool(&mut app, ToolState::Spring);
    drag(&mut app, Vec2::new(2.0, 2.0), Vec2::new(3.0, 2.5));
    assert_eq!(joint_count(&mut app), 0);
}
