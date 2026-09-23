---
title: srt render of the player exits without frames
description: "`srt render apps/player/src/index.tsx --file` exits 0 after `flux engine start` with no frame written and nothing logged, even at SRT_LOG=debug; playback ends before the first frame, and exit 0 with no message is the exit()-during-playback signature."
created: 2026-09-23
---

# srt render of the player exits without frames

```
srt render apps/player/src/index.tsx --file --fps 2 --duration 0.5 -o out
```

prints `[srt] flux engine start` and exits 0 with `out/` empty; no
`recorded N frames`, no error, nothing more at `SRT_LOG=debug`. Seen
2026-09-23 on the player as committed and after its routing migration.
Other apps render fine on the same runner (the router probe, the video
example), so it is something the player does at startup under playback.

`ExitPolicy::exit` in playback mode drains the raster thread and calls
`std::process::exit(0)` silently, which matches: something reaches
`exit()` or `background()` before the first frame. The player's own source
calls neither; candidates are a runtime module answering differently
without a dev session (`srt:apps`, `srt:dev` are `available: false`
there), the window `fullscreen` prop on the offscreen driver, or the
focus navigation setup.

Done: the player renders at least the home screen headlessly, and a
playback run that ends through `exit()` says so in the log at info level,
so the next silent exit is not silent.
