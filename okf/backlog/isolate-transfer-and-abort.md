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

Prerequisite done (2026-08-20): `AbortController`/`AbortSignal` as a
standards plugin (web shape, simplified semantics per the solidrt lens:
`abort(reason?)`, `signal.aborted`/`reason`/`onabort`/`throwIfAborted()`,
`AbortSignal.abort(reason?)`; handler property only, like the WebSocket
client; no `AbortSignal.timeout`/`any` until asked for; default reason is an
`Error` named "AbortError" - there is no `DOMException`). Fetch honors
`RequestInit.signal` (same day): abort rejects the fetch promise with the
signal's `reason` and drops the request mid-flight; an already-aborted
signal rejects without sending. Native consumers race work against
`AbortSignal::subscribe()` (a oneshot fired on abort) - the isolate rule
below uses the same hook.

The isolate rule, done (2026-08-20, verified): an `AbortSignal` among a
call's arguments is consumed as the call's signal (anywhere in the list; the
export sees only the other arguments; more than one throws a `TypeError`).
On a plain call, abort means "stop waiting": the parent rejects the call's
promise with `signal.reason` and forgets the call (the pending slot is
removed; the child's eventual reply finds no slot and is dropped, which
`deliver` already tolerates). A busy export in the child is untouched -
interrupting it stays `terminate()`'s job. An already-aborted signal rejects
without sending anything, and without spawning the child. On a stream, abort
acts as `return()`: the generator ends in the child (`finally` runs) and the
`for await` loop finishes cleanly, like a `break` from outside it - aborting
a subscription is not an error. Mechanically, once the child answers
"stream" the racing task hands the signal to a native `on_abort` closure
that just sends `Return`, so no task stays parked on the signal and an
unread stream still lets the runtime go idle. Tests:
flux/tests/isolate.rs.

## Order

Done except transfer: the standards plugin, fetch and the isolate rule all
landed 2026-08-20. Transfer stage 1 is small now that the vocabulary above
is in; transfer stage 2 carries the only real unknown (stealing the backing
from rquickjs).
