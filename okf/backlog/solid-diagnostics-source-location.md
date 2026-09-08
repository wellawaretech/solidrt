---
title: Reactivity diagnostics carry no source location
description: A STRICT_READ_UNTRACKED warning names the shape of the mistake but not the file or line, so finding it in an app with a dozen effects is a manual hunt; the diagnostics are subscribable and the dev server already remaps stack frames, so a location is attachable on our side.
created: 2026-09-08
---

# Reactivity diagnostics carry no source location

## Symptom

```
[STRICT_READ_UNTRACKED] Reactive value read directly in an effect callback
will not update. Move it into a tracking scope (JSX, a memo, or an effect's
compute function).
```

The text is right and the pointer to the reactivity-diagnostics skill is
useful, but there is no `.tsx:line`. In an app with a dozen effects the
warning says only that one of them is wrong, and the reporter had to
re-derive which by reading all of them. Both of theirs turned out to be the
same shape - a helper called in an effect's APPLY phase reading a signal
untracked - which a location would have shown in seconds.

The same gap applies to the other diagnostic codes
(`PENDING_ASYNC_UNTRACKED_READ` and friends).

## Done looks like

A dev-only subscriber in `@solidrt/core` that attaches a captured stack to
each diagnostic and prints it, the way a rejected property already does
(`setTreeProperty` in renderer.ts warns with `new Error().stack`, and the
dev server remaps those frames to the .tsx source).

The mechanism exists on both ends:

- `solid-js` exports `DEV`, whose `diagnostics.subscribe(listener)` fires
  per event with the code, kind, severity and message. Nothing in this
  repo subscribes today.
- The dev server's frame remapping is already in the path for property
  rejections, so the location arrives as app source, not bundle offsets.

Open question the work has to settle: the listener fires from inside the
read, so the stack is the right one, but the call site's own
`console.warn` happens after `emitDiagnostic` returns - the location has
to land WITH the warning rather than a line above or below it.
`diagnostics.setConsoleFooter` exists for exactly that adjacency and may
be the cleaner hook than `subscribe`.

Involves: `packages/core/src/` (a dev-only module, folded out of
production bundles like the leak sentinel is), and a check that the
remapper handles the frames this produces.
