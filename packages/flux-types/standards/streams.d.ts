// The WHATWG Streams subset: ReadableStream, WritableStream, TransformStream
// and the text streams. The names and shapes are the standard's; what is
// left out is what a single known app has not needed: no BYOB readers or
// byte streams (`type: "bytes"`), the only queuing strategy a chunk-count
// `highWaterMark`, no `pipeTo` options (`preventClose`, `preventAbort`,
// `preventCancel`, `signal`). Chunks are any JS value; a stream used as a
// body (`new Response(stream)`, a fetch body) must yield strings or
// Uint8Arrays.
//
// A ReadableStream is async-iterable, so `for await (let chunk of stream)`
// works, and ending the loop early cancels the stream. `Response.body`,
// `Request.body` and a child process's `stdout`/`stderr` are ReadableStreams
// over the runtime's own byte sources: one chunk is pulled per read, so a
// socket advances only as far as it is read, and cancelling the stream
// releases it.

/** The queuing strategy a stream takes: a chunk count, nothing else. */
interface QueuingStrategy {
  /**
   * How many chunks the stream queues ahead of its consumer (a readable:
   * before it stops pulling; a writable: before a writer sees
   * backpressure). Defaults to 1.
   */
  highWaterMark?: number
}

interface ReadableStreamDefaultController<R = any> {
  /** The chunks the queue has room for; null once the stream is errored. */
  readonly desiredSize: number | null
  /** Queue a chunk for the reader. Throws once the stream is closed. */
  enqueue(chunk: R): void
  /** End the stream after the queued chunks. */
  close(): void
  /** Error the stream: pending and later reads reject with `e`. */
  error(e?: any): void
}

/** What `new ReadableStream(source)` takes. Each method is optional. */
interface UnderlyingSource<R = any> {
  /** Runs at construction; a returned promise delays the first pull. */
  start?(controller: ReadableStreamDefaultController<R>): any
  /**
   * Called when the queue has room (and again once a returned promise
   * settles), to produce the next chunk(s).
   */
  pull?(controller: ReadableStreamDefaultController<R>): any
  /** Called when the consumer cancels the stream, with the reason. */
  cancel?(reason?: any): any
}

interface ReadableStreamReadResult<R> {
  value: R | undefined
  done: boolean
}

interface ReadableStreamDefaultReader<R = any> {
  /** Settles when the stream closes; rejects when it errors or the reader is released. */
  readonly closed: Promise<void>
  /** The next chunk, or `{ done: true }` at the end. */
  read(): Promise<ReadableStreamReadResult<R>>
  /** Cancel the stream, as {@link ReadableStream.cancel}. */
  cancel(reason?: any): Promise<void>
  /** Unlock the stream. Pending reads reject. */
  releaseLock(): void
}

interface ReadableStreamIteratorOptions {
  /** Keep the stream open when the loop ends early (`break`, a throw). */
  preventCancel?: boolean
}

/** A pair a stream pipes through; a TransformStream is one. */
interface ReadableWritablePair<R = any, W = any> {
  readable: ReadableStream<R>
  writable: WritableStream<W>
}

declare class ReadableStream<R = any> {
  /** No `type: "bytes"`; a strategy is a chunk count. */
  constructor(underlyingSource?: UnderlyingSource<R>, strategy?: QueuingStrategy)
  /** True while a reader, a pipe or an iteration holds the stream. */
  readonly locked: boolean
  /**
   * Lock the stream to a reader. One reader at a time: a second call
   * throws until the first is released. No `mode: "byob"`.
   */
  getReader(): ReadableStreamDefaultReader<R>
  /**
   * Discard the stream: the source's `cancel` runs with `reason`, pending
   * reads settle as done. Rejects while the stream is locked.
   */
  cancel(reason?: any): Promise<void>
  /**
   * Read the stream through `transform` and return its readable side. The
   * pipe runs on its own; an error on either side propagates to the other.
   * Throws when either stream is locked. No options.
   */
  pipeThrough<T>(transform: ReadableWritablePair<T, R>, options?: undefined): ReadableStream<T>
  /**
   * Read the stream into `destination` and close it at the end. An errored
   * source aborts the destination, an errored or closed destination cancels
   * the source, and the promise rejects with that error. No options.
   */
  pipeTo(destination: WritableStream<R>, options?: undefined): Promise<void>
  /**
   * Two streams over this one: every chunk goes to both, and the source is
   * cancelled once both are. A slow branch makes the other wait, since
   * chunks are not buffered per branch beyond its own queue.
   */
  tee(): [ReadableStream<R>, ReadableStream<R>]
  /** The async iterator `for await` uses; `preventCancel` keeps `break` from cancelling. */
  values(options?: ReadableStreamIteratorOptions): AsyncIterableIterator<R>
  [Symbol.asyncIterator](): AsyncIterableIterator<R>
  /**
   * A stream over an async (or sync) iterable: one item is pulled per
   * read, and cancelling the stream returns the iterator.
   */
  static from<R>(iterable: AsyncIterable<R> | Iterable<R | PromiseLike<R>>): ReadableStream<R>
}

