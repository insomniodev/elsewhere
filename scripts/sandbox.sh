#!/usr/bin/env bash
# Runs the app in a private headless Wayland session (sway) that agents can
# drive and capture without touching the desktop session or its permissions.
# Needs: sway, grim, wtype, jq, a C compiler. Run with no arguments for usage.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
state=$root/target/sandbox
shots=$state/shots

usage() {
  cat <<'EOF'
usage: scripts/sandbox.sh <command> [args]

session
  up                 start the headless session (SANDBOX_SIZE, SANDBOX_SCALE)
  down               stop the app and the session
  launch [--keep]    build, then (re)start the app; --keep keeps saved cities
  kill               stop the app
  status             session, app and window geometry
  log                print the app's stdout/stderr

capture
  shot [name]        capture the app window to target/sandbox/shots/<name>.png
  shot-full [name]   capture the whole output

input (coordinates are logical px relative to the window's top-left)
  move X Y           hover
  click X Y [right|middle] [count]
  drag X1 Y1 X2 Y2   left-button drag in small steps
  scroll X Y up|down|left|right [n]
  key COMBO...       e.g. Escape, Return, alt+Down, ctrl+a
  type TEXT          type literal text
  resize W H         resize the window (logical px)
  sway ARGS...       raw swaymsg against the session
EOF
}

die() { echo "sandbox: $*" >&2; exit 1; }

alive() { [[ -f $1 ]] && kill -0 "$(<"$1")" 2>/dev/null; }

# Loads SWAYSOCK and WAYLAND_DISPLAY of the running session.
attach() {
  alive "$state/sway.pid" && [[ -f $state/env ]] || die "no session; run: scripts/sandbox.sh up"
  read -r SWAYSOCK WAYLAND_DISPLAY <"$state/env"
  export SWAYSOCK WAYLAND_DISPLAY
  unset DISPLAY
}

msg() { swaymsg -s "$SWAYSOCK" "$@"; }

up() {
  for tool in sway grim wtype jq cc pkg-config; do
    command -v $tool >/dev/null || die "missing $tool"
  done
  if alive "$state/sway.pid"; then echo "session already up"; return; fi
  mkdir -p "$state" "$shots"
  rm -f "$state/env" "$state/pointer.in"
  mkfifo "$state/pointer.in"
  if [[ ! -x $state/pointer || $root/scripts/sandbox-pointer.c -nt $state/pointer ]]; then
    cc -O2 -o "$state/pointer" "$root/scripts/sandbox-pointer.c" \
      $(pkg-config --cflags --libs wayland-client)
  fi
  cat >"$state/sway.conf" <<EOF
output HEADLESS-1 resolution ${SANDBOX_SIZE:-1600x1400} scale ${SANDBOX_SCALE:-2}
xwayland disable
default_border none
default_floating_border none
focus_follows_mouse no
for_window [all] floating enable
exec "$state/pointer" "$state/pointer.in" 2>>"$state/pointer.log"
exec sh -c 'echo "\$SWAYSOCK \$WAYLAND_DISPLAY" > "$state/env"'
EOF
  # Detached from the desktop: no X, no parent compositor, no real input.
  env -u DISPLAY -u WAYLAND_DISPLAY -u SWAYSOCK \
    WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_LIBINPUT_NO_DEVICES=1 \
    setsid sway -c "$state/sway.conf" >"$state/sway.log" 2>&1 </dev/null &
  echo $! >"$state/sway.pid"
  for _ in $(seq 50); do
    alive "$state/sway.pid" || break
    # Ready once the virtual pointer has joined the seat.
    if [[ -s $state/env ]] && attach &&
      msg -t get_seats | jq -e '.[0].capabilities > 0' >/dev/null; then
      echo "session up: $WAYLAND_DISPLAY"
      return
    fi
    sleep 0.1
  done
  die "sway failed to start; see $state/sway.log"
}

down() {
  stop_app
  if alive "$state/sway.pid"; then kill "$(<"$state/sway.pid")"; fi
  rm -f "$state/sway.pid" "$state/env"
  echo "session down"
}

stop_app() {
  if alive "$state/app.pid"; then
    kill "$(<"$state/app.pid")"
    for _ in $(seq 30); do alive "$state/app.pid" || break; sleep 0.1; done
  fi
  rm -f "$state/app.pid"
}

launch() {
  local keep=${1:-}
  [[ -z $keep || $keep == --keep ]] || die "launch [--keep]"
  alive "$state/sway.pid" || up
  attach
  cargo build --quiet --manifest-path "$root/Cargo.toml" --bin elsewhere
  stop_app
  # A private config dir, so saved cities never touch the real ones.
  [[ $keep ]] || rm -rf "$state/config"
  mkdir -p "$state/config"
  XDG_CONFIG_HOME=$state/config RUST_BACKTRACE=1 \
    setsid "$root/target/debug/elsewhere" >"$state/app.log" 2>&1 </dev/null &
  echo $! >"$state/app.pid"
  for _ in $(seq 100); do
    if (geom) >/dev/null 2>&1; then
      msg "[pid=$(<"$state/app.pid")] focus" >/dev/null
      echo "app up: $(geom)"
      return
    fi
    alive "$state/app.pid" || break
    sleep 0.1
  done
  tail -n 30 "$state/app.log" >&2
  die "app window did not appear"
}

# Prints "X Y W H" of the app window in layout coordinates.
geom() {
  alive "$state/app.pid" || die "app not running; run: scripts/sandbox.sh launch"
  local pid out
  pid=$(<"$state/app.pid")
  out=$(msg -t get_tree | jq -r --argjson pid "$pid" '
    [recurse(.nodes[]?, .floating_nodes[]?) | select(.pid == $pid)][0]
    | select(. != null)
    | "\(.rect.x + .window_rect.x) \(.rect.y + .window_rect.y) \(.window_rect.width) \(.window_rect.height)"')
  [[ -n $out ]] || die "app window not mapped"
  echo "$out"
}

# Sends one command to the virtual pointer and waits until it is applied.
pointer() {
  local reply
  reply=$(mktemp -u "$state/reply.XXXXXX")
  mkfifo "$reply"
  # Opened read-write so neither end blocks if the helper is gone.
  echo "$* $reply" 1<>"$state/pointer.in"
  read -r -t 2 _ <>"$reply" || { rm -f "$reply"; die "virtual pointer not responding"; }
  rm -f "$reply"
}

# Moves to window-relative X Y.
point() {
  local g wx wy w h
  g=$(geom)
  read -r wx wy _ _ <<<"$g"
  read -r w h < <(msg -t get_outputs | jq -r '.[0].rect | "\(.width) \(.height)"')
  pointer move $((wx + $1)) $((wy + $2)) "$w" "$h"
}

button() { pointer button "$1" 1; pointer button "$1" 0; }

shot() {
  attach
  local name=${1:-$(date +%H%M%S)} g x y w h
  g=$(geom)
  read -r x y w h <<<"$g"
  grim -g "$x,$y ${w}x$h" "$shots/$name.png"
  echo "$shots/$name.png"
}

shot_full() {
  attach
  local name=${1:-full-$(date +%H%M%S)}
  grim "$shots/$name.png"
  echo "$shots/$name.png"
}

click() {
  (($# >= 2)) || die "click X Y [left|right|middle] [count]"
  attach
  local code=272 count=${4:-1} # evdev BTN_LEFT
  case ${3:-left} in
    left) ;; right) code=273 ;; middle) code=274 ;;
    *) die "unknown button: $3" ;;
  esac
  point "$1" "$2"
  for ((i = 0; i < count; i++)); do button $code; done
}

