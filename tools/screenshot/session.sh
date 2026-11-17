#!/bin/bash
# session.sh start <app args...> | op <id> [params-json] | grab <out.png> | stop
# Drives the app through its own operations over the remote surface
# (`--remote`): input with ui.click/ui.drag/ui.key/ui.text/ui.wheel, frames
# with ui.screenshot. Xvfb only gives the app a display.
D=${GRAD_DISPLAY:-:77}; S=${GRAD_SHOT_DIR:-/tmp/gradiance-shots}; mkdir -p "$S"
PORT=${GRAD_PORT:-15702}
ROOT=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
BIN=${GRAD_BIN:-$ROOT/target/debug/gradiance}
op() { # op <id> [params-json]: one gradiance.op call; prints the JSON-RPC reply
  curl -s -X POST "http://127.0.0.1:$PORT" -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"gradiance.op\",\"params\":{\"id\":\"$1\",\"params\":${2:-{\}}}}"
  echo
}
case "$1" in
start) shift
  export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json XDG_RUNTIME_DIR=/tmp/xdg-sess BEVY_ASSET_ROOT=$ROOT WGPU_BACKEND=vulkan RUST_LOG=${RUST_LOG:-warn}
  mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
  nohup Xvfb $D -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 & echo $! > $S/.xvfb.pid; sleep 1
  DISPLAY=$D nohup "$BIN" --remote "$PORT" "$@" > $S/session.log 2>&1 & echo $! > $S/.app.pid ;;
op) shift; op "$@" ;;
grab) out=$(realpath -m "$2"); rm -f "$out"
  op ui.screenshot "{\"path\":\"$out\"}" >/dev/null
  for _ in $(seq 120); do [ -s "$out" ] && { echo "grabbed $out"; exit 0; }; sleep 0.5; done
  echo "no screenshot after 60 s (see $S/session.log)"; exit 1 ;;
stop) kill $(cat $S/.app.pid) $(cat $S/.xvfb.pid) 2>/dev/null; echo stopped ;;
*) echo "usage: $0 start <app args...> | op <id> [params-json] | grab <out.png> | stop"; exit 2 ;;
esac
