---
type: backlog-item
title: overflow + viewBox clips the wrong rectangle
description: The overflow clip rect took the element's box extent as a raw number and applied it in the child's design (pre-viewBox) space, so a magnifying fit escaped the clip entirely and a minifying one cropped content early. Reported three times across four months (marble-fox F1, paper-crane 1, the unimog postmortem that supplied the root cause) with no dedicated item until now. Fixed 2026-08-08 - the clip is emitted in box space, under the user chain before the fit, on both the paint and hit paths, pinned at both scales.
status: done
timestamp: 2026-08-08T00:00:00Z
---

# overflow + viewBox clips the wrong rectangle

Promoted out of the feedback reports 2026-08-08. The defect had been reported
three times across four months while living only in `[open]` feedback lines
and a follow-up note in [[mcp-verification-surface]] - which is likely why it
kept coming back rather than getting fixed (the unimog postmortem's
observation, and the reason this file exists).

## The rule

Verified in both directions by the unimog postmortem
(`~/solidrt/demoes/unimog/POSTMORTEM.md`, headless probes on 0.0.46-12):

> The `overflow` clip rectangle takes its extent from the element's box **as a
> raw number** and applies it in the child's **design (pre-`viewBox`)
> coordinate space**. It is never divided by the viewBox fit scale
> `s = box / design`.

The origin is correct - (0,0) is the same point in both spaces - so only the
extent is wrong, and the sign of the error flips with `s`:

| `s` | Clip extent vs design space | Symptom |
| --- | --- | --- |
| `> 1` (magnifying) | too large | nothing clips; content escapes the box |
| `< 1` (minifying) | too small | content cropped early (the unimog: 75% of the scene in 75% of the window) |
| `= 1` (no viewBox) | exact | correct - why `overflow` works everywhere else |

The `s > 1` direction also explains the two puzzling details in the first
report: negative coordinates still clip (the origin is right, only the far
edge is wrong), and small static cases do not reproduce (content must
overhang past the raw box *number* in design units before anything clips).

## Sightings

- [[marble-fox]] F1 (0.0.44, 2026-08-04) - *"`overflow: "hidden"` does not
  clip a detached subtree past the box's bottom edge."* The `s > 1`
  direction. The reporter's hypothesis ("right origin, extent not derived
  from the element's box") was close: the extent IS the box's - it is just
  never converted into design space.
- [[paper-crane]] 1 (0.0.46, 2026-08-07) - narrowed it to
  viewBox-on-the-clipping-node with a three-case repro (detachedness is not
  the trigger; overflow alone clips fine; viewBox on an inner view clips
  fine), and noted the failing combination is exactly the documented
  fixed-aspect authoring shape.
- The unimog postmortem (2026-08-08) - the `s < 1` direction, previously
  unseen, which with both directions in hand gave the root cause. A minimal
  two-direction reproducer and the `<texture>` unit-contract probes are in
  the postmortem.

## Root cause in code

In `alloy/src/rendertree/composite.rs`, `record_node` emitted the overflow
clip AFTER the viewBox fit transform (`View::build` applies the composed
fit-then-user matrix in one call), so the box-sized rect `(0,0,w,h)` was
interpreted in design units. Meanwhile `draw_cached_recording` - the
Recording-boundary composite path - applied its hoisted clip BEFORE the
cached fitted content, i.e. in box space, correctly. The two paths disagreed,
which is the likely shape behind marble-fox's *"`srt render` clips correctly
while the client does not"* (boundary vs non-boundary recording, so
environment- and version-dependent). The hit-side overflow gate mirrored the
recorded (wrong) form and carried a comment calling the semantics unsettled.

## Fix (2026-08-08)

Settled: **the overflow clip means the element's layout box, in box space, on
every path.**

- **Paint** (`composite.rs`): on a fit-carrying View, `record_node` splits
  the composed matrix around the clip - user chain, clip, then fit - the
  order `draw_cached_recording` already used, so the three record/composite
  paths now agree. Rounded clip radii stay in box units by construction.
- **Hit** (`hit.rs`): the overflow gate maps the design-space local point
  forward through the fit before comparing against the box, mirroring the
  paint side; the "unsettled" comment is gone.
- **Tests, both scales** (every prior repro was one-directional, and a fix
  validated only at `s > 1` could have shipped leaving `s < 1` broken):
  - hit gate at `s = 0.5` and `s = 2`:
    `alloy/src/tests/hit.rs` (`overflow_gate_is_box_space_under_*`);
  - the rendering counterpart as a pixel-asserting example capturing a
    parent that composites one clipping plate per direction:
    `alloy/examples/overflow_viewbox.rs`. Verified to fail on the pre-fix
    code at exactly the postmortem's symptom (minifying: bottom-right of the
    box transparent; the hit test fails there too).
  - `flux/src/tests/properties.rs` already pins the overflow+viewBox props
    round-trip from paper-crane (the tooling half).

## Follow-up (2026-08-08, same day): scroll settled, record order linearized

Review of the fix found the identical divergence shape one op over: scroll on
a viewBox view was applied in box pixels by the Recording-boundary composite
(before the cached fit) but in design units by the non-boundary and snapshot
recordings (after the fit) and by the hit descent - so a scrolled viewBox
view would jump by the fit ratio when gaining or losing
`repaintBoundary="recording"`.

Settled: **scroll means box pixels on every path** (it pairs with the
box-space clip; one scroll pixel slides content one box pixel regardless of
fit scale). Paint applies the raw translate between clip and fit; the hit
descent, locals projection and bounding-box ascent convert with
`View::content_scroll` (offset divided by the fit scale, since those walk in
design space). Pinned at both scales by `scroll_is_box_pixels_under_*` in
`alloy/src/tests/hit.rs` and `bounding_box_scrolled_view_box_ancestor` in
`alloy/src/tests/tree.rs`.

With clip and scroll both under the user chain, `record_node` collapsed to
one linear record order - user matrix, clip, scroll, fit, children - where a
hoist is always a prefix of the first three and the fit is never hoisted
(it is content, recorded into caches and captures at every hoist level).
`View::build` no longer applies any matrix: the compositor owns the split
(`composite::own_matrix` + the recorded fit), so no composed-transform path
can put the clip or scroll in the wrong space again. The View matrices also
now resolve against the border box on every path, closing the View half of
[[padding-box-divergence]].

## Adjacent, not tracked here

- The tooling gap that made the bug undiagnosable (`props: true` omitting
  `overflow`) was fixed under [[mcp-verification-surface]] 2026-08-08.
- The unimog postmortem's remaining recommendations are separate asks,
  unfiled as of this writing: a paint-time warning when a painted rect
  exceeds its clip box, a painted-extent/`clipped` flag in
  `get_render_tree`, the `Scene` width/`pixelRatio` split in `@solidrt/3d`,
  and the viewBox/`<texture>` unit wording in the docs.
