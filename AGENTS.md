# Agents

## Running the app

Drive and screenshot the app with `scripts/sandbox.sh`, which runs it in a
private headless sway session. Don't control the live desktop (COSMIC) with
xdotool, ydotool or cosmic-screenshot; they fail there.

```sh
scripts/sandbox.sh launch      # build and open the app
scripts/sandbox.sh click 58 534
scripts/sandbox.sh shot name   # prints the PNG path
scripts/sandbox.sh down
```

Usage and gotchas: [.claude/skills/sandbox/SKILL.md](.claude/skills/sandbox/SKILL.md).
