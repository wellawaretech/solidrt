---
type: backlog-item
title: Dev/prod signal for validation
description: The missing runtime signal and shared helper behind the agreed convention of throwing in dev and warning in prod; today everything is dev, so validation sites throw.
status: deferred
timestamp: 2026-07-17T00:00:00Z
---

# Dev/prod signal for validation

Decided convention (2026-07-17): API input validation in solidrt follows
**throw in dev, ignore-with-warning in prod**. A developer should hit a hard
error the moment they pass a wrong option; a shipped app should not crash on
the same mistake, it should log and keep going.

What is missing is the runtime signal: nothing in flux/lattice knows whether
it is running in dev or prod. Today everything is dev (no production
packaging exists), so validation sites simply throw - which is the correct
dev half of the policy. When the production story lands (update mechanism,
pinned apps), this item is about:

- introducing the runtime dev/prod bit (probably alongside the app
  manifest / data-root resolution - a pinned production app is prod, a
  dev-server-connected or plain-script run is dev);
- a shared helper the validation sites call (throw vs warn on the signal),
  so the policy is one implementation, not a per-site convention;
- sweeping existing throw sites onto the helper.

First known site: the fetch `cache` option (unknown string values), see
`../plans/fetch-cache.md`.

Update 2026-07-25: core's `process.env.NODE_ENV` read (renderer leak
sentinel) migrated to `import.meta.env.DEV`, a bundle-time constant the
srt bundler defines - deliberately NOT this runtime signal, because that
site wants dead-code elimination and a runtime value can never fold. The
two stay separate concerns: `import.meta.env.DEV` = bundle flavor
(compile-time, foldable), this item = deployment context (runtime,
behavioral). A candidate shape noted for when this lands: flux can derive
the bit natively from boot mode (source eval = dev, compiled bytecode =
prod), builder override for embedders; exposure via `flux:process` `env`.
See okf/plans/examples-rescope.md ("Small fix folded in").