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
- **Errors as data.** Symptom: `catch (e) { if (e instanceof RangeError) }`
  across the boundary is always false; `e.stack` is glued into `e.message`.
  Send `{name, message, stack}` on `Reply Err`, rebuild via `globals[name]`
  when that is a constructor, else `Error`; `cause` only when `Sendable`.
- **Observable exit.** Symptom: an isolate that crashed (uncaught error out
  of a timer) is only discovered by the next call rejecting. A handle-level
  `exited: Promise<string | null>` (or `terminate()` resolving once the thread
  is gone) so it can be noticed and restarted.
- **`memoryLimit` option.** A runaway isolate takes the whole process down.
  QuickJS `JS_SetMemoryLimit` per child, one option in `isolate(id, opts)`.
- **Thread name per id.** All child threads are `flux-isolate` in `top -H`,
  gdb, perf; make it `flux-isolate:<id>`.
- **`AbortSignal` on plain calls.** Can only mean "stop waiting" (a running
  sync export is uninterruptible short of `terminate()`); the child drops the
  reply.
- **`instances` on a handle.** N instances of one module behind one handle,
  calls spread over them; shape undecided (a pool is userland today: N
  `isolate()` calls).
- **Sync generators.** `function*` exports as streams, same protocol.

Not wanted, decided in the plan: `SharedArrayBuffer`/`Atomics`, ports or
`postMessage`, source-text spawning, structured clone of identity-bearing
objects (Date/Map/Set), a serial-call option (a module that must not
interleave with itself serialises inside).
