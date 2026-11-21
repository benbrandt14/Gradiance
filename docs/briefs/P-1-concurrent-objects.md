# Max concurrent objects at 60 fps (perf deep dive)
Phase · owner-directed 2026-11-20 ("Improve the maximum number of concurrent objects while retaining 60fps with no stuttering") · status: done (in review, branch `perf/max-concurrent-objects`)

## Goal
One paragraph, the observable outcome: a settled sandbox scene holds far more
concurrent bodies inside the 60 fps frame budget on the target hardware
(RTX 3070, measured 1280x720 windowed), and a frame that briefly overloads
recovers instead of spiraling — no multi-second freezes.

## Read first
- `crates/gradiance-physics/src/lib.rs` — the islands enable + the wake-seam
  contract it documents (why it was off, what wakes what).
- `crates/gradiance-physics/src/island_sync.rs` — the paused-edit
  reconciliation; its module docs explain the avian message-expiry root cause.
- `crates/gradiance-geometry/src/polygonize.rs` — `ShapeContours`, the cached
  discretization every per-frame polygon consumer now reads.
- `tests/it/sleeping.rs` — the sleeping behaviour contract (8 tests).
- `docs/briefs/N-6-sketch-tool.md` § borders — the known issue this closes.

## Facts
Measured 2026-11-20, release build, single-thread headless rollouts
(`gradiance run`, 60 Hz fixed ticks, ground + N 0.4 m boxes) and the real
GPU desktop build (`--features diagnostics`, 20 s window):

| scene (ms/step) | before | after sleeping | all-static floor |
|---|---|---|---|
| 4000 flat resting | 7.7 | ~2 | 3.8 |
| 4000 settled pile | 12.0 | ~2–4 | 4.1 |
| empty scene | 1.5 | 1.5 | 1.5 |

| desktop frame time (avg) | before | after |
|---|---|---|
| empty scene | 8.4 ms (~120 fps) | ~8 ms |
| 1000 flat | 19.9 ms (~51 fps) | see Outcome |
| 4000 flat | 26.2 ms (~39 fps) | see Outcome |
| 4000 settling pile | 397 ms avg — fixed-catchup death spiral (~14 steps/frame) | bounded (clamp) |

- Islands were disabled as an **avian 0.7** panic workaround
  (`JointCollisionDisabled` on touching bodies); the pin is avian **0.8.0**,
  whose islands are a persistent-islands rewrite routing that exact path
  through `ContactStatusChangeQueue`. The repro is pinned by
  `tests/it/sleeping.rs::a_joint_between_touching_boxes_never_panics`.
- avian 0.8 surfaces two *new* island gaps, both fixed here:
  1. **static-static joints** panic in `merge_islands` (neither end is in an
     island). Derived `JointDisabled` now — sensors read angle/length from
     poses and keep working.
  2. **Joint graph changes while paused are lost**: avian communicates them
     through `JointGraphChange` messages read only inside the
     (paused-skipped) `PhysicsSchedule`, and bevy expires unread messages
     after two frames — delete-while-paused leaves a stale island node,
     undo reuses the slot, resume panics on the double link.
     `physics::island_sync` reconciles island links against the live graph
     while paused and drains the pending messages.
- avian wakes bodies itself on authored pose/velocity/force/`GravityScale`
  changes. It does **not** wake on: collider regen, layer regen, joint
  changes, `Forces` pushes, or the *removal* of
  `ConstantLinearAcceleration` (the gravity-drive unbind path). Those are
  Gradiance wake seams now.
- `draw_body_borders` re-polygonized every body every frame (the N-6
  known issue); `update_snapped_cursor` ran a linear exact-AABB scan over
  every collider every frame while paused; `sync_ground_planes` collected
  every `Changed<Transform>` body into a Vec each playing frame.
- The desktop "stutter" at high N was the **fixed-timestep catch-up death
  spiral**: any frame over 16.7 ms runs several steps next frame, each step
  > budget, self-sustaining.

