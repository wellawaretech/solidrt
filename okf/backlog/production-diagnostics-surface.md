---
title: Production diagnostics surface
description: Layout counters are latched into Stats but only dev-client queries read them; wanted a production consumer so field bug reports carry the numbers.
created: 2026-07-17
---

# Production diagnostics surface

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

## Second payload: the platform facts policy routes on (2026-08-14)

Counters say what the runtime did. They do not say what the runtime thinks
the device is, and that is the other half of a field report. [[frame-pacing-fluency]] burned a session on a policy that was
correct over an input that was false: SDL enumerates a touch device on the
Philips TV, so lattice picked `VsyncLocked` on a device that wanted
`SwapPaced`. Nothing in any dump would have shown that, because the facts
feeding the decision are not reported anywhere.

So the same surface should carry the facts with their provenance: touch and
keyboard presence and where each came from (SDL enumeration vs the Android
PackageManager feature query), refresh rate, display scale, the selected
frame pacing. These are read-once-and-cached values, so this is a dump of
existing state, not new collection - same gating line as above.