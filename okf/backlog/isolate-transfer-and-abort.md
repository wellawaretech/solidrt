---
title: Isolate transfer() and AbortSignal
description: Design proposal for the two isolate follow-ups that need new call-surface vocabulary - zero-copy buffer hand-over and abortable calls. Decides once how a non-payload argument rides a plain function call, so the module gets one coherent rule instead of two accidents.
created: 2026-08-20
---

# Isolate transfer() and AbortSignal

Isolate calls are plain function calls: `handle.f(a, b)`. There is no
`postMessage(value, { transfer })` options slot, so anything that is about the
call rather than payload needs its own way in. Two follow-ups from
okf/done/isolate-follow-ups.md want exactly that; this note proposes the
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

   **Findings (2026-08-20, from rquickjs 0.12.1 + its bundled quickjs-ng):**

   - rquickjs has no steal surface: `ArrayBuffer` offers `new` (ownership-
     taking Vec), `new_copy`, `from_source*` (drop-closure shim), `detach()`,
     `as_raw()`. Nothing returns the backing. Same for the C API: only
     `JS_NewArrayBuffer(Copy)` / `JS_DetachArrayBuffer` / `JS_GetArrayBuffer`.
   - The quickjs free-hook contract is subtle: `JS_DetachArrayBuffer` calls
     `free_func(rt, opaque, data)` synchronously and does NOT clear it; the
     GC finalizer later calls `free_func(rt, opaque, NULL)` AGAIN. So a
     conforming `free_func` must tolerate a second call with NULL data, and
     `opaque` must stay alive until the finalizer. quickjs's own
     `js_array_buffer_free` (a plain `js_free`) is fine with that; both of
     rquickjs's hooks are not: `ArrayBuffer::new`'s rebuilds a Vec from the
     (now NULL) pointer, `from_source`'s does `Box::from_raw(opaque)`
     unconditionally. So calling rquickjs's own `detach()` on any rquickjs-
     created external buffer is UB at GC time (upstream soundness bug 1).
   - Worse, and independent of anything we build: quickjs-ng exposes
     `ArrayBuffer.prototype.transfer/transferToImmutable/
     transferToFixedLength`, and its same-length path neuters the old buffer
     WITHOUT calling `free_func`, then hands the same `free_func` with
     `opaque = NULL` to the new buffer; the length-change path `js_realloc`s
     the external pointer. Pure JS calling `.transfer()` on any buffer an
     isolate delivered TODAY (external, opaque = Vec capacity) is UB: the old
     finalizer runs `Vec::from_raw_parts(NULL, cap, cap)`, the new one frees
     with capacity 0 (a leak). `resize()` guards external buffers; `transfer`
     does not (upstream bug 2, reachable today without transfer() work).
   - Consequence for stage 1: it may only use rquickjs `detach()` on
     JS-allocated buffers (`js_array_buffer_free` hook). Detaching a
     link-arrived buffer (the round-trip case) hits bug 1, so the sound
     free hook below is a prerequisite for BOTH stages, not just stealing.

   **The sound design (no rquickjs changes needed):** create link-arrived
   buffers ourselves via `rquickjs_sys::JS_NewArrayBuffer` (already a
   dependency; one `extern "C"` hook, ~40 lines in the plugin marshal layer)
   with `opaque = Box<StealSlot>` holding the Vec parts plus a state cell
   (Live / Stolen / DetachFired). Hook: data non-NULL + Live -> rebuild and
   drop the Vec, free the slot (single-call GC path); data non-NULL + Stolen
   -> touch nothing, mark DetachFired, keep the slot (the finalizer's NULL
   call frees it); data NULL -> free the slot. The steal site (our transfer()
   send path) takes the Vec out, sets Stolen, then detaches - the hook fires
   synchronously in the Stolen state. This matches quickjs-ng's own
   double-call convention exactly. `typed_into_js` switches from
   `ArrayBuffer::new` to this constructor, making every received buffer both
   steal-ready and detach-safe. Mitigation for bug 2 until upstream fixes it:
   delete the three `ArrayBuffer.prototype.transfer*` methods at context
   setup (solidrt lens: our `transfer()` vocabulary replaces them). Both
   bugs were independently found 2026-08-03 during flux:wasm work
   (okf/upstream/ rquickjs-detach-double-free and
   quickjs-ng-transfer-external-buffers; flux:wasm's `array_buffer_over`
   already works around bug 1 with a raw `JS_NewArrayBuffer`, free hook
   NULL) - and BOTH are fixed on quickjs-ng master (PR #1578 single-shot
   detach, in v0.16.0; PR #1594 realloc-callback redesign making external
   buffers transfer/resize correctly, in v0.16.x). Our tree does not have
   the fixes: rquickjs vendors 0.15.1, sync PR DelSkayn/rquickjs#723 open
   since 2026-08-07. So the StealSlot hook is a bridge: needed only while
   we sit on rquickjs 0.12.x; after the rquickjs bump the steal shrinks to
   safe `from_source` over a slot-holding source (detach then fires the
   callback exactly once), and the new realloc-signature plumbing arrives
   with the bump anyway. Alternatives to bridging: wait for the rquickjs
   release, or `[patch]` rquickjs-sys to a fork carrying #723.
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
is in. Stage 2's unknown is CLEARED (2026-08-20, findings above): rquickjs
offers no steal surface and its own detach is unsound for external buffers,
so stage 1's round-trip detach and stage 2's steal share one prerequisite -
a sound free hook - and should land together.

Decision 2026-08-20: transfer stages 1+2 are PARKED, blocked on the
rquickjs quickjs-ng bump (DelSkayn/rquickjs#723). Both underlying bugs are
fixed upstream; bridging with our own unsafe hook on rquickjs 0.12.x would
be throwaway code in the most safety-critical spot, and no consumer is
waiting on zero-copy. After the bump the steal is safe `from_source` over a
slot-holding source. Shipped now instead (same day): context setup removes
`ArrayBuffer.prototype.transfer/transferToImmutable/transferToFixedLength`
(`remove_array_buffer_transfer` in flux/src/plugins/mod.rs, test in
flux/tests/web_api.rs) - closes the pure-JS UB path on our vendored engine
and reserves the name for flux:isolate's transfer(); it stays after the
bump on API-lens grounds. Known divergence: scaffold tsconfig lib ESNext
still types `.transfer()`, so TS will not flag a call that now throws at
runtime.
