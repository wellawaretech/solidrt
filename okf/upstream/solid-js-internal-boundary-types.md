---
title: solid-js types re-export boundary primitives its d.ts strips
description: solid-js 2.0.0-rc.9 types/index.d.ts re-exports createErrorBoundary, createLoadingBoundary and createRevealOrder from client/hydration.js, but those are @internal and stripped from client/hydration.d.ts; with skipLibCheck they silently type as any, without it TS2305.
project: solid-js (github.com/solidjs/solid, packages/solid)
versions: solid-js 2.0.0-rc.9 through rc.11; fixed in rc.13 (rc.12 never published)
status: resolved
link: https://github.com/solidjs/solid/issues/3709
created: 2026-09-21
---

# solid-js types re-export boundary primitives its d.ts strips

Found 2026-09-21 on the rc.8 -> rc.9 bump: `packages/core/src/renderer.ts`
failed tsc with `Parameter 'error' implicitly has an 'any' type` on the
`createErrorBoundary` fallback, whose parameters had been contextually
typed at rc.8.

## Draft report

`packages/solid/src/index.ts` re-exports `createErrorBoundary`,
`createLoadingBoundary` and `createRevealOrder` from
`./client/hydration.js`. In `client/hydration.ts` all three are tagged
`@internal` ("kept exported for renderer, test, and compatibility use"),
and the d.ts build strips internal declarations, so the published
`types/client/hydration.d.ts` (rc.9) does not declare them while
`types/index.d.ts` still names them in its `export { ... } from
"./client/hydration.js"` list.

Effect for a consumer: with `skipLibCheck: true` the three imports resolve
to `any` without a diagnostic, so a fallback such as `(error, reset) =>`
loses its contextual types (TS7006 under `noImplicitAny`) and the
boundary's return type is `any`; without `skipLibCheck` the d.ts itself
fails with TS2305 ("has no exported member"). The runtime exports are
intact (`dist/solid.js` exports all three).

Suggested fix: either drop the three names from the `index.ts` re-export
list (they are internal, and `@solidjs/signals` exports the typed
primitives for renderers), or stop stripping them so the re-export has a
declaration to point at.

## Local impact and workaround

renderer.ts now imports `createErrorBoundary` from `@solidjs/signals`,
where it is public and typed, with a comment on why; five other core
files already import from signals directly. Nothing else in the repo
names the three primitives.

## Upstream

Filed 2026-09-29 as https://github.com/solidjs/solid/issues/3709, checked
against the rc.11 tarball and `next` at `0bf5758916`. The report suggests
the fix `index.ts` already uses for `materializeContainerTrace`: move the
three names into a `/** @internal */ export { ... } from
"./client/hydration.js"` block, which strips them from `index.d.ts` too
(`materializeContainerTrace` is absent from the published rc.11 d.ts, so
the pattern works). When it lands, renderer.ts can keep importing from
`@solidjs/signals`; only the comment on why needs updating.

## Outcome

Resolved in solid-js 2.0.0-rc.13 (bumped 2026-10-01; #3709 closed
2026-09-29): the published `types/index.d.ts` no longer names the three
boundary primitives, while `dist/solid.js` still exports them at runtime.
renderer.ts keeps importing `createErrorBoundary` from `@solidjs/signals`,
its typed public home; only the comment on why changed.
