---
title: Sockets, p2p streams and child stdin cannot be piped into
description: Bodies and child output are ReadableStreams since web-streams, but the byte duplexes left outside (flux:net Conn, flux:p2p P2pStream, a spawned child's stdin) still write through write()/closeWrite(), so nothing can pipeTo a socket or a child; give each the readable/writable pair, which for p2p first needs a write with backpressure in forge.
created: 2026-10-09
completed: 2026-10-09
---

# Sockets, p2p streams and child stdin cannot be piped into

Built 2026-10-09 as shaped below, with the open points settled this way:

- The read halves are forge `ByteStream`s, not plugin-side iterators: `Conn`
  and `Stream` keep their reader in a `stream::ReadSlot` and hand out a
  `read_half` view, so `byte_stream_readable` serves bodies, child pipes,
  sockets and p2p streams alike and flux has one readable builder. A view
  that is dropped (a `cancel()`) closes the slot, which for QUIC is
  STOP_SENDING: a peer writing to a reader that gave up gets an error
  instead of stalling on flow control. The p2p view holds a `Connection`
  clone, so a consumer still reading keeps the connection up.
- `close()` and `abort(reason)` share one `stream::Closing` per duplex: a
  token plus a reason slot both halves read. `close()` is graceful (reads
  end, writes fail); `abort` is Node's duplex-to-web mapping, a destroy with
  a reason: a pending or later read reports it, and a write in flight does
  too. TCP still sends FIN (it has no abortive half-close); QUIC resets the
  send half, so the peer's read fails rather than ending. A child's stdin is
  no duplex: its abort closes the pipe and leaves stdout alone.
- `byte_stream_iterable`'s `return()` ends a `next()` in flight, so a
  cancelled stream releases its hold and its source at once instead of when
  the peer next speaks (this improves fetch bodies too).
- The p2p writer task and its standing hold are gone; a `P2pStream` holds
  the engine only during a pending read or write, like a `Conn`.
- A QUIC stream reaches the acceptor with its first bytes, so the dialer
  writes first; documented on `accept` after the test fixture deadlocked
  on it.

Tests: `flux/tests/net.test.ts` (8), `flux/tests/subprocess.test.ts` (3),
`flux/tests/p2p.test.ts` (3), plus `forge/src/tests/net.rs` (10).

## Symptom

[web-streams](../done/web-streams.md) made `Request.body`, `Response.body`
and a child's `stdout`/`stderr` `ReadableStream`s. Three byte surfaces kept
their pre-streams shape:

- `flux:net` `Conn` and `flux:p2p` `P2pStream` are their own async
  iterators for the read half, with `write()`/`closeWrite()` for the
  write half.
- A spawned `Child` has `stdout`/`stderr` as ReadableStreams but stdin as
  `write()`/`closeWrite()`, two shapes on one object.

Reading already composes (`ReadableStream.from(conn).pipeThrough(...)`).
Writing does not: there is no `WritableStream` to `pipeTo`, so a TCP
proxy, a fetched body fed to a child, or one child piped into another is
a hand-written loop with manual backpressure and error handling.

No in-repo code outside the flux tests calls these methods (the dev
server's p2p tunnel goes through `flux:http`, not `P2pStream`), so there
is no migration.

## Shape

The pair Deno's `Conn` and `Child`, the WinterTC sockets API and Direct
Sockets agree on, and one shape only: the old methods go.

- `Conn`: `readable: ReadableStream<Uint8Array>`,
  `writable: WritableStream<string | Uint8Array>`, `remoteAddr`, `close()`.
  `writable.close()` is today's `closeWrite()` (FIN, reads continue);
  a write resolves once handed to the OS, as `write()` does now.
  `readable.cancel()` stops reading; `close()` stays the teardown of
  both halves. `[Symbol.asyncIterator]`, `write` and `closeWrite` go.
- `P2pStream`: the same, with `remoteId`.
- `Child`: `stdin: WritableStream<string | Uint8Array>` replaces `write`
  and `closeWrite`; `opts.stdin` keeps its meaning (written first). A
  detached child's stdin errors on the first write, as `write` does today.

Because our `pipeTo` closes the destination at the source's end and
aborts it on a source error, `a.readable.pipeTo(b.writable)` half-closes
`b` when `a` reaches EOF: the proxy semantics, with nothing extra.

### Pieces

1. `streams.rs`: `writable_to(ctx, sink)` beside `readable_from`, building
   `new WritableStream({ write, close, abort })` over native functions that
   capture only Rust state (the closure-capture trap in flux/CLAUDE.md).
   The read side wraps a native `{ next, return }` iterator with
   `readable_from`, as `body::byte_stream_readable` does; the per-read
   standing hold and the in-flight write hold stay as they are. Whether the
   forge read halves can be handed out as a `ByteStream`, so all three share
   `byte_stream_iterable`, is worth a look; `Conn::close()` cancelling a
   pending read through its token has to survive it.
2. forge p2p write path. `Stream::write` pushes onto an unbounded queue
   that `StreamWriter::run` drains, and a write error is only logged
   (`[flux] p2p write error`). A `WritableStream` over that resolves every
   write at once: a `pipeTo` from a fast source buffers without bound, and
   a failed write never reaches JS. Give it the `forge::net::Conn` shape
   instead: the send half behind an async lock, `write` awaiting
   `write_all` and returning its error, `finish` awaited, the writer task
   gone. This is the only forge change.
3. `net.rs`, `p2p.rs`, `subprocess.rs` build the pairs; flux-types
   `modules/net.d.ts`, `modules/p2p.d.ts`, `modules/subprocess.d.ts` are
   retyped, with the half-close and cancel semantics above in the doc
   comments.

## Done looks like

- The three surfaces have the shape above and nothing else.
- Tests: the `net.test.ts` echo over `readable`/`writable`; a TCP proxy
  through `pipeTo` that half-closes at EOF; a write to a peer that does not
  read stays pending once the socket buffers fill; a body piped into
  `cat`'s stdin and read back from its stdout; a p2p write after the peer
  went away rejects.

## Rejected

- Keeping `write`/`closeWrite` beside `writable`: two shapes for one thing.
- Adding only `readable`: reading is already one `ReadableStream.from`
  away; the missing piece is the write side.
- Making `Conn` itself a `ReadableStream` with write methods on it: a
  duplex is not a readable, and every runtime with web streams settled on
  the pair.

## Not in scope

- Streaming file reads and writes in `flux:fs` (a `stream()`, a writable
  for a download to disk): a feature, its own item when an app needs it.
- Kept as they are: isolate stream calls (values, not bytes, mirroring an
  async generator call with `return()` and abort), `Listener` and
  `Endpoint.accept` (they yield connections, not data), `Udp` (datagrams),
  `flux:tty` (line and key events of a terminal UI), `WebSocket` (event
  based; `WebSocketStream` is separate).
