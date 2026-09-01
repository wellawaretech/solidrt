---
title: Cheatsheet says untrack exempts owned-scope writes; the guard is owner-based and untrack cannot
description: REACTIVE_WRITE_IN_OWNED_SCOPE fires on the owner context, which untrack never touches (it only toggles tracking), so the cheatsheet's documented escape hatch ("move setters into untrack" / "untracked blocks") throws anyway; folded in, the thrown Error should name the owner it already computes for emitDiagnostic.
project: solid-js CHEATSHEET.md + @solidjs/signals core (github.com/solidjs/solid)
versions: solid-js / @solidjs/signals 2.0.0-rc.4; next branch checked 2026-08-31
status: resolved
link: https://github.com/solidjs/solid/issues/3157
created: 2026-08-31
---

# Cheatsheet says untrack exempts owned-scope writes; the guard is owner-based and untrack cannot

Found 2026-08-31 verifying external feedback on the stk2 project: a signal
write inside a component-body callback (a `ref`) throws
`REACTIVE_WRITE_IN_OWNED_SCOPE`, and following the cheatsheet's own fix -
wrapping the write in `untrack` - throws identically.

Two claims in `packages/solid/CHEATSHEET.md` on `next` (lines 515 and 648):

> **Write under owned scope** - move setters into event handlers /
> `onSettled` / `untrack`, or opt in with `{ ownedWrite: true }`.

> **No writes inside owned scope** - ... Move writes to event handlers,
> `onSettled`, or untracked blocks.

But the guard (`setSignal`, `packages/signals/src/core/core.ts:1263`) checks
the ambient owner `context` with three exemptions - `CONFIG_OWNED_WRITE`,
`CONFIG_CHILDREN_FORBIDDEN`, firewall - and `untrack` (core.ts:902) only
toggles `tracking`/`strictRead`; it never touches `context`. So `untrack`
cannot exempt a write, by construction. Verified on rc.4 under
`--conditions=development`: plain write in an owned scope throws, the same
write wrapped in `untrack` throws, the same write after the first `await` of
an async IIFE passes (the sync `context` global is gone by then).

```ts
import { createSignal, createRoot, createOwner, runWithOwner, untrack } from "@solidjs/signals"
let [_, set] = createSignal("")
createRoot(() => {
  runWithOwner(createOwner(), () => {
    untrack(() => set("x"))   // throws REACTIVE_WRITE_IN_OWNED_SCOPE
  })
})
```

The docs side is almost certainly the one to fix, not the guard: component
bodies already run wrapped in `untrack` (`createComponent` in solid's
dev.js), so if `untrack` cleared the guard it could never fire in a component
body at all - the scope where it is most needed. Ask: drop `untrack` /
"untracked blocks" from both cheatsheet lines, leaving event handlers,
`onSettled`, effect apply phase, and `ownedWrite`. If untrack-exempts-writes
is actually the intended semantic, then the guard needs to consult
`tracking`, and the component-body wrapping needs rethinking - but that reads
like a much bigger change than the doc edit.

Folded-in enhancement, kept short: both throw sites compute
`ownerId`/`ownerName` for `emitDiagnostic` two lines above the `throw`, then
throw the generic constant. Appending the owner name to the thrown message
("... inside <Scene>") would tell the developer which scope caught the write;
today that name reaches only the diagnostics channel, which apps do not
subscribe to by default.

On our side: `packages/core/AGENTS.md` (line ~322) and the scaffold AGENTS.md
(trap 2) repeat the cheatsheet's `untrack` advice verbatim and need the same
edit; a dev-mode diagnostics listener in `packages/core` could print the
owner name without waiting on upstream. The ref-callback-is-an-owned-scope
documentation gap from the same feedback is ours, not upstream's (the refs
are invoked in our 3d components' bodies).

Outcome 2026-08-31 (same day): fixed on `next` in 3e3676b0. Both cheatsheet
lines now state explicitly that untrack does NOT exempt owned-scope writes
(event handlers, onSettled, ownedWrite remain the escape hatches), settling
the docs as the wrong side. The enhancement landed too: the thrown message
now appends the owning scope's name. Not in any release yet (rc.4 is the
newest); flip to resolved when it reaches our solid-js. Our own copies of
the untrack advice (core, scaffold, console AGENTS.md) were corrected the
same day, with the ref callback added to the owned-scope list.

## Outcome

Resolved in 2.0.0-rc.5 (bumped 2026-09-02): CHEATSHEET.md lines 515 and 648
now state that untrack does not exempt owned-scope writes (the guard is
owner-based) and point to event handlers / onSettled / { ownedWrite: true }.
