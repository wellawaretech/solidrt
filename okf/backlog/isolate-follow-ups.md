---
title: Isolate follow-ups
description: The open ends left when isolates (okf/done/isolates-and-ports.md) closed, kept in one place so none vanishes with the done record; each is small and independent, none has a consumer yet. Zero-copy buffer transfer first when a payload size makes copying show up.
created: 2026-08-15
---

# Isolate follow-ups

The plan closed with calls, streams, concurrent dispatch and typed-array kinds
in place. What it explicitly left for a consumer to ask for, one line each,
symptom first where there is one:

- **Zero-copy buffer transfer.** Symptom: a large typed array (audio, mesh,
  frame) crossing per call shows up as copy time on both sides. Now that
  `Value::Bytes` carries its `Elem`, a transferred buffer keeps its view type,
  so this is contained: allocate the JS `ArrayBuffer` with a flux-owned free
  hook, hand ownership over the link, detach on the sending side. Opt-in per
  argument or result (the web `transfer` shape), copies stay the default.
- **Observable exit.** Symptom: an isolate that crashed (uncaught error out
  of a timer) is only discovered by the next call rejecting. A handle-level
  `exited: Promise<string | null>` (or `terminate()` resolving once the thread
  is gone) so it can be noticed and restarted.
- **`memoryLimit` option.** A runaway isolate takes the whole process down.
  QuickJS `JS_SetMemoryLimit` per child, one option in `isolate(id, opts)`.
- **`AbortSignal` on plain calls.** Can only mean "stop waiting" (a running
  sync export is uninterruptible short of `terminate()`); the child drops the
  reply.
- **`instances` on a handle.** N instances of one module behind one handle,
  calls spread over them; shape undecided (a pool is userland today: N
  `isolate()` calls).
- **Sync generators.** `function*` exports as streams, same protocol.

Separate item, done: okf/done/isolate-stack-attribution.md (isolate stacks
said `main:` and remapped against the app's sourcemap).

Done since this list was written: errors as data (2026-08-20): a throw
crosses as `Thrown` data on `Reply Err` - an error as
`CallError {name, message, stack, cause}`, rebuilt on the parent via
`new globals[name](message)` when that yields an error (`instanceof
RangeError` holds), else an `Error` carrying the name; the stack is a field
(no longer glued into the message); the `cause` chain crosses too, each cause
another rebuilt error or a sendable value (unsendable dropped, chain capped
at 8, which also ends a cyclic one); a thrown non-Error crosses as the value
itself when sendable.

Done: thread name per id (2026-08-20): child threads are named
`isolate:<id>` instead of a shared `flux-isolate`, so `top -H`, gdb and perf
tell isolates apart (Linux truncates thread names at 15 bytes, so a long id
keeps only its head).

Not wanted, decided in the plan: `SharedArrayBuffer`/`Atomics`, ports or
`postMessage`, source-text spawning, structured clone of identity-bearing
objects (Date/Map/Set), a serial-call option (a module that must not
interleave with itself serialises inside).
