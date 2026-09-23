---
title: SegmentedControl warns STRICT_READ_UNTRACKED per option
description: "Mounting a SegmentedControl logs Solid's STRICT_READ_UNTRACKED warning once per option (the player's settings screen shows three); the option row's For callback reads the controlled value untracked at its top, so the warning may be naming a real stale read."
created: 2026-09-23
---

# SegmentedControl warns STRICT_READ_UNTRACKED per option

Open the player's settings screen with a dev client and `/logs` gains three
entries of

```
[STRICT_READ_UNTRACKED] Reactive value read directly in <For> will not update.
Move it into a tracking scope (JSX, a memo, or an effect's compute function).
```

one per theme-mode option. Seen 2026-09-23 on the player before and after
its routing migration, so it is the component's, not the app's. When the
settings screen is the first screen (a launch link or a reload re-entry
at `/settings`) the batch appears twice: once at mount and once more
right after the runtime's input-devices fact arrives, so the option rows
are created again then.

`packages/components/src/segmented-control.tsx` renders the options with a
`For` whose callback sets up a `createPress` per option and reads the
controlled value through `value()` (`props.value`, else the internal
signal). A read of `props.value` at the callback's top runs in the item's
owner, not a tracking scope, which is exactly what the diagnostic says. If
that read feeds the selected look, the look would not follow a value
change; the control does visibly update today, so the read may be a
duplicate of a tracked one in the JSX below, in which case only the warning
is wrong.

Done: no warning on mount, and a check that the selected segment still
follows a controlled `value` change and a keyboard cycle.
