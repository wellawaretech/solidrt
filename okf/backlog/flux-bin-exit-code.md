---
title: flux binary exits 0 on uncaught errors
description: `flux script.js` exits 0 whether the entry module ran clean or threw, so nothing that drives it (a shell `&&`, a check rig, CI) can tell failure from success without parsing output. Surfaced 2026-08-17 by moving the @solidrt/3d check rigs from bun onto flux.
created: 2026-08-17
---

# flux binary exits 0 on uncaught errors

Symptom: run a script that throws at top level, or rejects its entry-module
promise, through the `flux` binary:

```
echo 'throw new Error("boom")' | target/release/flux -; echo $?
```

prints the error and `0`. A shell `a | flux - && next`, a Makefile rule, or
a CI step therefore treats a failed script as passed. The concrete victim
today is `packages/3d/checks/*-check.ts` (pick-check, order-check), which
run headless on flux and had to document "read the output, not the exit
code". It is also the prerequisite for any JS test runner
([js-test-infrastructure](js-test-infrastructure.md)).

Cause: `flux/src/bin/flux.rs` calls `engine.eval_source(&source).await` and
returns; `eval_source` reports a module error or an entry-promise rejection
through `report_error` and swallows it - correct for the engine (the runtime
is fail-soft, and lattice owns its own exit policy), wrong for a script
runner, where a nonzero exit on an uncaught error is what every caller
assumes.

Fix, bin-only, no engine API change: the builder already has
`on_uncaught(|msg| ..)`, called for exactly the cases that should fail the
run (module-level throw, unhandled rejection, throw out of a
fire-and-forget callback). Install it in `flux.rs` to set an
`AtomicBool`; after `eval_source` returns, `std::process::exit(1)` if set.
Same treatment for `fluxrt` if it runs scripts the same way. Then delete the
"exits 0 regardless" sentences from the two check rigs and
`packages/3d/AGENTS.md`, and their `throw` at the end can stay as the
signal.

Open question, decide when doing it: whether a throw from a fire-and-forget
callback (a timer, an event handler) after the entry finished should also
fail the run. The hook fires for those too; the simplest reading is yes -
any uncaught error means the script did not run clean - and it is what
node/bun do.