interface WritableStreamDefaultController {
  /** Error the stream: queued and later writes reject with `e`. */
  error(e?: any): void
}

/** What `new WritableStream(sink)` takes. Each method is optional. */
interface UnderlyingSink<W = any> {
  /** Runs at construction; a returned promise delays the first write. */
  start?(controller: WritableStreamDefaultController): any
  /**
   * Receives one chunk at a time; the next write waits for a returned
   * promise, which is how a slow sink paces a fast writer.
   */
  write?(chunk: W, controller: WritableStreamDefaultController): any
  /** Runs once after the last write when the stream is closed. */
  close?(): any
  /** Runs when the stream is aborted, with the reason. */
  abort?(reason?: any): any
}

interface WritableStreamDefaultWriter<W = any> {
  /** Fulfilled while the queue has room; a new promise each time it fills. */
  readonly ready: Promise<void>
  /** Settles when the stream closes; rejects when it errors or the writer is released. */
  readonly closed: Promise<void>
  /** The chunks the queue has room for; null once the stream is errored. */
  readonly desiredSize: number | null
  /**
   * Queue a chunk. Resolves once the sink has accepted it, so awaiting each
   * write is one way to see backpressure; `ready` is the other.
   */
  write(chunk: W): Promise<void>
  /** Close the stream after the queued writes; resolves once the sink closed. */
  close(): Promise<void>
  /** Abort the stream, as {@link WritableStream.abort}. */
  abort(reason?: any): Promise<void>
  /** Unlock the stream. A pending `ready` or `closed` rejects. */
  releaseLock(): void
}

declare class WritableStream<W = any> {
  /** A strategy is a chunk count. */
  constructor(underlyingSink?: UnderlyingSink<W>, strategy?: QueuingStrategy)
  /** True while a writer or a pipe holds the stream. */
  readonly locked: boolean
  /**
   * Lock the stream to a writer. One writer at a time: a second call throws
   * until the first is released.
   */
  getWriter(): WritableStreamDefaultWriter<W>
  /**
   * Error the stream with `reason`: queued writes reject, the sink's
   * `abort` runs once a write in flight has finished. Rejects while the
   * stream is locked.
   */
  abort(reason?: any): Promise<void>
  /** Close the stream after the queued writes. Rejects while the stream is locked. */
  close(): Promise<void>
}

interface TransformStreamDefaultController<O = any> {
  /** The readable side's room for chunks; null once it is errored. */
  readonly desiredSize: number | null
  /** Queue a chunk on the readable side. */
  enqueue(chunk: O): void
  /** Error both sides with `e`. */
  error(e?: any): void
  /** Close the readable side and error the writable side. */
  terminate(): void
}

/** What `new TransformStream(transformer)` takes. Each method is optional. */
interface Transformer<I = any, O = any> {
  /** Runs at construction; a returned promise delays the first chunk. */
  start?(controller: TransformStreamDefaultController<O>): any
  /**
   * Receives each written chunk, to enqueue zero or more chunks on the
   * readable side. Absent, chunks pass through unchanged. A throw errors
   * both sides.
   */
  transform?(chunk: I, controller: TransformStreamDefaultController<O>): any
  /** Runs when the writable side closes, before the readable side does. */
  flush?(controller: TransformStreamDefaultController<O>): any
}

declare class TransformStream<I = any, O = any> {
  /**
   * Strategies are chunk counts: the writable side's defaults to 1, the
   * readable side's to 0, so a chunk is transformed as the reader asks. A
   * write waits while the readable side's queue is full.
   */
  constructor(transformer?: Transformer<I, O>, writableStrategy?: QueuingStrategy, readableStrategy?: QueuingStrategy)
  readonly readable: ReadableStream<O>
  readonly writable: WritableStream<I>
}

/**
 * A TransformStream from UTF-8 bytes (Uint8Array or ArrayBuffer chunks) to
 * strings: a {@link TextDecoder} in `stream` mode, so a multibyte sequence
 * split across chunks decodes whole. Chunks that decode to nothing are not
 * enqueued.
 */
declare class TextDecoderStream {
  /** As the {@link TextDecoder} constructor: UTF-8 labels only. */
  constructor(label?: string, options?: TextDecoderOptions)
  /** Always "utf-8". */
  readonly encoding: string
  readonly fatal: boolean
  readonly ignoreBOM: boolean
  readonly readable: ReadableStream<string>
  readonly writable: WritableStream<Uint8Array | ArrayBuffer>
}

/**
 * A TransformStream from strings to their UTF-8 bytes. A chunk ending in a
 * high surrogate holds that half for the next chunk, so a surrogate pair
 * split across chunks encodes as one character.
 */
declare class TextEncoderStream {
  /** Always "utf-8". */
  readonly encoding: string
  readonly readable: ReadableStream<Uint8Array>
  readonly writable: WritableStream<string>
}
