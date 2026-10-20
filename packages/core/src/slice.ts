// Frame-sliced work: a job is an iterator whose every step is some work
// the frame must pay for on the JS thread (a blocking GPU create, a mesh
// built from decoded data, a chunk of procedural generation), and the
// queue runs as many steps as fit in one budget of MILLISECONDS per
// frame, first thing in the frame, measured by the wall clock, so a step
// is charged what the frame actually paid for it on this device. One
// budget for every job in the app (Unreal's time-sliced actor
// registration, Unity's asyncUploadTimeSlice, in one shape): two worlds
// streaming beside each other spend it together, never each their own.
// Jobs run in the order queued, one at a time; a cancelled job's iterator
// is closed (`return()`), so a generator's `finally` releases what its
// steps acquired. The queue demands a frame while work remains, so a
// still scene keeps stepping. Pure JS over an installed DRIVER (the
// clock and the frame request: window.ts installs performance.now and
// the native request; the headless check, tests/slice.test.ts, installs a
// clock of its own and drains by hand), so the pacing is testable where
// the app clock is frozen.

// Milliseconds of sliced work per frame when no app set a budget: Unity's
// asyncUploadTimeSlice default, a fraction of a 60 Hz frame that leaves
// the frame's own work its room.
const DEFAULT_SLICE_MS = 2

let budgetMs = DEFAULT_SLICE_MS

/** What the queue runs on: the clock a step's cost is measured by and
 * the frame request made while work remains (installSliceDriver). */
export type SliceDriver = {
  now(): number
  requestFrame(): void
}

let driver: SliceDriver = { now: () => 0, requestFrame: () => {} }

/** Install the queue's clock and frame request (core's window module
 * does, once; a headless test installs its own). */
export function installSliceDriver(next: SliceDriver): void {
  driver = next
}

export type SliceOptions = {
  /** A name for the job, for diagnostics. */
  label?: string
  /** Called synchronously, inside the frame's drain, as the job's last
   * step completes (`done`), when a step throws (`failed`) or on cancel
   * (`cancelled`): a write made here is in the same frame's flush and
   * paint, where the `done` promise settles after it. */
  onSettle?: (state: "done" | "failed" | "cancelled", error?: unknown) => void
}

export type Sliced = {
  /** Stop the job: its iterator is closed (a generator's `finally` runs)
   * and no further step runs. A finished job ignores it. */
  cancel(): void
  /** Settles when the job's iterator finishes or the job is cancelled;
   * rejects with the error a step threw (the job is then dropped). */
  readonly done: Promise<void>
  /** "queued" until the last step, then "done", "cancelled" or "failed". */
  readonly state: "queued" | "done" | "cancelled" | "failed"
}

type Job = {
  steps: Iterator<void>
  label: string | undefined
  onSettle: SliceOptions["onSettle"]
  state: Sliced["state"]
  resolve: () => void
  reject: (error: unknown) => void
}

let queue: Job[] = []
// The stats: milliseconds the last drain spent, and the steps run so far.
let lastMs = 0
let stepsTotal = 0

/** The queue's figures: the milliseconds the last frame's drain spent,
 * the jobs waiting or under way, the steps run so far. */
export function sliceStats(): { ms: number; queued: number; steps: number } {
  return { ms: lastMs, queued: queue.length, steps: stepsTotal }
}

/**
 * The milliseconds of sliced work (`slice`) every frame spends, for every
 * job in the app together (default 2): the frame's drain runs steps until
 * the budget is passed, so one oversize step may overrun it and the next
 * waits a frame. A positive number or Infinity (run every queued step in
 * the frame it was queued in); throws otherwise.
 */
export function setSliceBudget(ms: number): void {
  if (!(ms > 0) || Number.isNaN(ms)) throw new Error(`setSliceBudget: milliseconds per frame must be positive, got ${ms}`)
  budgetMs = ms
}

/** The slice budget in force (setSliceBudget). */
export function sliceBudget(): number {
  return budgetMs
}

/**
 * Queue an iterator (a generator object, or anything iterable over
 * `void` steps) to run under the frame slice budget: each `next()` is one
 * step, run first thing in a frame after the jobs queued before it, as
 * many per frame as the budget allows, timed by the wall clock. The
 * handle cancels the job and settles when it is done. A job queued
 * outside a frame requests one.
 */
export function slice(steps: Iterator<void> | Iterable<void>, opts?: SliceOptions): Sliced {
  let iterator: Iterator<void> =
    typeof (steps as Iterator<void>).next === "function" ? (steps as Iterator<void>) : (steps as Iterable<void>)[Symbol.iterator]()
  let job: Job = { steps: iterator, label: opts?.label, onSettle: opts?.onSettle, state: "queued", resolve: () => {}, reject: () => {} }
  let done = new Promise<void>((resolve, reject) => {
    job.resolve = resolve
    job.reject = reject
  })
  queue.push(job)
  driver.requestFrame()
  return {
    cancel() {
      if (job.state !== "queued") return
      job.state = "cancelled"
      let i = queue.indexOf(job)
      if (i >= 0) queue.splice(i, 1)
      try {
        job.steps.return?.()
      } catch (error) {
        console.error("A sliced job threw while being cancelled" + (job.label ? ` (${job.label})` : "") + ":", error)
      }
      job.onSettle?.("cancelled")
      job.resolve()
    },
    get done() {
      return done
    },
    get state() {
      return job.state
    },
  }
}

/**
 * Run the queued jobs' steps for this frame, in queue order, until the
 * budget is spent or the queue is empty (core's frame runs it first, so a
 * step's writes land in this frame's flush and paint). Demands another
 * frame while work remains.
 */
export function drainSlices(): void {
  let start = driver.now()
  let spent = 0
  while (queue.length > 0 && spent < budgetMs) {
    let job = queue[0]!
    let result: IteratorResult<void>
    try {
      result = job.steps.next()
    } catch (error) {
      queue.shift()
      job.state = "failed"
      job.onSettle?.("failed", error)
      job.reject(error)
      spent = driver.now() - start
      continue
    }
    stepsTotal++
    if (result.done) {
      queue.shift()
      job.state = "done"
      job.onSettle?.("done")
      job.resolve()
    }
    spent = driver.now() - start
  }
  lastMs = spent
  if (queue.length > 0) driver.requestFrame()
}

/** The jobs waiting or under way. */
export function slicesQueued(): number {
  return queue.length
}
