---
name: sandbox
description: Run, drive and screenshot the elsewhere app in a private headless Wayland session. Use whenever you need to launch the app, click, type, scroll, drag or capture it to verify a change in the real UI.
---

# App sandbox

`scripts/sandbox.sh` runs the app as a native Wayland client inside its own
headless sway session, with a virtual keyboard (wtype) and a virtual pointer
(`scripts/sandbox-pointer.c`, built on first use). It never touches the desktop
(COSMIC): no portals, capture prompts or `/dev/uinput`. Don't use xdotool,
ydotool or cosmic-screenshot on the live desktop; they fail there.

Needs `sway grim wtype jq` and a C compiler.

## Loop

```sh
scripts/sandbox.sh launch          # builds, starts the session if needed, opens the app
scripts/sandbox.sh shot list       # prints target/sandbox/shots/list.png
```

Read the printed PNG with the Read tool, act, shoot again. `launch` starts with
no saved cities; `launch --keep` keeps the previous run's. Re-run `launch` after
a code change. `down` stops everything when done.

## Coordinates

Input takes **logical** pixels from the window's top-left. Screenshots are at
2x, so **halve image pixel positions** before clicking: a row drawn at y=212 in
the PNG is at y=106. When unsure, `move` there and `shot` to see the hover.

## Input

```sh
scripts/sandbox.sh click 200 30            # left click; add right|middle and a count
scripts/sandbox.sh type "Quito"
scripts/sandbox.sh key Down Return         # keysyms; combos like alt+Down, ctrl+a
scripts/sandbox.sh scroll 200 300 down 3   # wheel notches; also up|left|right
scripts/sandbox.sh drag 100 106 100 300    # left-button drag, e.g. reorder a row
scripts/sandbox.sh move 150 120            # hover, for tooltips
scripts/sandbox.sh resize 600 700          # window size in logical px
```

Every command waits until the input is delivered, but give the app a moment
(`sleep 0.3`) before a `shot`, so it can re-render or finish animating.

## Debugging

- `status` shows the session, app pid and window geometry.
- `log` prints the app's stdout/stderr (panics included).
- `target/sandbox/sway.log` and `pointer.log` hold compositor and pointer errors.
- `SANDBOX_SIZE=2000x1600 SANDBOX_SCALE=1 scripts/sandbox.sh up` sets the
  virtual display (run `down` first if a session is up).
- `shot-full` captures the whole output; `sway <args>` sends raw swaymsg commands.
