---
type: backlog-item
title: Production diagnostics surface for bug reports
status: deferred
timestamp: 2026-07-17T00:00:00Z
---

# Production diagnostics surface for bug reports

The engine's layout-activity counters (measure calls, paragraph shapes,
dirtied nodes, cache gets/hits - alloy rendertree/counters.rs) are always on
by design, in production too: branchless integer bumps, and exactly the
numbers that diagnose layout-class field problems (they cracked the
postmortem's 184-row blowup in one session). They are latched into the
client's Stats every rebuild.

What is missing is a production consumer: today the only readers are the
go-client dev-server queries and the --stats overlay. Wanted: a way for a
production app to hand these numbers to a bug report. Candidate shapes,
undecided:

- a diagnostics dump appended to error/crash logs (stats snapshot at the
  moment things went wrong);
- a `flux:dev`-style debug command reachable in production builds;
- an app-facing API so the app itself can attach engine stats to its own
  feedback/report flow.

Whichever shape, it reads the existing latched Stats; no new collection.
The gating line is documented in counters.rs: bump-an-integer stays ungated,
anything heavier must be gated or must not ship.