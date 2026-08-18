---
title: "FFI write batching: batched creation, one-call drain, interned keys, command buffer"
description: Every property write is one string-keyed FFI call (mount fans a props object into per-prop calls; update bursts pay per-call overhead N times); four stages reduce it - createNode with a props object, a one-call-per-flush drain, interned prop ids with table dispatch, and a command buffer whose props land in a shared buffer Rust reads directly.
created: 2026-08-05
---

# FFI write batching: batched creation, one-call drain, interned keys, command buffer

## Problem

Ranked the top structural cost in the app-structure review
(okf/notes/app-structure-performance.md, finding a): every `setProperty`
marshals the property name as a fresh Rust String, then `apply_jsx`
(flux/src/alloy_plugins/properties/mod.rs) walks chained string matches.
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
  renderer.ts warn-onces on "Unknown property"/"Detached-only" (bad VALUES
  for known properties rethrow since the 2026-08-06 fail-soft decode work;
  a drain design must preserve that split). A drain
  cannot throw mid-batch (later writes would be lost); it returns an
  [index, message] error list and JS reruns the same filter per item. DX
  cost: the writer's stack is gone at drain time - the warn still names
  element kind + prop; exact stacks via a dev flag that bypasses the
  buffer.

## Stages

Order (revised 2026-08-18): batched creation first, then the one-call
drain, then interned keys, then the direct-read buffer. Interned keys were
originally listed first, but on today's per-call path the fixed crossing
cost (rquickjs call, arg marshalling, exception plumbing) dwarfs one short
String allocation and a linear match, so their payoff is invisible until
the crossings collapse. After the drain, they are exactly the per-write
cost that remains, and they shape the buffer: with ids a record is plain
numbers `(nodeId, propId, value)` and no name side-table is needed (only
string/object values need one), so interning lands before the buffer is
designed. They also simplify the native side: a `PropId -> handler` table
replaces the four-level fallthrough in `apply_jsx` (element-level,
per-kind, paint, layout), the interning table becomes the single registry
of which props exist for which kinds, and "Unknown property" /
"Detached-only" become lookup results instead of the end of a match chain.

Note that `try_edit` (the non-panicking edit a drain needs to skip writes
to nodes destroyed before the drain) already exists on rendertree.

1. **Batched node creation.** `createNode(id, kind, propsObject)`: one
   crossing per node at mount instead of one per prop. The universal
   renderer already hands static props to `createElement` as a single
   object (renderer.ts); today they are fanned out into per-prop calls.
   The renderer keeps routing events/focusable/hints/color parsing on the
   JS side and passes only tree-bound props. Rust applies in order and
   returns a `[name, message]` list of rejections; JS runs each through the
   same warn-once vs rethrow classification. This establishes the
   error-list contract the drain reuses.
2. **One-call drain.** Prop writes append to a JS array of
   `[nodeId, name, value]`; one `applyProps(batch)` call per flush window,
   drained explicitly in `runFrame` between `flush()` and `renderFrame()`,
   microtask drain as fallback. Apply in FIFO order (later write wins by
   construction; no dedupe needed - fine-grained reactivity writes each
   changed prop once per flush). One `request_frame` per drain. Still
   per-value marshalling, but the crossing count is already the final
   shape. Measure here before going further.
3. **Interned property keys.** JS resolves name -> small integer id once
   per unique name (one Map lookup thereafter); the native side dispatches
   on the id through a table instead of chained string matches. Kills the
   per-write String allocation and linear dispatch, and defines the record
   format for stage 4.
4. **Direct-read command buffer.** The batch moves to a shared ArrayBuffer
   Rust reads directly - no per-value rquickjs marshalling. Interned
   numeric props (the animation-frequency set: transforms, opacity, x/y,
   colors as packed u32) encode as `(nodeId, propId, value)` records;
   strings and objects go through a side list or UTF-8 in the buffer.
   Structural ops (create/insert/detach) can join the same buffer later,
   at which point stage 1 becomes a special case of it.

Layering holds throughout: the drain loop is marshalling (flux gui
plugin), `apply_jsx` (or its table successor) stays the per-item decoder,
rendertree stays engine-free (native types only).

## Measurement

Before and after each stage: the SETPROP_COUNT overlay (flux gui tree.rs;
keep counting logical writes, not drain calls) plus record/playback for a
deterministic trace. Capture a baseline on a component-heavy example
first. Before building stage 4, bound the ceiling with a microbench: 10k
individual `setProperty` calls vs one batched drain of the same writes.

## Deliberately out of scope

- Compositor-side animations and native scroll physics (findings b and c
  in the analysis): frame-frequency work leaving JS entirely. Separate
  items, sequenced after this generic track.
- Event-object garbage (finding d): unrelated path.
- Bulk reads (a `getBoundingBoxes(ids)` query): mentioned in the
  pointer-event-interest-mask item; reads are not the hot path here.
