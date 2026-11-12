#!/bin/bash
# session.sh start <app args...> | grab <out.png> | x <xdotool args...> | stop
D=${GRAD_DISPLAY:-:77}; S=${GRAD_SHOT_DIR:-/tmp/gradiance-shots}; mkdir -p "$S"
BIN=${GRAD_BIN:-$(git -C "$(dirname "$0")" rev-parse --show-toplevel)/target/debug/gradiance}
ROOT=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
case "$1" in
start) shift
  export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json XDG_RUNTIME_DIR=/tmp/xdg-sess BEVY_ASSET_ROOT=$ROOT WGPU_BACKEND=vulkan RUST_LOG=${RUST_LOG:-warn}
  mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
  nohup Xvfb $D -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 & echo $! > $S/.xvfb.pid; sleep 1
  DISPLAY=$D nohup "$BIN" "$@" > $S/session.log 2>&1 & echo $! > $S/.app.pid ;;
grab) DISPLAY=$D xwd -root -silent | convert xwd:- "$2" && echo "grabbed $2" ;;
x) shift; DISPLAY=$D xdotool "$@" ;;
stop) kill $(cat $S/.app.pid) $(cat $S/.xvfb.pid) 2>/dev/null; echo stopped ;;
esac
