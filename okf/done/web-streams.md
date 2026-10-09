---
title: Add ReadableStream, WritableStream and TransformStream as a documented subset
description: Flux has no web streams by decision; the first API whose standard shape is a transform (CompressionStream) shows that each stream-shaped standard would otherwise get a bespoke adapter, and the adapters together are a nonstandard streams API. Add the three classes as globals, implemented in embedded JS, with a documented subset (no BYOB, no queuing strategies beyond a chunk count), and type bodies and subprocess output as ReadableStream.
created: 2026-10-09
completed: 2026-10-09
---

# Add ReadableStream, WritableStream and TransformStream as a documented subset

Built 2026-10-09 as shaped below, with four decisions taken on the way:

- `TextDecoderStream` and `TextEncoderStream` live in `streams.js` beside
  the three classes (two small `TransformStream` wrappers over the
  `TextDecoder`/`TextEncoder` globals), not in `text.rs`; `streams.rs`
  installs all five globals.
- A native byte source stays a Rust-backed async iterator
  (`body::byte_stream_iterable`, which gained `return()` so a `break` or a
  cancel drops the socket), wrapped by the standard `ReadableStream.from`.
  `.body` is the same stream object on every read, as on the web; an
  `async function*` given as a body is wrapped the same way, so `.body` is
  one shape whatever was given.
- `text()`, `bytes()`, `arrayBuffer()` and `json()` on a stream-bodied
  Response drain the stream instead of throwing: `new Response(body
  .pipeThrough(...)).text()` is the idiom `compression-streams.md` builds on.
- A handler returning a fetched Response (`return fetch(upstream)`) is
  relayed natively by serve (`stream_incoming`), which is how the
  `buffered_bytes` gap closed.

Tests: `flux/tests/streams.test.ts` (22).

## Symptom

Flux deliberately has no `ReadableStream`, `WritableStream` or
`TransformStream`: byte streams are async-iterables built with the
`marshal.rs` helpers, and `Response.body`, `Request.body` and subprocess
`stdout`/`stderr` are typed `AsyncIterable<Uint8Array>` (the decision is
recorded in `okf/notes/flux-crate-review.md` and in the header of
`packages/flux-types/standards/fetch.d.ts`).

That decision was right while the only stream-shaped surface was a body you
read once: `for await` is a language feature, and the WHATWG Streams spec is
large (BYOB readers, locking, `tee`, queuing strategies, pipe options).

It stops being right at the first standard whose shape is a transform. The
standard `CompressionStream` is a `{ readable, writable }` pair used as
`body.pipeThrough(new DecompressionStream("gzip"))`; so are
`TextDecoderStream`/`TextEncoderStream`, `Blob.stream()`, streamed upload
bodies and `WebSocketStream`. Each of these, under "no `ReadableStream`",
needs its own adapter (`pipeThrough` on our own iterable, a `writable` with
only `getWriter()`), and the sum of those adapters is a streams implementation
that is nonstandard in name and shape. Under the solidrt lens (keep the
standard shape, simplify the semantics, document the contract) that is the
worse outcome.

The web has already moved toward the house pattern: `ReadableStream` is
async-iterable in the spec and in Chrome, Firefox, Node, Bun and Deno. A
`ReadableStream`-typed body keeps `for await (let c of res.body)` working, so
nothing of the current idiom is lost.

No in-repo code (core, cli, apps, examples) iterates a body or uses
`getReader`/`pipeThrough`, so there is no migration either way.

## Shape

### The contract (what flux-types documents)

- `ReadableStream`: `new ReadableStream({ start, pull, cancel })`,
  `getReader()` returning `{ read, cancel, releaseLock }`, `cancel()`,
  `locked`, `[Symbol.asyncIterator]`, `pipeThrough(transform)`,
  `pipeTo(writable)`, `tee()`. One reader at a time; a second `getReader()`
  throws, as on the web. No BYOB reader, no `type: "bytes"`, no queuing
  strategy beyond a chunk-count `highWaterMark` (default 1).
