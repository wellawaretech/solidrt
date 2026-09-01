---
title: Solid 2 cheatsheet does not say the For child body is untracked
description: CHEATSHEET.md documents the accessor/plain-value flip between keyed and non-keyed For, but not that the child function's body is an untracked scope, so the default "destructure at the top, use below" style silently freezes; the trap should sit next to For keyed={false}.
project: solid-js CHEATSHEET.md (github.com/solidjs/solid)
versions: solid-js 2.0.0-rc.3 (also rc.4, checked 2026-08-30)
status: resolved
link: https://github.com/solidjs/solid/issues/3126
created: 2026-08-30
---

# Solid 2 cheatsheet does not say the For child body is untracked

Found 2026-08-30 in external feedback on the render-lab demo (the 54
project): 24 `STRICT_READ_UNTRACKED` warnings from a non-keyed `<For>` whose
child read the item accessor at the top of its body. The warning text itself
is good ("Move it into a tracking scope (JSX, a memo, or an effect's compute
function)"); the gap is that the cheatsheet never says the body is one of the
untracked places.

Draft report:

```tsx
<For each={items} keyed={false}>
  {(item, i) => {
    let a = item().x        // untracked: child fn body is not a tracking scope
    return <Row a={a} />    // frozen at initial value
  }}
</For>

<For each={items} keyed={false}>
  {(item, i) => <Row a={item().x} />}   // tracked
</For>
```

The cheatsheet documents the accessor/plain-value flip between keyed and
non-keyed `<For>` (the "Control-flow components" section and the callback
shape line under it), but not that the child function's body is an untracked
scope. "Destructure at the top, use below" is the default JS style and it
silently breaks. Suggest putting the trap next to `<For keyed={false}>` in the
control-flow section, not only in the generic reactivity rules. The same
applies to the function children of `<Show>` / `<Match>`.

It is not specific to `keyed={false}`: mapArray runs the child under
`runWithOwner(createOwner(), ...)` in every keying mode (an owner, not a
computed), so whichever argument is an accessor freezes when read at the top
of the body - `i()` with `keyed` omitted, both `item()` and `i()` with a
custom key function. The non-keyed case is just where the item itself is the
accessor, so it is the one people hit.

Checked upstream 2026-08-30: not known. No issue, PR or discussion on
solidjs/solid mentions it (searched For/keyed/untracked/tracking scope/
STRICT_READ_UNTRACKED/cheatsheet), and `packages/solid/CHEATSHEET.md` on the
`next` branch (last touched 2026-08-12, "sweep beta-era wording to RC") is
byte-identical to the rc.3 copy we ship - line 645 still states only the
callback shape. The file exists on `next` only, so a PR targets that branch.

On our side the cheatsheet ships verbatim from the package, so it cannot be
patched locally; the scaffold AGENTS.md trap 1 (top-level reads untracked)
is where the platform-side wording lives.

Outcome 2026-08-31: fixed on `next` in b2f0988b (cheatsheet now states the
callback body is an owner, not a tracking scope, in three places including
next to the non-keyed `<For>` example). Not in any release yet (rc.4 is the
newest); flip to resolved when the fix reaches our solid-js.

## Outcome

Resolved in solid-js 2.0.0-rc.5 (bumped 2026-09-02): CHEATSHEET.md now says
the callback body is an owner, not a tracking scope, and that a top-of-body
accessor read freezes at the initial value, right under the For keying rules.
