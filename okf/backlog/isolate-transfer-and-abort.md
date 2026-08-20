---
title: Isolate transfer() and AbortSignal
description: Design proposal for the two isolate follow-ups that need new call-surface vocabulary - zero-copy buffer hand-over and abortable calls. Decides once how a non-payload argument rides a plain function call, so the module gets one coherent rule instead of two accidents.
created: 2026-08-20
---

# Isolate transfer() and AbortSignal

Isolate calls are plain function calls: `handle.f(a, b)`. There is no
`postMessage(value, { transfer })` options slot, so anything that is about the
call rather than payload needs its own way in. Two follow-ups from
okf/backlog/isolate-follow-ups.md want exactly that; this note proposes the
shared rule and the design of each.

## The shared rule: special arguments

A special argument is recognized by the call machinery, consumed, and not
sent. Two kinds exist:

- A value wrapped in `transfer(...)` (a marker from `flux:isolate`): sent,
  but by hand-over instead of copy.
- An `AbortSignal` among the arguments: not sent at all; it is the call's
  signal. (Today it would throw `TypeError: unsendable` anyway, so claiming
  it is compatible.)

Nothing else is special. Both are positional-agnostic: they may appear
anywhere in the argument list, and the child never sees the signal or the
marker (a transferred buffer arrives as the plain typed array it was).

## Zero-copy buffer transfer

### Where the copies actually are (measured from the code, 2026-08-20)

- Sending side: `value::from_js` copies the JS view into a `Vec<u8>`
  (`Value::Bytes`). One copy.
- The link: `Msg` over an mpsc channel moves the `Vec`. Zero copies.
- Receiving side: `typed_into_js` already hands the `Vec` to rquickjs's
  ownership-taking `ArrayBuffer::new` (not `new_copy`). Zero copies.

So the standing cost is one copy per crossing, on the sender, not two.
`transfer()` therefore has one copy to eliminate plus one semantic to add
(detach = hand-over, web-parity behavior).

### Proposal

`import { transfer } from "flux:isolate"`, usable on arguments and results:

    handle.process(transfer(frame))          // argument
    export function grab() { return transfer(buf) }   // result

`transfer(x)` accepts an `ArrayBuffer` or any typed-array view and marks the
underlying buffer; anything else throws a `TypeError`. On send the buffer is
detached (byteLength 0, like the web's transfer list); the receiver gets the
same element kind (`Value::Bytes` keeps its `Elem`). Copies stay the default:
an unmarked buffer behaves exactly as today.

Stages:

1. **Semantics first**: `transfer()` detaches on send; the send-side copy
   still happens for JS-allocated buffers (their bytes live in QuickJS's
   heap and cannot be stolen through today's rquickjs surface). Win: the
   contract and API land, receive side is already copy-free, and detach
   makes accidental reuse loud.
2. **Steal flux-owned allocations**: a buffer that arrived over the link is
   backed by a `Vec` flux allocated (rquickjs `ArrayBuffer::new` keeps it as
   opaque + free hook). Stealing it back on detach makes the round trip
   (parent -> child -> parent, e.g. a reused frame buffer) fully zero-copy.
   Needs a way to take the backing out of an ArrayBuffer without freeing:
   check rquickjs for it, else a small raw-qjs helper (or upstream PR).
3. (Only if a consumer shows the need) **flux-owned fresh allocations**: an
   allocation helper or allocator hook so first-crossing buffers are also
   stealable. Not designed here.

### Not proposed

A `transfer: [...]` options object on calls (no slot for it), transferring
anything but raw buffers, and `SharedArrayBuffer` (rejected in the plan).

## AbortSignal on plain calls

Fact first: flux has no `AbortSignal`/`AbortController` implementation. The
names appear only in flux-types `standards/fetch.d.ts`, so TypeScript
currently promises a global that does not exist (a parity gap worth fixing
regardless). This follow-up therefore has a prerequisite:

1. `AbortController`/`AbortSignal` as a standards plugin (web shape,
   simplified semantics per the solidrt lens: `abort()`, `signal.aborted`,
   `reason`, `"abort"` event / `onabort`; no `AbortSignal.timeout`/`any`
   until asked for).
2. The isolate rule: an `AbortSignal` among a call's arguments is consumed
   as the call's signal. Abort can only mean "stop waiting": the parent
   rejects the call's promise with `signal.reason` and forgets the call (the
   pending slot is removed; the child's eventual reply finds no slot and is
   dropped, which `deliver` already tolerates). A busy sync export in the
   child is untouched - interrupting it stays `terminate()`'s job. An
   already-aborted signal rejects without sending.

Scope: plain calls only. A stream is aborted by `break`/`return()` already;
wiring a signal to `return()` can come later if a consumer wants it. More
than one signal in an argument list throws.

## Order

AbortSignal's prerequisite (the standards plugin) is independent and useful
beyond isolates (fetch should honor it too - today it silently would not).
Transfer stage 1 and the AbortSignal rule are each small once this note's
vocabulary is agreed; transfer stage 2 carries the only real unknown
(stealing the backing from rquickjs).
