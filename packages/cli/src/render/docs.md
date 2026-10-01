# srt render

{{ usage render }}

Replays an optional recorded script and writes frames, which is how an app
produces video or deterministic screenshots with no display attached:
rendering uses SDL's offscreen driver, or alloy's own EGL pbuffer where that
driver cannot go headless.

```sh
srt run --capture session.json
srt render --script session.json --fps 60 --duration 5
srt render --settle --strict --size 480x640 --duration 1 --fps 2
```

A render has no wall clock, like an app test: time is the frames, timers
fire with the frame their time falls in, `performance.now()` reads 0, the
calendar starts at 2000-01-01T00:00:00Z and moves with the frames, and
`Math.random()` is seeded (`--seed <n>` for another sequence), so two
renders of one app write the same frames.

`--settle` runs the app to rest before the first frame is written: what it
started has landed (a fetch, a file read, a query), no timer is due and no
frame is demanded, so a transition the mount starts has played out. App
time passes while it does, and the frames start from there. An app that
never rests (a frame callback that never stops, a looping animation) fails
the render after 5000 ms of app time, naming what still wanted frames;
render such an app without the flag. `--strict` fails the render when the
app logs an error, a contained one included, so a scene whose build throws
no longer passes as an empty frame; without it, exit 0 means every frame
was written and nothing more.

`--link <link>` starts the app at a link (`env.launchLink`), so an app with
a router renders the screen the link names: one deterministic screenshot
per screen, for docs or a visual regression. `--capture` records key events
from connected clients to a script (pointer input is not captured yet). `--size` is physical output pixels: layout runs
at exactly that size, so frames are identical on every machine. Every frame
follows a frame callback: frame k is the app's state after its (k+1)th
`onFrame` call, at time (k+1)/fps; the mount state before any callback is
drawn but never written. Frames land in the directory the command runs from;
`-o <path>` picks another (or a path prefix for the `-NNNNNN.png` names).
A project's fonts (`solidrt.fonts` in package.json) register over the
runtime's built-in defaults, so text renders with the same fonts as under
the dev server. The app runs in the same sandbox as under a dev client
(`--data-root`, `-c`, the same defaults as `srt run`), so the state a dev
session built is what the frames show.
