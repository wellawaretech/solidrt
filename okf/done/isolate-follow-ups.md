---
title: Isolate follow-ups
description: The open ends left when isolates (okf/done/isolates-and-ports.md) closed, kept in one place so none vanished with the done record. All of them are now built, decided against, or moved to a design note (okf/backlog/isolate-transfer-and-abort.md); this is the record of how each ended.
created: 2026-08-15
completed: 2026-08-21
---

# Isolate follow-ups

The plan closed with calls, streams, concurrent dispatch and typed-array kinds
in place, and left a handful of ends for a consumer to ask for. They were
worked off over the following days; what each one became is below.

Still only a wish, no consumer: `instances` on a handle - N instances of one
module behind one handle with calls spread over them, shape undecided (a pool
is userland today: N `isolate()` calls). Kept as a line in okf/ideas.md.

Designed, not yet built: zero-copy buffer transfer needs new call-surface
vocabulary (a special argument on a plain function call), decided in
okf/backlog/isolate-transfer-and-abort.md - which also records the
`AbortSignal` rule, built 2026-08-20: a signal among a call's arguments is
consumed as the call's signal; abort rejects the call with `signal.reason`
and drops the eventual reply, without touching the export.

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

Done: observable exit (2026-08-20): `exited` is the third reserved name on a
handle - a `Promise<string | null>` settling once the child is gone, with the
uncaught error that ended it or `null` after a clean end or `terminate()`.
Reading it is a first use (spawns the child like a call does) and starts the
exit pump, a link reader that holds the parent's loop open until the child
exits, so an exit is noticed with no call in flight (before, nothing read the
link between calls). A never-spawned handle that is terminated resolves
`null`.

Done: thread name per id (2026-08-20): child threads are named
`isolate:<id>` instead of a shared `flux-isolate`, so `top -H`, gdb and perf
tell isolates apart (Linux truncates thread names at 15 bytes, so a long id
keeps only its head).

Done: `memoryLimit` option (2026-08-20): `isolate(id, { memoryLimit })`, a
heap limit in bytes for the child runtime (QuickJS memory limit via a
per-engine, non-inherited builder option - a child's limit does not cascade
to isolates it spawns). Past the limit, allocations in the child fail with
an out-of-memory error where they happen (verified: the failing call
rejects, the child survives, the parent is untouched); an exit this causes
is observable via `exited`. Invalid values throw a `TypeError`.

Done as an error instead (2026-08-20): sync generators. Support was dropped
deliberately - an `async function*` with a sync body already offloads
identically, so `function*` streaming would add a second protocol for
nothing. Instead the child recognizes a returned generator object (via its
`Generator` toStringTag, so arrays and other sendable iterables stay plain
values) and rejects with "export 'x' returned a sync generator: make it an
async generator to stream from an isolate"; before, it fell through to the
unsendable/empty-object result path. flux-types maps `Generator`-returning
exports to `never`.

Not wanted, decided in the plan: `SharedArrayBuffer`/`Atomics`, ports or
`postMessage`, source-text spawning, structured clone of identity-bearing
objects (Date/Map/Set), a serial-call option (a module that must not
interleave with itself serialises inside).
