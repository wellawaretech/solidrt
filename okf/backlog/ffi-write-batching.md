---
type: backlog-item
title: "FFI write batching: interned keys, batched creation, command buffer"
description: Every property write is one string-keyed FFI call (mount fans a props object into per-prop calls; update bursts pay per-call overhead N times); three stages reduce it - intern prop names to ids, createNode with a props object, and a command buffer whose props land in a shared buffer Rust reads directly, drained once per flush.
status: open
timestamp: 2026-08-05T00:00:00Z
---

# FFI write batching: interned keys, batched creation, command buffer

## Problem

Ranked the top structural cost in the app-structure review
(okf/analysis/app-structure-performance.md, finding a): every `setProperty`
marshals the property name as a fresh Rust String, then `apply_jsx`
(flux/src/plugins/gui/properties/mod.rs) walks chained string matches.
Mounting a node with 10 props is 11 FFI calls; a 200-item list with 5 nodes
each is thousands, each with string allocation and linear dispatch, landing
exactly where QuickJS (no JIT) already hurts most. After mount, update
bursts (animation frames, interaction effects) pay the fixed per-call cost
(rquickjs call overhead, arg marshalling, exception plumbing, RefCell
borrow, latch store) once per prop instead of once per burst.

Sequencing decision: generic before specific. This track benefits every
write - mount, updates, and animations alike - so it goes before the
animation-specific compositor-side-animation work (finding b in the same
analysis), which remains the endgame for frame-frequency motion but is a
separate item.

## Verified constraints (2026-08-05)

- A natural batch boundary exists. Writes only happen in synchronous
  bursts: effects run inside a Solid flush, and window.ts `runFrame` calls
  `flush()` then `renderFrame()`; event-driven updates flush on a
  microtask. Same shape as renderer.ts's `pendingDestroy` sweep.
- `setProperty` returns nothing to JS (flux gui tree.rs), so no return
  value forces synchrony. `request_frame` is an atomic latch consumed once
  per render tick (alloy rendertree platform.rs), so N latch-sets
  collapsing to one at drain time changes nothing observable.
- Drain timing is the sharp edge: `renderFrame()` runs synchronously right
  after `flush()`, before any microtask, so a microtask-only drain paints a
  stale tree. The drain must run explicitly in `runFrame` between `flush()`
  and `renderFrame()`, with the microtask drain as the fallback for bursts
  outside the frame path.
- `getBoundingBox` needs no drain barrier: it reads layout computed at the
  last produced frame, so pending writes are invisible to it today too.
- Buffered writes can outlive their node (write buffered, node removed and
  destroyed, then drain) - an interleaving that cannot happen today. That
  path hits `node_mut`'s expect panic (alloy rendertree tree.rs); the
  drain must skip unknown ids via a non-panicking edit variant (native
  Rust API on rendertree, engine-independence holds). A write to a dead
  node is semantically a no-op.
- The error path moves. Today a rejected prop throws per call and
  renderer.ts warn-onces on "unknown property"/"detached-only". A drain
  cannot throw mid-batch (later writes would be lost); it returns an
  [index, message] error list and JS reruns the same filter per item. DX
  cost: the writer's stack is gone at drain time - the warn still names
  element kind + prop; exact stacks via a dev flag that bypasses the
  buffer.

## Stages

1. **Interned property keys.** JS resolves name -> small integer id once
   per unique name (one Map lookup thereafter); the native side dispatches
   on the id through a table instead of chained string matches. Kills the
   per-call String allocation and linear dispatch. Mechanical,
   independently shippable.
2. **Batched node creation.** `createNode(id, kind, propsObject)`: one
   crossing per node at mount instead of one per prop. The universal
   renderer already hands static props to `createElement` as a single
   object (renderer.ts); today they are fanned out into per-prop calls.
3. **Command buffer.** Prop writes append to a shared ArrayBuffer that
   Rust reads directly - no per-value rquickjs marshalling - drained in
   one FFI call per flush window. Interned numeric props (the
   animation-frequency set: transforms, opacity, x/y, colors as packed
   u32) encode as (nodeId, propId, value) records; strings and objects go
   through a side list or UTF-8 in the buffer. Apply in FIFO order (later
   write wins by construction; no dedupe needed - fine-grained reactivity
   writes each changed prop once per flush). One `request_frame` per
   drain. A plain-JS-array drain (one call, per-value marshal) is an
   acceptable stepping stone that already yields the one-call-per-flush
   shape, but the direct-read buffer is the design goal. Structural ops
   (create/insert/detach) can join the same buffer later, at which point
   stage 2 becomes a special case of it.

Layering holds throughout: the drain loop is marshalling (flux gui
plugin), `apply_jsx` stays the per-item decoder, rendertree stays
engine-free (it only gains a non-panicking edit variant, native types
only).

## Measurement

Before and after each stage: the SETPROP_COUNT overlay (flux gui tree.rs;
keep counting logical writes, not drain calls) plus record/playback for a
deterministic trace. Capture a baseline on a component-heavy example
first. Before building stage 3, bound the ceiling with a microbench: 10k
individual `setProperty` calls vs one batched drain of the same writes.

## Deliberately out of scope

- Compositor-side animations and native scroll physics (findings b and c
  in the analysis): frame-frequency work leaving JS entirely. Separate
  items, sequenced after this generic track.
- Event-object garbage (finding d): unrelated path.
- Bulk reads (a `getBoundingBoxes(ids)` query): mentioned in the
  pointer-event-interest-mask item; reads are not the hot path here.
