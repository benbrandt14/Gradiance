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

Done 2026-11-23 (PR on the S-17 branch).

- The `zip` crate is not in `Cargo.lock`. `persist::bundle` writes and
  reads a stored (uncompressed) ZIP itself, with CRC-32 (decision
  `share-bundle-stored-zip`). The reader takes stored entries only and
  names the fix for a compressed one. Python's `zipfile` reads a bundle
  and its `testzip` passes.
- Layout: `scene.ron`, `thumbnail.png`, `images/<file>`. Each body
  texture path is rewritten to its `images/` entry; a missing image keeps
  its path and is logged.
- `scene.share` (`(share-scene path)`, File > Share..., Editor seam, no
  undo step) queues a `ShareSceneRequest`; the persist system writes the
  file and copies its path (`ClipboardWrite`).
- `scene.load` takes a `.zip`: the images go to a folder under the temp
  directory, which becomes the scene folder; the last-save path is cleared
  (decision `shared-bundle-opens-to-temp`). The open dialog shows `.zip`.
- `browser::thumbnail_png` encodes the S-17 thumbnail in memory.
- Tests: `tests/it/share.rs` (share then open gives the same
  `scene.dump`; an image travels inside the bundle and resolves after
  open); unit tests for the CRC, the writer and reader, a damaged archive,
  and unique image names.

