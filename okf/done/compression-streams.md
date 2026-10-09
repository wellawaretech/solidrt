---
title: Add CompressionStream and DecompressionStream
description: Flux has no way to inflate or deflate bytes (a fetched .gz asset, a ZIP entry, a compressed save), and the web standard for it is the Compression Streams API; add the two classes as TransformStreams over an incremental flate2 codec in forge, one worker job per chunk so no thread is parked and no frame stalls, with the spec's three formats and its TypeError cases.
created: 2026-10-09
completed: 2026-10-09
---

# Add CompressionStream and DecompressionStream

Builds on `web-streams.md`: each class is a `TransformStream`.

Built 2026-10-09 as shaped below, with four decisions taken on the way:

- flate2's gzip mem API (`new_gzip`) exists on its zlib backends only, not
  on the pure-Rust backend in the tree, so `forge::compression` does the
  gzip framing itself: the 10-byte header (and the optional fields an
  incoming one may carry) and the CRC-32/size trailer, over flate2's `Crc`,
  around the mem API's raw deflate. The zlib format stays the mem API's.
- The two classes live in `streams.js` beside the text streams, over a
  native `DeflateCodec` object (`compression.rs`, the marshalling only) that
  `streams.rs` hands the source: pacing the output needs the transform's
  backpressure signal, which only the streams source has.
- The cap is a step loop, not one call per chunk: a codec step returns one
  piece of at most `OUTPUT_CHUNK_BYTES` (64 KiB) and says whether more is
  pending; the transform enqueues it, waits for the reader, and steps again.
  One worker job per piece, on a pool of `CODEC_THREADS` (2).
- The errors reject as `TypeError` through `JsTypeResult` (`js_error.rs`),
  the typed sibling of `JsResult`.

Tests: `flux/tests/compression.test.ts` (10), `forge/src/tests/compression.rs`
(9).

## Symptom

An app that fetches a gzipped asset, reads an entry out of a ZIP, or writes a
compressed save has nothing to call: flux ships no deflate, and a JS inflater
runs on the JS thread at QuickJS speed. The web standard for this is the
Compression Streams API: `CompressionStream` and `DecompressionStream`, each a
transform stream over the formats `"gzip"`, `"deflate"` (the zlib format) and
`"deflate-raw"`, supported in every browser, Node, Bun and Deno since 2023.
Browsers have no API for the ZIP container itself: a ZIP library (fflate,
zip.js) parses the directory and feeds each entry's bytes through
`DecompressionStream("deflate-raw")`, so these two classes are what makes
such a library work here.

## Shape

### What app code sees

```ts
let text = await new Response(res.body.pipeThrough(new DecompressionStream("gzip"))).text()

for await (let chunk of entry.pipeThrough(new DecompressionStream("deflate-raw"))) { ... }

let cs = new CompressionStream("gzip")
let writer = cs.writable.getWriter()
await writer.write(chunk)
await writer.close()
```

All of it runs unchanged in browsers, Node, Bun and Deno.

### Contract (what flux-types documents)

- Formats: `"gzip"`, `"deflate"`, `"deflate-raw"`. Any other format throws a
  `TypeError` from the constructor.
- Errors as the spec: corrupt input, input cut short at close, and data after
  the end of the compressed stream all error the readable with a `TypeError`.
  A second gzip member (concatenated `.gz` files) counts as trailing data.
- Output is produced as the reader pulls, in chunks capped by a named
  constant, so a tiny gzip that expands to gigabytes cannot run ahead of the
  reader.

### Pieces

1. `forge::compression` (engine-free): `Format`, and an incremental `Codec`
   with `push(input) -> output` and `finish() -> output`, over flate2's
   `Compress`/`Decompress` (`new` with and without the zlib header,
   `new_gzip`; all three formats on one API). flate2 `=1.1.9` is already in
   the tree via `image` -> `png` (miniz_oxide backend): no new crate, add the
   exact pin to forge's manifest. `Status::StreamEnd` plus `total_in` detect
   trailing data; `finish()` on an unfinished stream is the cut-short error.
   Unit tests in `forge/src/tests/`.
2. `flux/src/standards_plugins/compression.rs`: the two classes as
   `TransformStream`s whose `transform(chunk)` is one native per-chunk codec
   call and whose `flush()` is `finish()`. Each call is one job on a small
   `forge::workers::Workers` pool (the ktx2 pattern), the codec state moved
   into the job and back, so no worker thread is parked for a stream's
   lifetime and inflating a large asset never stalls a frame. The job is an
   `in_flight` hold; nothing standing, since the transform owns no resource
   between chunks.
3. flux-types: `standards/compression.d.ts`, referenced from `index.d.ts`;
   the runtime docs page lists the two globals.
4. `flux/tests/compression.test.ts`: a round trip per format; fixed bytes
   made by real gzip and zlib; a fetched body decompressed through
   `pipeThrough`; the writer path; the three error cases including a second
   gzip member; the output cap against highly compressible input.

## Rejected

- A long-lived worker job per stream: parks a worker thread blocked on the JS
  writer, so a few open streams exhaust a small pool.
- flate2's `write::GzEncoder`/`GzDecoder` wrappers: the `mem` API covers gzip
  since 1.1 and gives the stream-end signal the trailing-data error needs.
- A non-standard `flux:compression` with `gzip(bytes)`/`gunzip(bytes)`:
  smaller, but code ported from the web would not run.

## Not in scope

- Transparent `Content-Encoding: gzip` decoding in `fetch`: a separate item.
  reqwest is built without its gzip feature and flux sends no
  `Accept-Encoding`, so servers send plain bodies today.
- ZIP: a JS library over `deflate-raw`; a native `flux:zip` only if parsing
  the directory in JS turns out to be slow.
- brotli and zstd: not among the standard's formats; add one when an app
  needs it.
