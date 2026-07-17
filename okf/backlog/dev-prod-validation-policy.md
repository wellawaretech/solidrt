---
type: backlog-item
title: Runtime dev/prod signal for the validation policy (throw in dev, warn in prod)
status: deferred
timestamp: 2026-07-17T00:00:00Z
---

# Runtime dev/prod signal for the validation policy

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