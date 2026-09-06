---
title: A standing onFrame ticks at 2-3 Hz on an idle desktop client
description: debugging.md promises a registered onFrame keeps the runtime calling it every frame at the refresh rate; on a SwapPaced desktop client whose picture does not change, the callback ran 2-3 times a second and only a picture change (or a control-API call) produced a frame, so a frame loop that integrates from tick deltas (a gravity fall that does not move the camera yet) stalls.
created: 2026-09-06
---

# A standing onFrame ticks at 2-3 Hz on an idle desktop client

## Symptom

`packages/cli/agents/debugging.md` ("Lessons that cost real time"): a
registered onFrame is a standing request, not demand-gated - "the
runtime keeps calling it - and presents - every frame at the refresh rate
until you deregister it (fps stays at the refresh rate on an idle
screen)". Observed on the desktop (Linux, Wayland/Hyprland, SwapPaced
pacing, release client of 2026-09-06, file-mode dev server): a bare
`onFrame(() => frames++)` with nothing else changing ticks 2-3 times a
second; `/stats` shows `fps 0`, `reusedPerSec 0`, `idleTicks` growing.
Focused or unfocused made no difference. While the picture changed every
frame (a camera moving) the same app ran at the refresh rate, and each
control-API call produced one more tick.

Repro (`probes/` is gitignored, so the whole probe is here): save this as
`probes/onframe-probe.tsx`, `bunx srt run probes/onframe-probe.tsx --file
--port 34902`, then `POST /__control__/debug?name=frames` twice a second
apart.

```tsx
import { onFrame, pct, render } from "@solidrt/core"
import { registerDebug } from "srt:dev"

let frames = 0
onFrame(() => {
  frames++
})
registerDebug("frames", () => frames)

render(() => (
  <window>
    <view width={pct(100)} height={pct(100)} backgroundColor="#223344" />
  </window>
))
```

## Why it matters

A frame loop that integrates from tick deltas and only changes the
picture once it has integrated something - a fall from rest, a timer
that fires after N frames of nothing - never gets going. `examples/
collision.tsx` in `@solidrt/3d` hit exactly this: its gravity loop
starts at speed 0, the first frames move the eye by nothing visible, and
the loop then waited on the idle cadence. `/clock?step=<n>` did not
advance the onFrame tick either (steps ran, `tick` deltas stayed 0).
Seen again 2026-09-06 from the other side: `step=<n>` answers with
`pendingSteps` and returns BEFORE the steps run, and they drain at the
idle cadence, so a debug read right after the POST races them (a 120-step
request had advanced a glide by a few frames' worth when read at once,
and had landed after a 3 s wait). debugging.md's "freeze, step, snapshot"
reads as synchronous; either the endpoint blocks until its queue drains,
as `/input` does for its events, or the guide says to wait.

Either the runtime's idle-tick gate is over-gating a standing request
(the `idle-tick runaway` fix, or the frame-pacing policy work in
progress), or the contract in debugging.md and core's `onFrame` doc no
longer holds and needs rewriting - one of the two must give.

## Done looks like

The probe ticks at the refresh rate, or the docs say what a standing
request does get and how a loop from rest should prime itself.
