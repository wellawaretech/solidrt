---
title: Where pointer position filtering belongs after frame batching
description: Gesture recognizers still run an EMA over pointer positions that predates frame-batched input; whether same-age batches make it unnecessary, and where filtering should live if a noisier device needs it, was never settled on device.
created: 2026-08-13
---

# Where pointer position filtering belongs after frame batching

What it looks like when this matters: a drag on a noisy touchscreen either
jitters (no filtering) or lags the finger (too much filtering), and the knob
that decides which is buried in a recognizer rather than owned anywhere.

Frame-batched input changed the premise the current filter was written under -
samples in a batch are now the same age, so the raw span may be clean enough on
its own. Three questions, unanswered because they need a device:

- **Does dropping the EMA hold on device?** The captured tablet showed
  +-1-3px/event of sensor dither. Needs an on-device A/B with the transform.ts
  filter behind a flag - not a desk judgement.
- **Should present-per-frame be enforced at the same time?** Apps currently
  choose when to push camera updates. Now that input arrives at frame cadence
  anyway, that may want to stop being app discipline.
- **Runtime or recognizer?** If a noisier device shows dither that survives
  batching, filtering could move to the runtime (One Euro over the samples,
  every consumer benefits) instead of staying per-recognizer, which is today's
  answer.

Promoted out of [frame-batched-pointer-input](../done/frame-batched-pointer-input.md)
when okf was restructured: the work is finished, these questions are not, and a
done record is the wrong place to keep them. The coalescing rules that ARE
settled are in
[what must not be collapsed](../notes/pointer-coalescing-traps.md).
