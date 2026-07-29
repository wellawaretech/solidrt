---
type: backlog-item
title: captureSnapshot on detached (d-*) nodes
description: A d-* node is drawn but has no layout entry, so every capture of one rejected as zero-sized; captures now size from the node's painted box, sourced so it cannot diverge from what the paint path uses.
status: done
timestamp: 2026-07-29T00:00:00Z
---

# captureSnapshot on detached (d-*) nodes

Raised by the shadertoy field report (projects/shadertoy/SOLIDRT-FEEDBACK.md
#6): `get_render_tree` reported the `d-texture` node as 1692x1128, capturing
that exact node failed with "capture node has no layout box (zero size)", and
the MCP `get_snapshot` doc had explicitly advised capturing the smallest node
(the `<texture>` leaf itself). Two tools disagreeing about the same node reads
as a bug.

Shipped 2026-07-29 (first pass): only the error message, which named the
detached case and pointed at the laid-out ancestor, plus the same rule on
`captureSnapshot` in flux-types/core and in the MCP tool description.
`captureSnapshot`'s doc also said "detached" meaning UNMOUNTED, colliding with
`d-*` meaning detached from layout; that is disambiguated.

Shipped 2026-07-29 (behaviour, the proposed shape below): `service_captures`
sizes a detached capture from `element.kind.local_bounds(ctx.size)` - the
verify-first check held: `service_captures` runs at the top of
`build_recursive`, immediately after the caller's child walk set `ctx.size`
for this exact node, so the fallback equals what `build()` reads by
construction. The `-(x, y)` counter-translate applies to non-View kinds only:
a View's offset (translate) lives in the matrix that `Hoist::Transform`
already keeps out of the recording. Zero painted size still rejects, with a
message naming the detached case. Doc text in flux-types/core/MCP updated to
the new contract. Verified by `alloy/examples/capture_detached.rs`: offset
d-rect with w/h (offset countered), full-bleed d-rect (ancestor box, no
siblings), d-view with rotate (matrix hoisted), zero-size rejection,
unchanged laid-out capture, and display scale 2 (catches scale/translate
ordering). The ink-extent question below remains explicitly out of scope.

## Why the current answer is weak

`service_captures` (alloy/src/rendertree/composite.rs) sizes captures from
`element.layout`, which a `d-*` node never has - that is the definition of the
primitive, not a degenerate case. So the workaround is "capture the nearest
laid-out ancestor", and that ancestor is usually a container holding other
children: you asked for a leaf and got its siblings composited in. It defeats
the smallest-node workflow for exactly the primitives the performance notes
push authors toward.

The node is nonetheless drawn, into a definite rectangle. There is something
real to capture.

## Two cases

- **Own `w`/`h` set** (`<d-rect x={10} y={20} w={40} h={40}>`): the box is
  fully determined by the node's own props. No ancestor geometry is involved.
- **No `w`/`h`** (a full-bleed `<d-texture>`, the shadertoy case): the size
  comes from the nearest laid-out ancestor - parent, grandparent, any depth
  (`RenderTree::content_fallback` walks up). At paint time the same thing
  happens via `ctx.size`: `Texture::build` does `w = self.w.unwrap_or(ctx.size.w)`.

An earlier attempt was reverted for calling case 2 illegitimate because the
size is "layout-derived". That reasoning is too strong: the inherited size is
not invented for the capture, it is the node's real drawn extent.

## The mistake worth not repeating

That attempt sized from `content_fallback` while the paint path sizes from
`ctx.size`, assuming without checking that the two always agree. If they ever
diverge the capture is silently cropped or padded - worse than a clean
rejection, because it fails quietly.

The fix is to stop having two size sources. Size from
`element.kind.local_bounds(ctx.size)`, reading `ctx.size` before `record_node`
runs, so the capture box equals the painted box by construction rather than by
assumption.

## Proposed shape

1. Verify first: confirm `ctx.size` at `service_captures` time is the value
   `build()` would use for this node. This single check decides whether the
   approach is sound; it is what the reverted attempt skipped.
2. Size the capture from `local_bounds(ctx.size)`, and translate the sub
   display list by `-(x, y)` so the node's content lands at the texture origin
   (a `d-*` node paints at its own offset, which a capture must not bake in).
   Detached children position relative to the parent, so the translate covers
   the subtree.
3. Keep the improved rejection message for what remains genuinely uncapturable
   (no painted size at all, e.g. before the first layout populates the cache).
4. Verify against both cases - an offset `d-rect` with explicit `w`/`h`, and a
   full-bleed `d-texture` - comparing each capture with the same content laid
   out normally. Check the `hoisted_matrix` interaction too: it already runs
   for captures, so a `d-view` with rotate/scale has its transform hoisted.

## Open questions

- **Is the case-2 size acceptable?** A full-bleed `<d-texture>` captures at its
  laid-out ancestor's dimensions. Defensible (it is the real drawn size) but it
  will surprise anyone reading "detached" as "has no size".
- **Shapes capture their drawing surface, not their ink.** A `d-path` squiggle
  captures as a large mostly-empty texture, because `local_bounds` for Path is
  the fallback box. Laid-out nodes behave the same way, so this is consistent
  rather than a regression - but if "smallest node" should mean actual ink
  extent, that is real paint bounds off the display list, a separate and bigger
  change. Explicitly out of scope here.

Related: [[capture-pixels-round-trip]] (the other open captureSnapshot shape
question) and [[mcp-agent-loop-improvements]] (the `get_snapshot` tool text
that pointed at the leaf in the first place).
