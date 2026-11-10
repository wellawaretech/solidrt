---
title: Decide whether scroll and JS-driven motion drop pixel rounding
description: After pixel-snap-at-draw a transform track moves its subtree unrounded, but a scroll offset still moves by whole device pixels and a value JS writes every frame (a pan, a pinch) still rounds, so a fling's tail steps and a pinched scale keeps a half-pixel wobble; neither is measured, unrounded scroll softens text and reverses a decision browsers share, so a frame-by-frame read comes first, and the follow spec waits on it; `pixelSnap={false}` of pixel-snap-at-draw is the answer where a subtree should never round.
created: 2026-11-10
---

# Decide whether scroll and JS-driven motion drop pixel rounding

## Symptom

okf/done/pixel-snap-at-draw.md drops the rounding under a running track
on a transform. Two kinds of motion stay rounded after it:

- A scroll offset. Under the wheel's spring or a fling's tween the content
  moves by whole device pixels. In the slow tail of a fling it steps one
  pixel every few frames.
- A value JS writes every frame: a pan under a finger (`ScrollView`
  withdraws its scroll entries during a drag, so the drag is a direct
  write), a pinch, a scrubbed progress. It has no track, so it rounds. A
  translate steps by whole device pixels, and a scale about the centre
  keeps the back-and-forth of up to 0.5 device px that one rounding leaves.

Neither is measured. Both are read off the rule, not off a frame-by-frame
read like the one in pixel-snap-at-draw.

## Why this is its own item

- Whole-pixel scroll was a decision
  (okf/done/pixel-snapped-paint-boxes.md), and it is what browsers do.
  A rounded movement only ever steps forward, so it cannot wobble, which
  was the symptom pixel-snap-at-draw fixes.
- Unrounded, a text's quad is sampled with linear filtering at a fraction
  of a pixel, so text is soft for as long as the scroll runs. A 180 ms pop
  hides that; a scroll the user reads during does not. A 1 px divider
  spreads over two rows and back as the fraction moves. Below display
  scale 2 this is expected to show.
- It costs a new transition spec, a hold constant, a `ScrollView` change,
  and a re-record at the end of a fling for every row recorded during it.

## First step: read it

Add a debug switch that puts `ScrollX` and `ScrollY` in the motion set of
pixel-snap-at-draw. Read a fling's tail and a wheel glide frame by frame,
rounded and unrounded, at display scale 1, 1.5 and 2: the movement per
frame, and the partial coverage on text stems and on a 1 px divider.

Decide from that read. If rounded scroll holds up, the scroll half closes
as a note under `okf/notes/`: a limitation, with the read as its evidence.

## Done looks like

If the read supports it:

- A running track on a scroll offset drops the rounding for the
  scroller's children. The scroller's own box and clip do not move, so
  they stay rounded. Scrolling text moves smoothly.
- A finger drag on a `ScrollView` goes through the transition engine, is
  smooth, and adds no frame of lag.
- A value written every frame under a follow spec moves unrounded and
  rounds again when the writes stop.

## What it involves

### Scroll in the motion set

`ScrollX` and `ScrollY` join the props whose running track sets the
`unrounded` flag, for the children only. Rows recorded under that motion
re-record once when it ends, by the rule pixel-snap-at-draw gives
recordings; after a fling that is every row that came into view during it,
in one frame.

### A follow spec

An ordinary track does not do for a value that follows input:
`transition_write` only retargets, so the value first moves at the next
`advance_transitions`, a frame behind the finger at best.

The engine gains a follow spec. A write lands on the value in the same
frame, and the track stays running until no write has come for
`FOLLOW_HOLD_MS`, a named constant past the gap of touch input slower than
the display, so a frame with no move does not bring snapping back mid-drag.
Then it settles like any track. `viewportTransition()` in
`packages/components/src/scroll-view.tsx` declares it on `scrollX` and
`scrollY` during a pan, in place of dropping them. The spec goes through the
transition decoder and the core types.

The follow spec is not tied to scroll: declared on a scale or a translate
it is the answer for a pinch or a drag, smooth while the writes come and
crisp at rest.

### The override is not this item

`pixelSnap={false}` is part of okf/done/pixel-snap-at-draw.md: it turns
the rounding off for a subtree, at rest too. A pan or a pinch can take it
today and move smoothly, at the cost of crispness at rest. It cannot give
smooth in motion and crisp at rest; the follow spec does, and that is what
this item is still for.

## Prior art

Browsers round scroll offsets to device pixels. `text-rendering:
geometricPrecision` and SVG's `shape-rendering` are per-element overrides.
WPF's `SnapsToDevicePixels` is inherited down the tree.

## Related

- okf/done/pixel-snap-at-draw.md: the rule, the `unrounded` flag this
  extends, and the `pixelSnap` override. It lands first.
- okf/done/pixel-snapped-paint-boxes.md: where whole-pixel scroll was
  decided.
