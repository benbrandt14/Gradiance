#!/usr/bin/env bash
# Shows what upstream changed in one ported file since the revision recorded
# in vendor/UPSTREAM.toml (docs/unified-surface-plan.md § 6).
#
#   tools/upstream-diff.sh               # list the entries
#   tools/upstream-diff.sh ui_registry   # upstream diff rev..HEAD of that file,
#                                        # then the local file vs upstream HEAD
#
# Needs git and network access to the upstream; writes only under a temp dir.
set -euo pipefail
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
manifest="$root/vendor/UPSTREAM.toml"

# Reads `key = "value"` of the [[file]] entry whose name is $1.
field() {
  awk -v want="$1" -v key="$2" '
    /^\[\[file\]\]/ { name = "" }
    /^name *=/ { gsub(/^name *= *"|"$/, ""); name = $0 }
    name == want && $0 ~ "^" key " *=" { sub("^" key " *= *\"", ""); sub("\"$", ""); print; exit }
  ' "$manifest"
}

if [[ $# -eq 0 ]]; then
  grep -E '^name *=' "$manifest" | sed -E 's/^name *= *"(.*)"/\1/'
  exit 0
fi

name=$1
repo=$(field "$name" repo); path=$(field "$name" path)
rev=$(field "$name" rev); local_file=$(field "$name" local)
if [[ -z $repo || -z $path || -z $rev || -z $local_file ]]; then
  echo "no complete entry named '$name' in vendor/UPSTREAM.toml" >&2
  exit 1
fi

work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
git -C "$work" init -q
git -C "$work" remote add origin "$repo"
git -C "$work" fetch -q --filter=blob:none origin HEAD
git -C "$work" fetch -q --filter=blob:none origin "$rev" 2>/dev/null \
  || git -C "$work" fetch -q --unshallow origin 2>/dev/null || true
head=$(git -C "$work" rev-parse FETCH_HEAD)
echo "== $name: $path, recorded $rev, upstream HEAD $head"
echo "== upstream changes since the recorded revision"
git -C "$work" diff "$rev" "$head" -- "$path" || echo "(the recorded revision is not reachable)"
echo "== local file vs upstream HEAD (local changes are listed in UPSTREAM.toml)"
git -C "$work" show "$head:$path" > "$work/upstream"
diff -u "$work/upstream" "$root/$local_file" || true