## Approach
Four layers, each independently measured (commits in this order):
1. **Re-enable islands** (sleeping) + the wake seams + `SleepingDisabled`
   for enabled motors (`Box2D`'s motor-keeps-awake rule) + the paused-edit
   reconciliation (`island_sync`).
2. **Cache the discretization** (`ShapeContours`, derived beside the
   collider) and read it from every per-frame polygon consumer.
3. **Gate the per-frame scans**: snap skips the collider scan while paused
   with unchanged inputs; ground planes filter in place; the debug overlay
   pass skips entirely when every per-body toggle is off.
4. **Clamp fixed-timestep catch-up** (`clamp_fixed_catchup`,
   `ceil(speed)+3` steps per frame) — overload slows the sim below wall
   clock instead of freezing it.

## Out of scope
- avian-side fixes upstream (the two gaps above are worked around through
  public API; a fork/patch of avian is an owner decision).
- **Directional-shadow rendering of tall scenes** (bevy-side, pre-existing):
  a 4000-box tower costs ~242 ms/frame in shadow passes; shadows off
  renders the same scene at 47 ms. Its own follow-up brief (cascade /
  shadow-distance fit for the 2.5D view).
- egui outliner/depth-panel virtualization (O(N) only while their pane is
  open).
- The while-playing snap scan (still linear; while playing the physics step
  dominates — a BVH-backed query is a small follow-up).
- `retune_joints`/`guard_dangling_joints` per-frame joint passes (O(joints),
  not body-scaled).
- Exposing sleep thresholds or a SimSettings sleeping toggle (feel question
  — see the decisions below).

## Done when
- `tests/it/sleeping.rs` (8 tests) pins: rest sleeps; shape edit, teleport,
  joint change, thruster, fluid wake; motors keep bodies awake; jointed
  pairs sleep as one island; the 0.7 repro never panics.
- fmt + clippy + `--test it` (537) + boundaries + doc gates green.
- The headless N-scaling matrix and the desktop frame-time table re-measured
  on the branch (below).

## Outcome
Branch `perf/max-concurrent-objects`, two commits:
`0decac1` (islands + wake seams + reconciliation + tests),
`b7680f3` (contour cache + gated scans + catchup clamp).

Decisions (best-estimate, per the scoped-work ruling):
- **Sleeping joints read zero reaction force while their island sleeps**
  (XPBD solves nothing; `Box2D` parks machinery the same way). The force
  plot, the joint-load tint and `break_overloaded_joints` see 0 for parked
  joints; `the_joint_force_source_reads_the_load` now asserts the live
  (awake) read. If the owner wants resting joints to report their static
  load, that is an avian change (hold-last-lagrange semantics) — review
  question material.
- **Submerged bodies never sleep** (buoyancy wakes what it pushes): a
  floater converges by bobbing; freezing mid-bob is visibly wrong. Fields
  and air drag keep `Box2D`'s constant-of-the-scene semantics (no wake) —
  a body hovering in a field at rest simply stays put.
- **Borders draw every connected component** now, matching the extruded
  mesh (previously only the largest was outlined — a subtle inconsistency
  the cache removed for free).
- **Overload drops time rather than spiraling** (the clamp): at high body
  counts under 60 fps the sim briefly runs slower than wall clock. This is
  the standard fixed-timestep trade; `gradiance run` rollouts are
  unaffected (manual clock, one step per update).

Measured after (this branch, same scenes, same method; single-run
two-point slopes, **plain release** build, single-thread headless):

| scene (ms/step, settled steady state) | before | after |
|---|---|---|
| empty scene | 1.5 | ~1.5 |
| 4000 flat resting | 7.7 | **1.0** |
| 8000 flat resting | — | ~5 |
| 16000 flat resting | — | 3.5 |
| 4000 settled pile | 12.0 | **1.6** |

A resting scene now costs barely above the empty-scene floor — sleeping
bodies skip integration, solving and transform propagation, so the
per-step cost scales with *awake* bodies, not scene size.

Desktop end-to-end (RTX 3070, 1280x720): comparing like for like is
complicated by the desktop session — a Wayland compositor throttles
occluded windows to ~0-1 fps, so both builds were re-measured on an Xvfb
display with the real GPU (same present artifact both sides):

| Xvfb avg frame time | before | after |
|---|---|---|
| 1000 flat | 63.8 ms | 56.9 ms |
| 4000 flat | 69.4 ms | 67.4 ms |
| 16000 flat | 520.3 ms | **75.5 ms** |
| 4000 pile | ~500 ms | ~285 ms (settled; probe: awake=0) |

The remaining frame cost is **render-side, not physics**: at 4000 visible
bodies the desktop frame is render-bound (shadows + per-body toon
materials). One pathology stands out and predates this work (present in
the before build): a **tall** scene (a 4000-box tower) costs ~242 ms/frame
in directional-light shadow rendering — the bevy cascade fit for a 50 m
tall scene; with shadows off the same scene renders at 47 ms. That is the
next lever for dense vertical piles (its own brief: shadow-cascade tuning
for the 2.5D view / shadow-distance fit), together with the egui outliner
pane and the while-playing snap scan.