drag() {
  (($# == 4)) || die "drag X1 Y1 X2 Y2"
  attach
  local steps=12 x y
  point "$1" "$2"
  pointer button 272 1
  for ((i = 1; i <= steps; i++)); do
    x=$(($1 + ($3 - $1) * i / steps))
    y=$(($2 + ($4 - $2) * i / steps))
    point "$x" "$y"
    sleep 0.01
  done
  pointer button 272 0
}

scroll() {
  (($# >= 3)) || die "scroll X Y up|down|left|right [n]"
  attach
  local axis step
  case $3 in
    up) axis=0 step=-1 ;; down) axis=0 step=1 ;;
    left) axis=1 step=-1 ;; right) axis=1 step=1 ;;
    *) die "unknown direction: $3" ;;
  esac
  point "$1" "$2"
  for ((i = 0; i < ${4:-1}; i++)); do pointer scroll $axis $step; sleep 0.02; done
}

key() {
  (($# >= 1)) || die "key COMBO..."
  attach
  # wtype uploads a keymap first; without a pause the first key is dropped.
  local combo args=(-s 50) mods parts keysym
  for combo in "$@"; do
    IFS=+ read -ra parts <<<"$combo"
    keysym=${parts[-1]}
    mods=("${parts[@]:0:${#parts[@]}-1}")
    for m in "${mods[@]}"; do args+=(-M "$m"); done
    args+=(-k "$keysym")
    for m in "${mods[@]}"; do args+=(-m "$m"); done
  done
  wtype "${args[@]}"
}

status() {
  alive "$state/sway.pid" || { echo "session: down"; return; }
  attach
  echo "session: up ($WAYLAND_DISPLAY)"
  if alive "$state/app.pid"; then echo "app: pid $(<"$state/app.pid"), window $(geom)"
  else echo "app: not running"; fi
}

cmd=${1:-}
(($#)) && shift
case $cmd in
  up) up ;;
  down) down ;;
  launch) launch "$@" ;;
  kill) stop_app ;;
  status) status ;;
  log) cat "$state/app.log" ;;
  shot) shot "$@" ;;
  shot-full) shot_full "$@" ;;
  move) attach; point "$1" "$2" ;;
  click) click "$@" ;;
  drag) drag "$@" ;;
  scroll) scroll "$@" ;;
  key) key "$@" ;;
  type) attach; wtype -s 50 -- "$*" ;; # pause: see key()
  resize) attach; msg "[pid=$(<"$state/app.pid")] resize set $1 $2" >/dev/null ;;
  sway) attach; msg "$@" ;;
  *) usage; [[ -z $cmd ]] || exit 1 ;;
esac
