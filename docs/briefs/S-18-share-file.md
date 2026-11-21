# Share a scene as one file
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("online scene sharing") · status: ready

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

File ▸ Share writes one self-contained file (scene RON + thumbnail +
referenced images) and copies its path. Opening that file loads the scene.
No server: hosting is out of scope; this is the part of sharing that lives
in the app.

## Read first

- `crates/gradiance-persist` — save/load.
- Brief S-17 (thumbnail) and S-13 (image assets).

## Facts

- A zip with `scene.ron` at the root keeps RON the one scene format.
  *Check* the `zip` crate is already in `Cargo.lock`; a new crate is a
  reviewed lockfile change.

## Approach

- `scene.share` (`(share-scene path)`) and load accepts the bundle.

## Out of scope

Any network service, accounts, ratings, comments.

## Done when

- Round-trip test: share then open gives the same `scene-dump`. Gates green.

## Outcome
