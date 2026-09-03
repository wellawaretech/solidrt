# srt render

{{ usage render }}

Replays an optional recorded script and writes frames, which is how an app
produces video or deterministic screenshots with no display attached:
rendering uses SDL's offscreen driver, or alloy's own EGL pbuffer where that
driver cannot go headless.

```sh
srt run --capture session.json
srt render --script session.json --fps 60 --duration 5
```

`--capture` records key events from connected clients to a script (pointer
input is not captured yet). `--size` is physical output pixels: layout runs
at exactly that size, so frames are identical on every machine. Every frame
follows a frame callback: frame k is the app's state after its (k+1)th
`onFrame` call, at time (k+1)/fps; the mount state before any callback is
drawn but never written. Frames land in the directory the command runs from;
`-o <path>` picks another (or a path prefix for the `-NNNNNN.png` names).
