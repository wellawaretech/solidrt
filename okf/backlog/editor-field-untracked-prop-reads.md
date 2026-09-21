---
title: TextInput logs STRICT_READ_UNTRACKED for a ref and for focus moved from an effect
description: EditorField reads props.ref in its ref callback and props.onBlur in its blur handler, both of which can run in an untracked owned scope, so a ref'd field warns at every mount and autoFocus warns whenever another field was focused; done means neither warns, in EditorField and every wrapper that forwards ref.
created: 2026-09-21
---

# TextInput logs STRICT_READ_UNTRACKED for a ref and for focus moved from an effect

## Symptom

Reported in [[quartz-heron]] items 2 and 3.

- Every mounted `TextInput` with a `ref` logs `[STRICT_READ_UNTRACKED]
  Reactive value read directly in <EditorField> will not update.`, once
  per field per mount.
- With one field focused, mounting a second with `autoFocus` logs the "in
  an effect callback" variant. With nothing focused beforehand it does
  not.

## Cause

- The ref callback in packages/components/src/editor-field.tsx calls
  `props.ref?.(n)`: a prop read in an owned, untracked scope. `TextInput`
  forwards its own `ref` through the same path.
- `autoFocus` calls `setFocus` in an effect's apply phase. `setFocus` runs
  the previously focused field's blur handler synchronously, and
  `handleBlur` reads `props.onBlur`, so the read lands inside the caller's
  effect. Any app code calling `setFocus` from an effect hits the same.

## Done looks like

Neither case warns. Two candidate fixes, to be chosen by where the
problem really lives:

- read the handler and ref props under `untrack` in EditorField (and in
  every component that forwards `ref` or calls handler props from a
  callback; sweep packages/components for the pattern), or
- dispatch focus and blur handlers outside the caller's scope in core
  (`setFocus` in packages/core/src/window.ts), which fixes the second case
  for every focusable, not only EditorField.

The first is local and certain; the second changes when blur handlers run
relative to `setFocus` returning, which callers may rely on. Check the
Solid 2 cheat sheet on reading props in event-like callbacks before
choosing.
