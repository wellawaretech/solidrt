---
title: Guarantee a microtask checkpoint between event dispatches
description: Events dispatched in one run-loop drain run without a microtask flush between them, so a handler reads stale signal values written by the previous event - lost edits, intermittent state corruption.
created: 2026-08-14
---

# Guarantee a microtask checkpoint between event dispatches

## Symptom

When several input events reach JS in one run-loop drain, their handlers run
back-to-back within a single task. Solid 2.0 defers signal read visibility to
the microtask flush, so the second handler reads the values from BEFORE the
first handler's writes. Any handler pair where the second reads a signal the
first wrote silently miscomputes, and only under burst timing - it works when
the same events arrive spread out, which makes it look like flaky app logic.

Concrete case (diagnosed 2026-08-14): TextInput corruption on Android.
SDL's IME bridge (SDLInputConnection.updateText) emits every IME operation as
a burst - N synthetic Backspace key events plus one TEXT_INPUT commit, queued
from one Java call stack. Each edit in createTextBuffer then computed from the
same stale pre-burst value: backspace-then-type yielded "hellop" from
"hello" (the backspace undone), a suggestion pick yielded "helo Hello "
instead of "Hello ". Reproduced end-to-end on an Android device by blocking
the JS thread while a burst arrived (probe: text-input-probe.tsx at repo
root). The TextInput-level fix (synchronous commit inside createTextBuffer)
is being handled separately; this item is the engine-wide class.

## Mechanism

The flux run loop (flux/src/engine.rs, the tokio::select! in run(), around
line 224) drains queued exec closures and drives the QuickJS job queue as
independent select branches. Branch choice is unbiased, so with several
closures queued the loop can execute closure after closure without ever
polling runtime.idle() in between - and Solid's flush is a queued job, so no
microtask checkpoint separates the dispatches. Whether a burst corrupts is
therefore a race: the same replayed sequence sometimes interleaves flushes
and comes out correct.

Desktop input never queues bursts (one event per loop wakeup), which is why
the class only surfaces on Android IME input today. Anything that queues
multiple exec closures while JS is busy can trigger it: input floods, events
arriving during a long frame callback, replayed recordings.

## What done looks like

An event handler can rely on every prior event's signal writes being visible:
each exec closure (or at least each event dispatch) is followed by a job-queue
drain before the next one runs. The blocked-thread burst replay (busy-wait
debug command + send_input burst) then produces correct text deterministically,
where today it intermittently corrupts.

## Rough shape

- Bias the run loop to drain the job queue between exec closures (biased
  select, or explicitly drive pending jobs after each closure) in
  flux/src/engine.rs.
- Cost: one job-queue poll per event at burst time; idle behavior unchanged.
- Risk to scope before starting: this changes engine-wide event dispatch
  semantics - anything (accidentally) depending on same-task dispatch of
  bursts, e.g. code observing several events before a flush, changes behavior.
  Also verify the pointer-batching path (moves are frame-batched, not events)
  is unaffected.
- Verification: probe app replay on a device plus the blocked-thread replay
  on desktop, both deterministic after the change.
