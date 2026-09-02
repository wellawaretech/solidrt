---
title: Postmortem - a bad GPU counter steered a day of TV perf work
description: gpuFrameExecMs on the MediaTek TV produced a plausible-looking "40 ms GPU fill" number that spawned a mis-attributed backlog item, a probe implementation, and a design debate before saturation measurement showed the cost was CPU-side display-list walking; the trap was documented six days earlier but left armed.
created: 2026-09-02
---

# Postmortem: a bad GPU counter steered a day of TV perf work

The incident behind okf/backlog/display-list-op-cost.md (born as
"partial-repaint-android"), 2026-09-02. The instrument at fault is the
one okf/done/gpu-timer-attribution.md had already convicted on
2026-08-27.

## Timeline

1. **2026-08-27.** The gpu-timer-attribution investigation (Adreno
   tablet) establishes that `gpu*ExecMs` is not defensible on tilers
   and proposes three fix routes. None is decided or landed; `/stats`
   keeps serving the numbers with nothing at the point of use saying
   they cannot be compared.
2. **2026-09-02, morning.** TV verification of partial repaint quotes
   `gpuFrameExecMsPerFrame`: 20x20 mover 39-40 ms vs full window
   44-45 ms. Conclusion drawn: "fill cost is damage-size-independent,
   the multisampled fast path redraws everything". The number arrived
   with a story that fit the architecture, so it was not cross-checked,
   and a backlog item was created on that attribution.
3. **2026-09-02, afternoon.** Downstream of the phantom "GPU fill"
   cost: an eglSetDamageRegionKHR hint gets designed, implemented,
   deployed and measured (two TV build cycles); a rig-vs-fast-path
   design debate and a per-app-configurability discussion run on the
   assumption the trade-off exists. The broken counter "confirms"
   itself twice along the way (37.2 vs 39.0 hinted, 39.8 rig) - a
   wrong instrument is perfectly consistent with itself.
4. **The catch.** The rig measurement contradicts the model: partial
   repaint engages on every frame, damage is 660 px, and the cost does
   not move. That contradiction - not any warning - triggers reading
   the timer-attribution finding and switching to the defensible
   method (saturate via `/clock?scale=5`, fps = frame delta over
   timeMs delta). Two runs plus a 50-rect control probe then settle
   it: the cost scales with display-list op count on the armv7 CPU;
   window config and GPU are irrelevant. Total time from contradiction
   to attribution: under half an hour.

## Why the guards failed

- **A convicted instrument stayed armed.** The 08-27 item shaped the
  problem and proposed fixes, but no decision landed. Documentation of
  a trap is not disarmament: the next consumer meets the number where
  it is served (`/stats`, MCP get_stats, the HUD), not where it is
  documented.
- **The rule lived away from the point of use.** "Never quote
  gpu*ExecMs from a tiler" existed in a memory hook and a backlog
  file. Nothing in the stats output, the MCP tool description, or
  debugging.md flags the field. It was consumed unflagged twice in one
  day, by sessions that both knew of the timer item's existence.
- **Plausible-narrative capture.** The wrong number came with a
  correct-sounding mechanism (fast path draws full frames), and its
  two readings ordered the way the story predicted (mover < full
  window). Internally consistent and wrong.
- **Single-instrument conclusion.** The original claim rested on one
  counter. The cross-check that settled everything (saturation fps)
  costs five minutes and was available the whole time.

## What worked

- Stopping on contradiction instead of forcing the narrative: rig ==
  fast path made no sense under the fill model, and that was treated
  as a method problem, not noise.
- The 08-27 finding, once actually consulted, contained both the
  verdict and the correct method.
- A control experiment (the 50-rect sparse probe) converted suspicion
  into attribution in a single run.

## Cost and salvage

Cost: roughly half a day on wrong premises - a probe implementation
later removed, three extra TV build/deploy cycles, and design
reasoning about a trade-off that does not exist. Salvage: the
EGL_KHR_partial_update/EXT_buffer_age false-negative fix, the per-stack
EGL facts, a definitive "no" on the damage hint, and the real finding
(per-op DL cost) which matters more than what the item set out to fix.

## Actions

1. **Decide and land gpu-timer-attribution stage 1** (report the
   timers absent on tiler renderer strings, or keep only the per-frame
   total). This incident is now recorded in that item as the second
   concrete case of the numbers steering real work. Root fix: disarm
   the trap where it is served.
2. **Point-of-use guidance**: the item's "also worth doing" line for
   debugging.md and the MCP stats description (compare frame-counter
   delta over timeMs delta on tilers) should land with it.
3. **Method rule for perf claims**: a performance number that spawns a
   backlog item or a design decision needs a second, independent
   confirmation - a different instrument, or subtraction/saturation -
   and the item should record the method next to the number, so a
   later reader can judge whether the number survives.