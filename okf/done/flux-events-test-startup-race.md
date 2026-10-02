---
title: The flux event tests wait for the listener, not a sleep
description: The three tests in flux/tests/events.rs emit after a fixed 100 ms sleep, but an exec closure queued before the engine is up runs before the entry module, so on a slow startup the first event reaches no listener and the test hangs for good. Wait until the listener is registered instead, and fail on a lost event rather than hang.
created: 2026-10-01
completed: 2026-10-02
---

# The flux event tests wait for the listener, not a sleep

Symptom: `emit_triggers_listener`, `event_delivery_with_set_interval` and
`microtask_registered_listener_with_set_interval` (`flux/tests/events.rs`)
can hang without end. All three did at once in a release run's
`gate / check` job: "has been running for over 60 seconds", then nothing
until the job's 60-minute timeout cancelled it and blocked the publish. The
same commit had passed the same step in under 3 minutes an hour earlier: a
race, not a regression.

## Why

`run_with_events` starts the engine on a thread, sleeps 100 ms, then emits
through `ExecHandle::exec`. The sleep stands in for "the script has
registered its listener", and nothing guarantees it:

- `FluxEngine::run` (`flux/src/engine.rs`) runs every closure queued
  between build and eval *before* the entry module, on purpose: those
  closures carry the launch facts a module-scope read must see. An emit
  queued before the engine is up is one of them, so it dispatches to no
  listener and is lost.
- The engine reaches that point only after `plugins::init_context`. A
  debug build starting three engines in parallel on a slow runner can take
  longer than the 100 ms (150 ms for the two interval tests, whose first
  emit follows 50 ms later).
- With an event lost, the test never gets its last one: `unsub()` and
  `clearInterval` never run, the listener's standing hold and the interval
  keep the engine alive, and `engine_thread.join()` waits forever.

## What was done

- The tests emit only once the listener exists. The test's `on` global
  signals a `std::sync::mpsc` channel on each call, and `run_with_events`
  waits on that instead of the sleep. The signal comes from `on` itself, so
  the microtask test is covered too. The wait has a timeout of its own, so a
  broken `on` fails the test rather than moving the hang.
- A lost event fails the test instead of hanging it: the engine thread
  reports its end on a channel and the test waits with `recv_timeout`, so a
  regression costs seconds, not a CI job's timeout.
- The 50 ms gaps between emits stay: they exercise delivery while an
  interval runs, not readiness.

One file, `flux/tests/events.rs`. No engine change: the pre-eval drain is
the contract, and the tests leaned on timing it never promised. No other
flux test emits into an engine from outside through `ExecHandle`.
