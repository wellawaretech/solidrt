// The WHATWG Streams subset: ReadableStream, WritableStream and
// TransformStream, plus TextDecoderStream and TextEncoderStream over the
// TextDecoder/TextEncoder globals, and CompressionStream and
// DecompressionStream over the native DeflateCodec (compression.rs) this
// source takes. Evaluated once per context by streams.rs, which installs
// what this source returns as globals. Plain JS on purpose: streams are
// promise and queue plumbing, which is short here and miserable in rquickjs
// (okf/done/web-streams.md).
//
// The subset, as packages/flux-types/standards/streams.d.ts documents it: no
// BYOB readers or byte streams, a chunk-count highWaterMark as the only
// queuing strategy, no pipeTo options. One reader or writer at a time, as
// on the web.
(DeflateCodec) => {
  // Chunks a stream queues ahead of its consumer before it stops pulling,
  // or ahead of its sink before a writer sees backpressure (the spec's
  // default count queuing strategy).
  const DEFAULT_HIGH_WATER_MARK = 1
  // A TransformStream's readable side queues nothing of its own: a chunk
  // is pulled from the writable side as the reader asks (the spec's default
  // for that side).
  const TRANSFORM_READABLE_HIGH_WATER_MARK = 0
  // ReadableStream.from pulls one item at a time, as the reader asks, so an
  // iterator over a socket only advances as far as it is read (the spec's
  // highWaterMark for `from`).
  const FROM_HIGH_WATER_MARK = 0
  // The UTF-16 range of a high surrogate: a string chunk ending in one is
  // half a character, held for the next chunk by TextEncoderStream.
  const HIGH_SURROGATE_FIRST = 0xd800
  const HIGH_SURROGATE_LAST = 0xdbff

  // The internal state of every stream, reader, writer and controller: the
  // public objects carry no properties of their own, as on the web.
  let internals = new WeakMap()

  function noop() {}

  function deferred() {
    let d = {}
    d.promise = new Promise((resolve, reject) => {
      d.resolve = resolve
      d.reject = reject
    })
    return d
  }

  // A promise whose rejection is somebody else's to observe (a stream's
  // `closed`, a pipe started by pipeThrough): never reported as unhandled.
  function handled(promise) {
    promise.catch(noop)
    return promise
  }

  // Call an underlying-source or sink method as a promise: a sync throw is a
  // rejection, a plain value a fulfillment, as the spec's "call ... and
  // return a promise" does. A missing method is a fulfilled promise.
  function invoke(fn, thisArg, args) {
    if (fn === undefined) return Promise.resolve()
    try {
      return Promise.resolve(fn.apply(thisArg, args))
    } catch (e) {
      return Promise.reject(e)
    }
  }

  function methodOf(obj, name, api) {
    let fn = obj[name]
    if (fn === undefined || fn === null) return undefined
    if (typeof fn !== "function") throw new TypeError(`${api}: ${name} is not a function`)
    return fn
  }

  function highWaterMarkOf(strategy, fallback, api) {
    if (strategy === undefined || strategy === null) return fallback
    if (strategy.size !== undefined) throw new RangeError(`${api}: a queuing strategy with a size function is not supported`)
    let hwm = strategy.highWaterMark
    if (hwm === undefined) return fallback
    if (typeof hwm !== "number" || Number.isNaN(hwm) || hwm < 0) {
      throw new RangeError(`${api}: highWaterMark must be a non-negative number`)
    }
    return hwm
  }

  function isObject(value) {
    return (typeof value === "object" && value !== null) || typeof value === "function"
  }

  // -- ReadableStream --

  // rs: { state, storedError, queue, reader, readRequests, closeRequested,
  //       started, pulling, pullAgain, highWaterMark, pull, cancel }
  // `reader` is the active reader's internals, or null when unlocked.

  function rsDesiredSize(rs) {
    if (rs.state === "errored") return null
    if (rs.state === "closed") return 0
    return rs.highWaterMark - rs.queue.length
  }

  function rsShouldPull(rs) {
    if (rs.state !== "readable" || rs.closeRequested || !rs.started) return false
    if (rs.reader !== null && rs.reader.readRequests.length > 0) return true
    return rsDesiredSize(rs) > 0
  }

  function rsCallPull(rs) {
    if (!rsShouldPull(rs)) return
    if (rs.pulling) {
      rs.pullAgain = true
      return
    }
    rs.pulling = true
    rs.pull().then(
      () => {
        rs.pulling = false
        if (rs.pullAgain) {
          rs.pullAgain = false
          rsCallPull(rs)
        }
      },
      (e) => rsError(rs, e),
    )
  }

  function rsFinishClose(rs) {
    rs.state = "closed"
    let reader = rs.reader
    if (reader === null) return
    for (let request of reader.readRequests.splice(0)) request.resolve({ value: undefined, done: true })
    reader.closed.resolve()
  }

  function rsEnqueue(rs, chunk) {
    if (rs.closeRequested || rs.state !== "readable") throw new TypeError("ReadableStream: the stream is not readable")
    let reader = rs.reader
    if (reader !== null && reader.readRequests.length > 0) {
      reader.readRequests.shift().resolve({ value: chunk, done: false })
    } else {
      rs.queue.push(chunk)
    }
    rsCallPull(rs)
  }

  function rsClose(rs) {
    if (rs.closeRequested || rs.state !== "readable") throw new TypeError("ReadableStream: the stream is not readable")
    rs.closeRequested = true
    if (rs.queue.length === 0) rsFinishClose(rs)
  }

  function rsError(rs, e) {
    if (rs.state !== "readable") return
    rs.state = "errored"
    rs.storedError = e
    rs.queue = []
    let reader = rs.reader
    if (reader === null) return
    for (let request of reader.readRequests.splice(0)) request.reject(e)
    reader.closed.reject(e)
  }

  function rsRead(rs) {
    if (rs.state === "closed") return Promise.resolve({ value: undefined, done: true })
    if (rs.state === "errored") return Promise.reject(rs.storedError)
    if (rs.queue.length > 0) {
      let chunk = rs.queue.shift()
      if (rs.closeRequested && rs.queue.length === 0) rsFinishClose(rs)
      else rsCallPull(rs)
      return Promise.resolve({ value: chunk, done: false })
    }
    let request = deferred()
    rs.reader.readRequests.push(request)
    rsCallPull(rs)
    return request.promise
  }

  function rsCancel(rs, reason) {
    if (rs.state === "closed") return Promise.resolve()
    if (rs.state === "errored") return Promise.reject(rs.storedError)
    rs.queue = []
    rsFinishClose(rs)
    return rs.cancel(reason).then(noop)
  }

  class ReadableStreamDefaultController {
    constructor(rs) {
      internals.set(this, rs)
    }

    get desiredSize() {
      return rsDesiredSize(internals.get(this))
    }

    enqueue(chunk) {
      rsEnqueue(internals.get(this), chunk)
    }

    close() {
      rsClose(internals.get(this))
    }

    error(e) {
      rsError(internals.get(this), e)
    }
  }

  class ReadableStreamDefaultReader {
    constructor(stream) {
      let rs = internals.get(stream)
      if (rs === undefined || !(stream instanceof ReadableStream)) {
        throw new TypeError("ReadableStreamDefaultReader: not a ReadableStream")
      }
      if (rs.reader !== null) throw new TypeError("ReadableStream: the stream is locked")
      let reader = { rs, readRequests: [], closed: deferred() }
      handled(reader.closed.promise)
      if (rs.state === "closed") reader.closed.resolve()
      else if (rs.state === "errored") reader.closed.reject(rs.storedError)
      rs.reader = reader
      internals.set(this, reader)
    }

    get closed() {
      return internals.get(this).closed.promise
    }

    read() {
      let reader = internals.get(this)
      if (reader.rs === null) return Promise.reject(new TypeError("ReadableStreamDefaultReader: the reader was released"))
      return rsRead(reader.rs)
    }

    cancel(reason) {
      let reader = internals.get(this)
      if (reader.rs === null) return Promise.reject(new TypeError("ReadableStreamDefaultReader: the reader was released"))
      return rsCancel(reader.rs, reason)
    }

    releaseLock() {
      let reader = internals.get(this)
      let rs = reader.rs
      if (rs === null) return
      let released = new TypeError("ReadableStreamDefaultReader: the reader was released")
      for (let request of reader.readRequests.splice(0)) request.reject(released)
      reader.closed.reject(released)
      rs.reader = null
      reader.rs = null
    }
  }

  class ReadableStream {
    constructor(underlyingSource = {}, strategy = {}) {
      if (underlyingSource === null) underlyingSource = {}
      if (!isObject(underlyingSource)) throw new TypeError("ReadableStream: the underlying source must be an object")
      if (underlyingSource.type !== undefined) throw new RangeError("ReadableStream: byte streams (type: \"bytes\") are not supported")
      let start = methodOf(underlyingSource, "start", "ReadableStream")
      let pull = methodOf(underlyingSource, "pull", "ReadableStream")
      let cancel = methodOf(underlyingSource, "cancel", "ReadableStream")
      let rs = {
        state: "readable",
        storedError: undefined,
        queue: [],
        reader: null,
        closeRequested: false,
        started: false,
        pulling: false,
        pullAgain: false,
        highWaterMark: highWaterMarkOf(strategy, DEFAULT_HIGH_WATER_MARK, "ReadableStream"),
        pull: null,
        cancel: null,
      }
      let controller = new ReadableStreamDefaultController(rs)
      rs.pull = () => invoke(pull, underlyingSource, [controller])
      rs.cancel = (reason) => invoke(cancel, underlyingSource, [reason])
      internals.set(this, rs)
      invoke(start, underlyingSource, [controller]).then(
        () => {
          rs.started = true
          rsCallPull(rs)
        },
        (e) => rsError(rs, e),
      )
    }

    get locked() {
      return internals.get(this).reader !== null
    }

    getReader(options) {
      if (options !== undefined && options !== null && options.mode !== undefined) {
        throw new TypeError("ReadableStream: BYOB readers (mode: \"byob\") are not supported")
      }
      return new ReadableStreamDefaultReader(this)
    }

    cancel(reason) {
      let rs = internals.get(this)
      if (rs.reader !== null) return Promise.reject(new TypeError("ReadableStream: the stream is locked"))
      return rsCancel(rs, reason)
    }

    // The async iterator: a reader for the iteration's lifetime, released
    // at the end; `break` (the iterator's `return`) cancels the stream
    // unless `preventCancel` says not to.
    values(options) {
      let reader = this.getReader()
      let preventCancel = options !== undefined && options !== null && Boolean(options.preventCancel)
      let finished = false
      let iterator = {
        next() {
          if (finished) return Promise.resolve({ value: undefined, done: true })
          return reader.read().then(
            (result) => {
              if (result.done) {
                finished = true
                reader.releaseLock()
              }
              return result
            },
            (e) => {
              finished = true
              reader.releaseLock()
              throw e
            },
          )
        },
        return(value) {
          if (finished) return Promise.resolve({ value, done: true })
          finished = true
          let cancelled = preventCancel ? Promise.resolve() : reader.cancel()
          reader.releaseLock()
          return cancelled.then(() => ({ value, done: true }))
        },
        [Symbol.asyncIterator]() {
          return this
        },
      }
      return iterator
    }

    [Symbol.asyncIterator]() {
      return this.values()
    }

    pipeTo(destination, options) {
      if (!(destination instanceof WritableStream)) throw new TypeError("pipeTo: the destination must be a WritableStream")
      checkPipeOptions(options, "pipeTo")
      if (this.locked) return Promise.reject(new TypeError("pipeTo: the stream is locked"))
      if (destination.locked) return Promise.reject(new TypeError("pipeTo: the destination is locked"))
      return pipe(this, destination)
    }

    pipeThrough(transform, options) {
      if (!isObject(transform) || !(transform.readable instanceof ReadableStream) || !(transform.writable instanceof WritableStream)) {
        throw new TypeError("pipeThrough: the transform must be a { readable, writable } pair")
      }
      checkPipeOptions(options, "pipeThrough")
      if (this.locked) throw new TypeError("pipeThrough: the stream is locked")
      if (transform.writable.locked) throw new TypeError("pipeThrough: the transform's writable side is locked")
      handled(pipe(this, transform.writable))
      return transform.readable
    }

    // Two streams over this one: every chunk goes to both branches, the
    // source is cancelled once both branches are.
    tee() {
      let reader = this.getReader()
      let reading = false
      let cancelled = [false, false]
      let reasons = [undefined, undefined]
      let controllers = []
      let sourceCancelled = deferred()
      let pull = () => {
        if (reading) return Promise.resolve()
        reading = true
        reader.read().then(
          (result) => {
            reading = false
            for (let i = 0; i < 2; i++) {
              if (cancelled[i]) continue
              if (result.done) controllers[i].close()
              else controllers[i].enqueue(result.value)
            }
          },
          // reader.closed carries the error to the branches.
          noop,
        )
        return Promise.resolve()
      }
      let cancel = (i) => (reason) => {
        cancelled[i] = true
        reasons[i] = reason
        if (cancelled[0] && cancelled[1]) reader.cancel(reasons).then(sourceCancelled.resolve, sourceCancelled.reject)
        return sourceCancelled.promise
      }
      let branches = [0, 1].map(
        (i) =>
          new ReadableStream({
            start(controller) {
              controllers[i] = controller
            },
            pull,
            cancel: cancel(i),
          }),
      )
      reader.closed.catch((e) => {
        for (let controller of controllers) controller.error(e)
        if (!cancelled[0] || !cancelled[1]) sourceCancelled.resolve()
      })
      return branches
    }

    // A stream over an async (or sync) iterable: one item is pulled per
    // read, and cancelling the stream returns the iterator.
    static from(iterable) {
      let getAsync = isObject(iterable) ? iterable[Symbol.asyncIterator] : undefined
      let getSync = isObject(iterable) || typeof iterable === "string" ? iterable[Symbol.iterator] : undefined
      let async = typeof getAsync === "function"
      if (!async && typeof getSync !== "function") throw new TypeError("ReadableStream.from: the argument is not iterable")
      let iterator = async ? getAsync.call(iterable) : getSync.call(iterable)
      if (!isObject(iterator)) throw new TypeError("ReadableStream.from: the iterator is not an object")
      return new ReadableStream(
        {
          async pull(controller) {
            let result = await iterator.next()
            if (!isObject(result)) throw new TypeError("ReadableStream.from: the iterator result is not an object")
            if (result.done) controller.close()
            else controller.enqueue(async ? result.value : await result.value)
          },
          async cancel(reason) {
            let ret = iterator.return
            if (ret === undefined || ret === null) return
            let result = await ret.call(iterator, reason)
            if (!isObject(result)) throw new TypeError("ReadableStream.from: the iterator result is not an object")
          },
        },
        { highWaterMark: FROM_HIGH_WATER_MARK },
      )
    }
  }

  function checkPipeOptions(options, api) {
    if (options === undefined || options === null) return
    for (let name of ["preventClose", "preventAbort", "preventCancel", "signal"]) {
      if (options[name] !== undefined) throw new TypeError(`${api}: the ${name} option is not supported`)
    }
  }

  // Read `source` into `destination` until the source ends; then close the
  // destination. An errored source aborts the destination, an errored or
  // closed destination cancels the source, and either way the pipe rejects
  // with that error. Both ends are locked for the duration.
  async function pipe(source, destination) {
    let reader = source.getReader()
    let writer = destination.getWriter()
    let rs = internals.get(source)
    let ws = internals.get(destination)
    try {
      while (true) {
        if (ws.state === "errored" || ws.state === "erroring") throw ws.storedError
        if (ws.state !== "writable") throw new TypeError("pipeTo: the destination was closed")
        await writer.ready
        let result = await reader.read()
        if (result.done) break
        handled(writer.write(result.value))
      }
    } catch (e) {
      if (rs.state === "errored") await handled(writer.abort(e))
      else if (rs.state === "readable") await handled(reader.cancel(e))
      reader.releaseLock()
      writer.releaseLock()
      throw e
    }
    reader.releaseLock()
    try {
      await writer.close()
    } finally {
      writer.releaseLock()
    }
  }

  // -- WritableStream --

  // ws: { state, storedError, queue, inFlight, closeRequest, pendingAbort,
  //       started, writer, highWaterMark, write, close, abort }
  // `queue` holds the write requests not yet finished; its head is the one
  // in flight while `inFlight` is set. `writer` is the active writer's
  // internals, or null when unlocked.

  function wsDesiredSize(ws) {
    if (ws.state === "errored" || ws.state === "erroring") return null
    if (ws.state === "closed" || ws.state === "closing") return 0
    return ws.highWaterMark - ws.queue.length
  }

  function wsBackpressure(ws) {
    return wsDesiredSize(ws) <= 0
  }

  // Keep the writer's `ready` in step with backpressure: pending while the
  // queue is at the high-water mark, fulfilled once it drains.
  function wsUpdateReady(ws) {
    let writer = ws.writer
    if (writer === null || ws.state !== "writable") return
    if (wsBackpressure(ws)) {
      if (writer.readySettled) {
        writer.ready = deferred()
        writer.readySettled = false
        handled(writer.ready.promise)
      }
    } else if (!writer.readySettled) {
      writer.ready.resolve()
      writer.readySettled = true
    }
  }

  function wsAdvance(ws) {
    if (!ws.started || ws.inFlight || ws.state === "erroring" || ws.state === "errored" || ws.state === "closed") return
    if (ws.queue.length > 0) {
      let request = ws.queue[0]
      ws.inFlight = true
      ws.write(request.chunk).then(
        () => {
          ws.inFlight = false
          ws.queue.shift()
          request.resolve()
          if (ws.state === "erroring") wsFinishErroring(ws)
          else {
            wsUpdateReady(ws)
            wsAdvance(ws)
          }
        },
        (e) => {
          ws.inFlight = false
          ws.queue.shift()
          request.reject(e)
          if (ws.state === "erroring") wsFinishErroring(ws)
          else wsStartErroring(ws, e)
        },
      )
      return
    }
    if (ws.closeRequest !== null && ws.state === "closing") {
      let request = ws.closeRequest
      ws.closeRequest = null
      ws.inFlight = true
      ws.close().then(
        () => {
          ws.inFlight = false
          // An abort that arrived while the close was in flight is moot:
          // the sink closed cleanly.
          let abort = ws.pendingAbort
          ws.pendingAbort = null
          if (abort !== null) abort.resolve()
          ws.state = "closed"
          request.resolve()
          if (ws.writer !== null) ws.writer.closed.resolve()
        },
        (e) => {
          ws.inFlight = false
          request.reject(e)
          if (ws.state === "erroring") wsFinishErroring(ws)
          else wsStartErroring(ws, e)
        },
      )
    }
  }

  function wsStartErroring(ws, e) {
    if (ws.state !== "writable" && ws.state !== "closing") return
    ws.state = "erroring"
    ws.storedError = e
    let writer = ws.writer
    if (writer !== null && !writer.readySettled) {
      writer.ready.reject(e)
      writer.readySettled = true
    }
    if (!ws.inFlight) wsFinishErroring(ws)
  }

  function wsFinishErroring(ws) {
    ws.state = "errored"
    let e = ws.storedError
    for (let request of ws.queue.splice(0)) request.reject(e)
    if (ws.closeRequest !== null) {
      ws.closeRequest.reject(e)
      ws.closeRequest = null
    }
    let abort = ws.pendingAbort
    ws.pendingAbort = null
    let aborted = abort === null ? Promise.resolve() : ws.abort(abort.reason)
    aborted.then(
      () => {
        if (abort !== null) abort.resolve()
        if (ws.writer !== null) ws.writer.closed.reject(e)
      },
      (abortError) => {
        if (abort !== null) abort.reject(abortError)
        if (ws.writer !== null) ws.writer.closed.reject(e)
      },
    )
  }

  function wsWrite(ws, chunk) {
    if (ws.state === "errored" || ws.state === "erroring") return Promise.reject(ws.storedError)
    if (ws.state !== "writable") return Promise.reject(new TypeError("WritableStream: the stream is closing or closed"))
    let request = deferred()
    request.chunk = chunk
    ws.queue.push(request)
    wsUpdateReady(ws)
    wsAdvance(ws)
    return request.promise
  }

  function wsClose(ws) {
    if (ws.state === "errored" || ws.state === "erroring") return Promise.reject(ws.storedError)
    if (ws.state !== "writable") return Promise.reject(new TypeError("WritableStream: the stream is closing or closed"))
    ws.state = "closing"
    ws.closeRequest = deferred()
    let promise = ws.closeRequest.promise
    let writer = ws.writer
    if (writer !== null && !writer.readySettled) {
      writer.ready.resolve()
      writer.readySettled = true
    }
    wsAdvance(ws)
    return promise
  }

  function wsAbort(ws, reason) {
    if (ws.state === "errored" || ws.state === "closed") return Promise.resolve()
    if (ws.pendingAbort !== null) return ws.pendingAbort.promise
    let abort = deferred()
    abort.reason = reason
    ws.pendingAbort = abort
    if (ws.state === "writable" || ws.state === "closing") wsStartErroring(ws, reason)
    return abort.promise
  }

  class WritableStreamDefaultController {
    constructor(ws) {
      internals.set(this, ws)
    }

    error(e) {
      wsStartErroring(internals.get(this), e)
    }
  }

  class WritableStreamDefaultWriter {
    constructor(stream) {
      let ws = internals.get(stream)
      if (ws === undefined || !(stream instanceof WritableStream)) {
        throw new TypeError("WritableStreamDefaultWriter: not a WritableStream")
      }
      if (ws.writer !== null) throw new TypeError("WritableStream: the stream is locked")
      let writer = { ws, ready: deferred(), readySettled: false, closed: deferred() }
      handled(writer.ready.promise)
      handled(writer.closed.promise)
      if (ws.state === "errored" || ws.state === "erroring") {
        writer.ready.reject(ws.storedError)
        writer.readySettled = true
        if (ws.state === "errored") writer.closed.reject(ws.storedError)
      } else if (ws.state === "closed") {
        writer.ready.resolve()
        writer.readySettled = true
        writer.closed.resolve()
      } else if (ws.state === "closing" || !wsBackpressure(ws)) {
        writer.ready.resolve()
        writer.readySettled = true
      }
      ws.writer = writer
      internals.set(this, writer)
    }

    get ready() {
      return internals.get(this).ready.promise
    }

    get closed() {
      return internals.get(this).closed.promise
    }

    get desiredSize() {
      let writer = internals.get(this)
      if (writer.ws === null) throw new TypeError("WritableStreamDefaultWriter: the writer was released")
      return wsDesiredSize(writer.ws)
    }

    write(chunk) {
      let writer = internals.get(this)
      if (writer.ws === null) return Promise.reject(new TypeError("WritableStreamDefaultWriter: the writer was released"))
      return wsWrite(writer.ws, chunk)
    }

    close() {
      let writer = internals.get(this)
      if (writer.ws === null) return Promise.reject(new TypeError("WritableStreamDefaultWriter: the writer was released"))
      return wsClose(writer.ws)
    }

    abort(reason) {
      let writer = internals.get(this)
      if (writer.ws === null) return Promise.reject(new TypeError("WritableStreamDefaultWriter: the writer was released"))
      return wsAbort(writer.ws, reason)
    }

    releaseLock() {
      let writer = internals.get(this)
      let ws = writer.ws
      if (ws === null) return
      let released = new TypeError("WritableStreamDefaultWriter: the writer was released")
      if (!writer.readySettled) {
        writer.ready.reject(released)
        writer.readySettled = true
      }
      if (ws.state !== "closed" && ws.state !== "errored") writer.closed.reject(released)
      ws.writer = null
      writer.ws = null
    }
  }

  class WritableStream {
    constructor(underlyingSink = {}, strategy = {}) {
      if (underlyingSink === null) underlyingSink = {}
      if (!isObject(underlyingSink)) throw new TypeError("WritableStream: the underlying sink must be an object")
      if (underlyingSink.type !== undefined) throw new RangeError("WritableStream: the type option is not supported")
      let start = methodOf(underlyingSink, "start", "WritableStream")
      let write = methodOf(underlyingSink, "write", "WritableStream")
      let close = methodOf(underlyingSink, "close", "WritableStream")
      let abort = methodOf(underlyingSink, "abort", "WritableStream")
      let ws = {
        state: "writable",
        storedError: undefined,
        queue: [],
        inFlight: false,
        closeRequest: null,
        pendingAbort: null,
        started: false,
        writer: null,
        highWaterMark: highWaterMarkOf(strategy, DEFAULT_HIGH_WATER_MARK, "WritableStream"),
        write: null,
        close: null,
        abort: null,
      }
      let controller = new WritableStreamDefaultController(ws)
      ws.write = (chunk) => invoke(write, underlyingSink, [chunk, controller])
      ws.close = () => invoke(close, underlyingSink, [])
      ws.abort = (reason) => invoke(abort, underlyingSink, [reason])
      internals.set(this, ws)
      invoke(start, underlyingSink, [controller]).then(
        () => {
          ws.started = true
          if (ws.state === "erroring") wsFinishErroring(ws)
          else wsAdvance(ws)
        },
        (e) => {
          ws.started = true
          wsStartErroring(ws, e)
        },
      )
    }

    get locked() {
      return internals.get(this).writer !== null
    }

    getWriter() {
      return new WritableStreamDefaultWriter(this)
    }

    abort(reason) {
      let ws = internals.get(this)
      if (ws.writer !== null) return Promise.reject(new TypeError("WritableStream: the stream is locked"))
      return wsAbort(ws, reason)
    }

    close() {
      let ws = internals.get(this)
      if (ws.writer !== null) return Promise.reject(new TypeError("WritableStream: the stream is locked"))
      return wsClose(ws)
    }
  }

  // -- TransformStream --

  // ts: { readable, writable, readableController, backpressure, changed }
  // Backpressure runs from the readable side to the writable: a write waits
  // while the readable side's queue is full (`backpressure`), and `changed`
  // settles each time that flips.

  function tsSetBackpressure(ts, value) {
    if (ts.backpressure === value) return
    ts.backpressure = value
    let changed = ts.changed
    ts.changed = deferred()
    changed.resolve()
  }

  function tsErrorWritable(ts, e) {
    wsStartErroring(internals.get(ts.writable), e)
    tsSetBackpressure(ts, false)
  }

  function tsError(ts, e) {
    ts.readableController.error(e)
    tsErrorWritable(ts, e)
  }

  class TransformStreamDefaultController {
    constructor(ts) {
      internals.set(this, ts)
    }

    get desiredSize() {
      return internals.get(this).readableController.desiredSize
    }

    enqueue(chunk) {
      let ts = internals.get(this)
      let rs = internals.get(ts.readable)
      if (rs.closeRequested || rs.state !== "readable") throw new TypeError("TransformStream: the readable side is closed")
      try {
        ts.readableController.enqueue(chunk)
      } catch (e) {
        tsErrorWritable(ts, e)
        throw rs.storedError
      }
      if (rsDesiredSize(rs) <= 0) tsSetBackpressure(ts, true)
    }

    error(e) {
      tsError(internals.get(this), e)
    }

    terminate() {
      let ts = internals.get(this)
      let rs = internals.get(ts.readable)
      if (rs.state === "readable" && !rs.closeRequested) ts.readableController.close()
      tsErrorWritable(ts, new TypeError("TransformStream: terminated"))
    }
  }

  class TransformStream {
    constructor(transformer = {}, writableStrategy = {}, readableStrategy = {}) {
      if (transformer === null) transformer = {}
      if (!isObject(transformer)) throw new TypeError("TransformStream: the transformer must be an object")
      if (transformer.readableType !== undefined || transformer.writableType !== undefined) {
        throw new RangeError("TransformStream: readableType and writableType are not supported")
      }
      let start = methodOf(transformer, "start", "TransformStream")
      let transform = methodOf(transformer, "transform", "TransformStream")
      let flush = methodOf(transformer, "flush", "TransformStream")
      let ts = { readable: null, writable: null, readableController: null, backpressure: true, changed: deferred() }
      let controller = new TransformStreamDefaultController(ts)
      let started = invoke(start, transformer, [controller])
      // The identity transform passes the chunk through.
      let transformChunk = (chunk) =>
        (transform === undefined ? invoke(controller.enqueue, controller, [chunk]) : invoke(transform, transformer, [chunk, controller])).catch(
          (e) => {
            tsError(ts, e)
            throw e
          },
        )
      ts.readable = new ReadableStream(
        {
          start(readableController) {
            ts.readableController = readableController
            return started
          },
          pull() {
            tsSetBackpressure(ts, false)
            return ts.changed.promise
          },
          cancel(reason) {
            tsErrorWritable(ts, reason)
          },
        },
        { highWaterMark: highWaterMarkOf(readableStrategy, TRANSFORM_READABLE_HIGH_WATER_MARK, "TransformStream") },
      )
      ts.writable = new WritableStream(
        {
          start() {
            return started
          },
          async write(chunk) {
            if (ts.backpressure) {
              await ts.changed.promise
              let ws = internals.get(ts.writable)
              if (ws.state === "erroring" || ws.state === "errored") throw ws.storedError
            }
            return transformChunk(chunk)
          },
          async close() {
            try {
              await invoke(flush, transformer, [controller])
            } catch (e) {
              tsError(ts, e)
              throw e
            }
            let rs = internals.get(ts.readable)
            if (rs.state === "readable" && !rs.closeRequested) ts.readableController.close()
          },
          abort(reason) {
            let rs = internals.get(ts.readable)
            if (rs.state === "readable") ts.readableController.error(reason)
          },
        },
        { highWaterMark: highWaterMarkOf(writableStrategy, DEFAULT_HIGH_WATER_MARK, "TransformStream") },
      )
      internals.set(this, ts)
    }

    get readable() {
      return internals.get(this).readable
    }

    get writable() {
      return internals.get(this).writable
    }
  }

  // -- The text streams --

  class TextDecoderStream {
    constructor(label, options) {
      let decoder = new TextDecoder(label, options)
      let transform = new TransformStream({
        transform(chunk, controller) {
          let text = decoder.decode(chunk, { stream: true })
          if (text !== "") controller.enqueue(text)
        },
        flush(controller) {
          let text = decoder.decode()
          if (text !== "") controller.enqueue(text)
        },
      })
      internals.set(this, { decoder, transform })
    }

    get encoding() {
      return internals.get(this).decoder.encoding
    }

    get fatal() {
      return internals.get(this).decoder.fatal
    }

    get ignoreBOM() {
      return internals.get(this).decoder.ignoreBOM
    }

    get readable() {
      return internals.get(this).transform.readable
    }

    get writable() {
      return internals.get(this).transform.writable
    }
  }

  class TextEncoderStream {
    constructor() {
      let encoder = new TextEncoder()
      // A high surrogate at the end of a chunk waits for its low half.
      let pending = ""
      let transform = new TransformStream({
        transform(chunk, controller) {
          let text = pending + String(chunk)
          pending = ""
          let last = text.charCodeAt(text.length - 1)
          if (last >= HIGH_SURROGATE_FIRST && last <= HIGH_SURROGATE_LAST) {
            pending = text.slice(-1)
            text = text.slice(0, -1)
          }
          if (text !== "") controller.enqueue(encoder.encode(text))
        },
        flush(controller) {
          if (pending !== "") controller.enqueue(encoder.encode(pending))
        },
      })
      internals.set(this, { encoder, transform })
    }

    get encoding() {
      return internals.get(this).encoder.encoding
    }

    get readable() {
      return internals.get(this).transform.readable
    }

    get writable() {
      return internals.get(this).transform.writable
    }
  }

  // -- The compression streams --

  // A chunk written to a compression stream, as the bytes the codec takes:
  // an ArrayBuffer or any view of one (the standard's BufferSource).
  function bytesOf(chunk, api) {
    if (chunk instanceof Uint8Array) return chunk
    if (chunk instanceof ArrayBuffer) return new Uint8Array(chunk)
    if (ArrayBuffer.isView(chunk)) return new Uint8Array(chunk.buffer, chunk.byteOffset, chunk.byteLength)
    throw new TypeError(`${api}: a chunk must be an ArrayBuffer or an ArrayBuffer view`)
  }

  // Whether the readable side still takes chunks: not once it is cancelled,
  // errored or closing.
  function tsReadable(ts) {
    let rs = internals.get(ts.readable)
    return rs.state === "readable" && !rs.closeRequested
  }

  // A TransformStream over a native deflate codec: a chunk is one codec
  // step on the codec threads, and the output comes back in pieces of at
  // most the codec's piece size, each handed on once the reader asks for
  // it, so an input that inflates to a lot never runs ahead of its reader
  // (okf/done/compression-streams.md).
  function codecStream(codec, api) {
    let ts = null
    // One codec call, then its further pieces for what it holds, waiting
    // for the reader between them. A readable side that no longer takes
    // chunks ends the run: nobody is waiting for the rest.
    async function run(controller, call) {
      let piece = await call()
      while (true) {
        if (!tsReadable(ts)) return
        if (piece.length > 0) controller.enqueue(piece)
        if (!codec.pending) return
        while (ts.backpressure) await ts.changed.promise
        if (!tsReadable(ts)) return
        piece = await codec.next()
      }
    }
    let transform = new TransformStream({
      transform(chunk, controller) {
        let bytes = bytesOf(chunk, api)
        return run(controller, () => codec.push(bytes))
      },
      flush(controller) {
        return run(controller, () => codec.finish())
      },
    })
    ts = internals.get(transform)
    return transform
  }

  class CompressionStream {
    constructor(format) {
      internals.set(this, codecStream(new DeflateCodec("compress", format), "CompressionStream"))
    }

    get readable() {
      return internals.get(this).readable
    }

    get writable() {
      return internals.get(this).writable
    }
  }

  class DecompressionStream {
    constructor(format) {
      internals.set(this, codecStream(new DeflateCodec("decompress", format), "DecompressionStream"))
    }

    get readable() {
      return internals.get(this).readable
    }

    get writable() {
      return internals.get(this).writable
    }
  }

  return {
    ReadableStream,
    WritableStream,
    TransformStream,
    TextDecoderStream,
    TextEncoderStream,
    CompressionStream,
    DecompressionStream,
  }
}
