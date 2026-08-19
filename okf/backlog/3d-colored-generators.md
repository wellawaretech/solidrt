---
title: Colored geometry generates twice
description: withColors throws on anything but standard-layout input and copies into a fresh buffer, so building coloured geometry always generates twice; a layout option on the generators or a colored flag would remove the copy. Split from 3d-geometry-ops when that shipped 2026-08-19.
created: 2026-08-19
---

# Colored geometry generates twice

Symptom: every coloured mesh is built as a standard-layout geometry and then
re-packed by `withColors` into a 12-float interleave - two allocations and a
copy per geometry, for the layout the generator could have emitted directly.

Where it lives: the generators in `packages/3d/src/geometry.ts` all emit the
standard layout; `withColors` is the only path to "colored" and it copies.

## Done looks like

A generator can emit colored-layout vertices in one pass - either a `layout`
option shared by the generators or a `colored: true` flag - and `withColors`
stays for hand-built / already-generated geometry. `fillColors` already
writes colour slots in place, so the generator only has to reserve the
stride.
