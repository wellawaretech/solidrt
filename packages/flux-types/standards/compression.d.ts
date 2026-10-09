// The Compression Streams API: CompressionStream and DecompressionStream,
// TransformStreams over the deflate formats, with the standard's names and
// shapes. A chunk written in is an ArrayBuffer or a view of one; what comes
// out is Uint8Arrays of at most 64 KiB each, produced as the reader pulls,
// so a small input that inflates to a lot never runs ahead of its reader.
// The codec runs off the JS thread, a step per piece.
//
// Errors are the standard's TypeErrors: an unknown format throws from the
// constructor; a chunk that is not a buffer, corrupt input, input cut short
// when the writable side closes, and data after the end of the compressed
// stream (a second gzip member included) error both sides of the stream.

/**
 * The formats: `gzip` (RFC 1952), `deflate` (the zlib format, RFC 1950: a
 * two-byte header and an Adler-32 trailer) and `deflate-raw` (RFC 1951, the
 * bare stream, what a ZIP entry holds).
 */
type CompressionFormat = "gzip" | "deflate" | "deflate-raw"

declare class CompressionStream {
  /** Any other `format` throws a TypeError. */
  constructor(format: CompressionFormat)
  readonly readable: ReadableStream<Uint8Array>
  readonly writable: WritableStream<ArrayBuffer | ArrayBufferView>
}

declare class DecompressionStream {
  /** Any other `format` throws a TypeError. */
  constructor(format: CompressionFormat)
  readonly readable: ReadableStream<Uint8Array>
  readonly writable: WritableStream<ArrayBuffer | ArrayBufferView>
}
