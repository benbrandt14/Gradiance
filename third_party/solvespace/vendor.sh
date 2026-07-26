#!/usr/bin/env bash
#
# Re-vendor the SolveSpace constraint solver subset from upstream.
#
# The vendored tree is a *pristine* copy of upstream at the pinned tag: no
# edits, no patches, no fork. Everything Gradiance needs to adapt the solver
# lives in `crates/gradiance-slvs-sys/` instead, so re-running this script
# against a newer upstream tag is a clean overwrite rather than a merge.
#
# Usage:  third_party/solvespace/vendor.sh [tag]
#
# After running, update the pin recorded in SOURCE.md and re-run the gate.

set -euo pipefail

TAG="${1:-v3.2}"
UPSTREAM="https://github.com/solvespace/solvespace"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The files upstream's `slvs-solver` + `slvs-interface` CMake targets compile,
# plus the headers they include transitively. Kept as one list so the set is
# reviewable in a diff.
FILES=(
  include/slvs.h

  src/constrainteq.cpp
  src/entity.cpp
  src/expr.cpp
  src/system.cpp
  src/util.cpp
  src/slvs/lib.cpp

  src/defs.h
  src/dsc.h
  src/expr.h
  src/handle.h
  src/param.h
  src/polygon.h
  src/resource.h
  src/sketch.h
  src/solvespace.h
  src/ttf.h
  src/ui.h
  src/util.h
  src/platform/gui.h
  src/platform/platform.h
  src/render/render.h
  src/srf/surface.h
)

echo "==> cloning $UPSTREAM at $TAG"
git clone --depth 1 --branch "$TAG" --filter=blob:none --no-checkout "$UPSTREAM" "$WORK/ss" >/dev/null 2>&1
git -C "$WORK/ss" sparse-checkout set --no-cone '/include' '/src' '/COPYING*' >/dev/null
git -C "$WORK/ss" checkout >/dev/null 2>&1
COMMIT="$(git -C "$WORK/ss" rev-parse HEAD)"

echo "==> $TAG is $COMMIT"

rm -rf "$HERE/include" "$HERE/src"
for f in "${FILES[@]}"; do
  mkdir -p "$HERE/$(dirname "$f")"
  cp "$WORK/ss/$f" "$HERE/$f"
done
cp "$WORK/ss"/COPYING* "$HERE/" 2>/dev/null || true

echo "==> vendored ${#FILES[@]} files"
find "$HERE/include" "$HERE/src" -type f | sort | sed "s|$HERE/|    |"
echo "==> pinned commit: $COMMIT"
echo "    record this in SOURCE.md if it changed"