- `WritableStream`: `new WritableStream({ write, close, abort })`,
  `getWriter()` returning `{ write, close, abort, ready, closed }`, `abort()`,
  `locked`. `write()` resolves when the sink accepted the chunk, so a fast
  writer sees backpressure.
- `TransformStream`: `new TransformStream({ start, transform, flush })` with
  `readable` and `writable`. Cancel and error propagate through a pipe chain
  as the spec says. `pipeTo` options (`preventClose`, `preventAbort`,
  `preventCancel`) are not supported and throw.
- Chunks are any JS value. Bodies require string or `Uint8Array` chunks, as
  today.

### Pieces

1. `flux/src/standards_plugins/streams.js`, embedded with `include_str!` and
   evaluated once at context build (the `test_plugins/test.js` precedent).
   Streams are promise and queue plumbing: short in JS, miserable in
   rquickjs. `streams.rs` beside it installs the three globals, in the init
   order of `plugins/mod.rs` before `request`/`response`.
2. `body.rs`: `byte_stream_iterable` becomes the native underlying source.
   Its `next()` is the `pull`, its hold logic unchanged; `.body` wraps it in
   a `ReadableStream`. A `ReadableStream` passed as a body is async-iterable,
   so the existing `pump_async_iterable` pumps it: serve and a streamed fetch
   upload need no new path. The gap in `forge_plugins/serve.rs`
   (`buffered_bytes` serves an `Incoming` body as empty) closes along the
   way, since a streamed body now always arrives as a stream object.
3. Subprocess `stdout`/`stderr` wrap the same way, for one shape across the
   runtime.
4. `TextDecoderStream` and `TextEncoderStream` in `text.rs`: a
   `TransformStream` over the existing `TextDecoder({ stream: true })` and
   `TextEncoder`. Nearly free, and the usual second thing ported code needs
   after `pipeThrough`.
5. flux-types: `standards/streams.d.ts`, referenced from `index.d.ts`;
   `fetch.d.ts` retyped (`body: ReadableStream<Uint8Array>`) and its "no
   ReadableStream" header removed; `modules/subprocess.d.ts` retyped;
   `text.d.ts` gains the two transform classes. The runtime docs page lists
   the new globals. The "no ReadableStream, async-iterables are the house
   pattern" lines in `okf/notes/flux-crate-review.md` get a pointer here.

## Rejected

- Keeping the decision and giving `CompressionStream` a `readable` plus a
  `write`/`close` pair of its own: honest, but nonstandard, and the next
  stream-shaped standard repeats the exercise.
- A full WHATWG implementation (BYOB, byte streams, queuing strategies, pipe
  options): the parts no single known app needs, and the reference
  implementation is thousands of lines.
- Implementing the classes in Rust: each `read()`/`write()` is a promise
  resolved against a queue state machine; in rquickjs that is `Persistent`
  juggling and finalizer traps for no gain, since no I/O happens inside the
  classes themselves.

## Done looks like

- The three classes plus `TextDecoderStream`/`TextEncoderStream` are globals,
  declared in flux-types with the subset above as doc comments.
- `Response.body`, `Request.body` and subprocess output are `ReadableStream`s;
  `for await` over them works unchanged.
- `flux/tests/streams.test.ts` covers: reader lifecycle and locking;
  backpressure through `write()`; `tee`; cancel and error propagation through
  a pipe chain; a fetch body piped through a `TransformStream` into a served
  `Response`; `for await` on a body; `TextDecoderStream` across a split UTF-8
  sequence.

## Not in scope

- BYOB readers, byte streams, queuing strategies, `pipeTo` options.
- `Blob`, `FormData`, `WebSocketStream`: separate items when an app needs
  them; they become small once this lands.
- `CompressionStream`/`DecompressionStream`: `compression-streams.md`, which
  builds on this item.
