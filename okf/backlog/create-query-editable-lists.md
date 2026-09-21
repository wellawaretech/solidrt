---
title: createQuery rows remount the editors of an editable list on each write
description: A reactive query returns fresh row objects on every re-run, so a keyed For of editors over it remounts every editor on each write and loses focus and caret; done means an editable list over a query keeps its editors, by keyed reconciliation or at least a documented pattern.
created: 2026-09-21
---

# createQuery rows remount the editors of an editable list on each write

## Symptom

An app with one editor per database row did not use `createQuery`
because each write re-runs the query, which returns fresh row objects, and
a keyed `<For>` over them remounts every editor, losing focus and caret.
It loaded the rows once into a store and wrote through instead. Inferred
from the code, not tried. Reported in [[quartz-heron]] item 9.

## Done looks like

First confirm the remount with a probe (a `<For>` of `TextInput`s over a
`createQuery`, type into one). If confirmed:

- a `key` option on `createQuery`/`createQueryRow` (e.g. `key: "id"`) that
  reconciles re-run results into a store by that key, so unchanged rows
  keep identity and changed rows update in place, the way Solid's
  `reconcile` does; or
- if that is not wanted, a note in the data docs pointing editable UIs at
  load-then-write-through.

Involves: `@solidrt/core/data`, its docs.
