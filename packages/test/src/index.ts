// Tests for a SolidRT app: `test` registers one and hands it the app under
// test, `expect` asserts. The base is flux:test (flat tests, every test in
// an engine of its own, a seeded Math.random); what this adds is the app
// layer, on the dev client's test mode (okf/done/test-harness.md).
//
// An app test has no wall clock. Time is the frames the test asks for:
// nothing runs between two frames, timers fire with the frame their time
// falls in, performance.now() reads 0 and the calendar starts at a fixed
// epoch and moves with the frames. So a test does the same on every run
// and never waits.
//
// Nodes are named the way a user or an agent names them: by the text they
// show, by a `label`, or by a ref the test holds. Input goes through the
// real pipeline (hit test, recognizers, focus) in the control API's event
// shape, so what an agent did by hand over MCP transcribes into a test.

import { test as register, expect } from "flux:test"
import * as gui from "flux:test/gui"
import { capture, debug, frame as stepFrame, frameRate, inputPlan, inputStep, link, painted as paintFrame, setFrameRate, settle, time, windowReady } from "sol:test"
import { createElement, insert, render } from "@solidrt/core"

export { expect }

// How far below a whole frame count a time still rounds down to it:
// ms * fps / 1000 of an exact multiple can land a rounding error above.
const FRAME_COUNT_EPSILON = 1e-9
// How many nodes one query returns at most. Far more than a test means to
// name at once; a query that reaches it is reported as such.
const FIND_LIMIT = 1000
// How many of the texts and labels in scope a failed `find` lists, so the
// message says what was there instead without printing a whole screen.
const FOUND_INSTEAD_MAX = 12
// How much app time `settle` gives an app to come to rest unless the call
// says otherwise: far past any transition an app runs, short enough that a
// frame callback that never stops fails promptly.
const SETTLE_MAX_MS = 5000
// Bytes per pixel of a capture.
const RGBA = 4

/** A window point, in logical pixels. */
export interface Point {
  x: number
  y: number
}

/**
 * What a node is named by. Every field given has to hold. A string matches
 * exactly; a RegExp matches a part.
 */
export interface Query {
  /** The text a text node shows. */
  text?: string | RegExp
  /** The node's `label` prop. */
  label?: string | RegExp
  /** The element kind: "view", "text", "rect", ... */
  kind?: string
}

/**
 * A node as the control API's `/tree` reports it (the fields are documented
 * with it, in the debugging guide of `@solidrt/cli`): the painted box in
 * window coordinates, the text and label where there are any, and the
 * off-default props under their JSX names.
 */
export interface NodeRecord {
  id: number
  kind: string
  x: number
  y: number
  width: number
  height: number
  text?: string
  label?: string
  props?: Record<string, unknown>
  children?: NodeRecord[]
  [field: string]: unknown
}

/**
 * A name for a node, resolved each time it is used, so it stays valid while
 * the tree changes. Reading or acting through a locator that names no node,
 * or more than one, throws: `findAll` is for the plural case.
 */
export interface Locator {
  /** The node this names within this one's subtree. */
  find(query: Query): Locator
  /** Every node the query matches within this one's subtree, as it is now. */
  findAll(query: Query): Locator[]
  /** Whether this names exactly one node right now. Never throws. */
  readonly exists: boolean
  /**
   * Whether the node is painted where a user could see it: mounted, its
   * painted box not cut away by a clipping ancestor or the window, and not
   * faded out. False when the node is not there. It says nothing of what
   * is painted over it; a tap does (it fails on a covered node).
   */
  readonly visible: boolean
  /** The text the node shows; undefined for a node that is no text. */
  readonly text: string | undefined
  /** The painted box, in window coordinates. */
  readonly box: { x: number; y: number; width: number; height: number }
  /** The node's off-default props, under their JSX names. */
  readonly props: Record<string, unknown>
  /** The node's whole record. */
  readonly record: NodeRecord
  /**
   * The subtree as an outline: one line per node, indented by depth, with
   * its kind, label, text and painted box (`view [save] 20,50 120x40`). No
   * ids, so it reads the same on every run and diffs cleanly: compare it
   * to a string the test holds to pin a whole layout, or print it to see
   * what is there. `{ props: true }` adds each node's off-default props. A
   * failed test prints the whole app this way.
   */
  outline(options?: { props?: boolean }): string
  /**
   * The pixels the node paints, its subtree included. Drawn now, from the
   * tree as it is: no frame runs and no app time passes, so a transition
   * is read where it stands. The last resort of a test, for what only the
   * picture shows (a shader, a gradient, a blend): a node's text, box and
   * props say the rest, cheaper and on any GPU.
   */
  pixels(): Pixels
  /**
   * The color at a point of the node, in its own coordinates (0, 0 is the
   * top left of its painted box): `[r, g, b, a]`, 0 to 255. See `pixels`.
   *
   * @example
   * expect(swatch.pixel(10, 10)).toEqual([255, 0, 0, 255])
   */
  pixel(x: number, y: number): [r: number, g: number, b: number, a: number]
}

/**
 * Pixels read back from the GPU: RGBA8, rows top to bottom, alpha
 * premultiplied (a half-transparent red reads `[128, 0, 0, 128]`), at the
 * display scale. The shape `captureSnapshot` and `readTexture` return.
 */
export interface Pixels {
  width: number
  height: number
  data: Uint8Array
}

/** A locator that is also the `ref` of the node it names: `<view ref={it} />`. */
export type RefLocator = Locator & ((node: { id: number }) => void)

/** One event in the control API's `/input` shape. */
export type InputEvent = Record<string, unknown> & { type: "pointer" | "key" | "wheel" | "text" | "gamepad" | "back" }

export interface PointerOptions {
  /** "mouse" (the default) or "touch". */
  pointerType?: "mouse" | "touch"
  /** The mouse button, 0 (left, the default) to 4. */
  button?: number
  shift?: boolean
  ctrl?: boolean
  alt?: boolean
  meta?: boolean
}

/** The app under test, as a test's function receives it. */
export interface TestApp {
  /**
   * Mounts `ui` and returns a locator for the window it is in. `ui` is what
   * an app hands `render`: its `<window>`, or any content, which is then
   * put in a window of its own. Once per test (a test is one app). The
   * first frame is built when this returns; no app time has passed, and
   * no animation clock is stamped yet: a transition written before the
   * first `frame` or `advance` starts at the first frame that runs (the
   * startup anchor), not at `time` 0, so a test timing a track against
   * `time` runs one frame first and writes after it.
   * `ui` runs as a component body, the owner of what it creates: a
   * pointer feed, an input map, a GPU buffer or a scene is created inside
   * it, never at the top of the test.
   */
  mount(ui: () => unknown): Promise<Locator>
  /**
   * Starts the app an entry module is, the way the runtime starts it: pass
   * a function that imports the entry, whose top level calls `render`.
   * Returns a locator for its window, the first frame built. In place of
   * `mount`, once per test; every test gets the entry evaluated afresh.
   *
   * @example
   * let root = await app.load(() => import("../src/index.tsx"))
   */
  load(entry: () => Promise<unknown>): Promise<Locator>
  /**
   * Delivers a link to the app the way the OS delivers one (the raw string,
   * on `onLink`) and runs the frame it lands in. Throws when the app does
   * not listen for links. What the link opens may load or animate:
   * `settle()` after it.
   */
  link(link: string): Promise<void>
  /**
   * Calls a debug command the app registered (`registerDebug` of
   * `sol:dev`), runs a frame, and returns what the command returned. The
   * way to put a loaded app into a state its UI reaches slowly or not at
   * all: the same commands an agent calls over MCP. Throws on an unknown
   * name, listing the registered ones.
   */
  debug(name: string, args?: unknown): Promise<unknown>
  /**
   * The GPU resource inventory, as the control API's `/gpu` reports it:
   * textures, buffers, pipelines and every draw target's entries with
   * their counts and uniforms. `label` keeps the resources created with
   * that label; `draw` names the draw entry reported in full. "Does it
   * draw, and with what" is read here, not off pixels.
   */
  gpu(options?: { label?: string; draw?: number }): Record<string, unknown>
  /** The node the query names, anywhere in the app. */
  find(query: Query): Locator
  /** Every node the query matches, as the tree is now. */
  findAll(query: Query): Locator[]
  /**
   * A locator for a node the test's own JSX creates, which is also its ref:
   * `let box = app.ref()`, then `<view ref={box} />` and `box.box.width`.
   */
  ref(): RefLocator
  /**
   * Runs `count` frames (default 1), one after another. A frame fires the
   * timers due by its time, runs the frame callbacks and the flush, and
   * draws if anything demanded a draw.
   */
  frame(count?: number): Promise<void>
  /**
   * Runs one frame and returns what its paint drew of the node: the
   * capture is queued ahead of the frame, so the frame's own paint
   * services it, and the image is that frame exactly as its callbacks,
   * effects and the extensions' publishes drew it. The probe for "what a
   * frame writes is in that frame", where `pixels()` paints the tree
   * afresh after the fact. One frame of app time passes.
   *
   * @example
   * let frame = await app.painted(leaf)
   * expect(frame.data[0]).toBe(255)
   */
  painted(target: Locator): Promise<Pixels>
  /**
   * Runs frames until `ms` of app time have passed: as many as it takes to
   * cover it, so at 60 fps `advance(100)` is 6 frames. The frame is the
   * time resolution of a test: a timer due at 50 ms fires with the frame at
   * 50 ms, one due at 55 ms with the frame at 66.7 ms.
   */
  advance(ms: number): Promise<void>
  /** App time as of the last frame, in milliseconds; 0 when the test starts. */
  readonly time: number
  /**
   * Runs the app until it is at rest: nothing it started is still in flight
   * (a fetch, a file read, a query), no timer is due, and no frame is
   * demanded. Work in flight is waited for with no app time passing; a
   * demanded frame and a timer that is due are run, so a running transition
   * is played to its end and the test reads the end state. A timer due
   * later is not waited for: `advance` reaches it. What stands (a server,
   * an open socket) is not waited for either.
   *
   * Throws once `maxMs` of app time (default 5000) have passed without
   * rest, naming what is left: a frame callback that never stops, a looping
   * animation, a playing video.
   *
   * @example
   * await app.tap(app.find({ text: "Load" }))
   * await app.settle()
   * expect(app.find({ label: "rows" }).findAll({ kind: "text" }).length).toBe(20)
   */
  settle(options?: { maxMs?: number }): Promise<void>
  /**
   * A tap at the node's painted center, or at a point: down, a frame (and
   * `holdMs`, if given), up, and one frame more, so the tree is current
   * when this returns. A mouse is moved there a frame ahead. Naming a node
   * that something else covers at that point throws, saying what is there;
   * a point lands on whatever is there.
   */
  tap(target: Locator | Point, options?: PointerOptions & { holdMs?: number }): Promise<void>
  /**
   * A drag from one node's center (or point) to another's: down, one move
   * per frame along the line over `durationMs` (default 300), up.
   */
  drag(from: Locator | Point, to: Locator | Point, options?: PointerOptions & { durationMs?: number }): Promise<void>
  /** A key pressed and released a frame (and `holdMs`) later, by its W3C key name ("Enter", "a"). */
  key(key: string, options?: { holdMs?: number; shift?: boolean; ctrl?: boolean; alt?: boolean; meta?: boolean }): Promise<void>
  /** Text as an input method commits it, to the focused node. */
  type(text: string): Promise<void>
  /**
   * The user's back intent, the event the Android button, a pad's back
   * button and the desktop dev chord become: into the `onBack` stack,
   * never a key.
   */
  back(): Promise<void>
  /**
   * Events in the control API's `/input` shape, as `send_input` takes them.
   * `delayMs` and `holdMs` are app time: frames run to cover them. One
   * frame runs after the last event.
   */
  input(events: InputEvent[]): Promise<void>
}

export interface TestOptions {
  /**
   * The frame rate the test steps at, a positive integer (default 60). A
   * test that asserts a threshold to the millisecond sets 1000.
   */
  fps?: number
}

// -- Locators --

function describeQuery(query: Query): string {
  let parts = Object.entries(query).map(([name, value]) => `${name}: ${value instanceof RegExp ? value : JSON.stringify(value)}`)
  return `{ ${parts.join(", ")} }`
}

function describeNode(record: NodeRecord): string {
  if (record.text !== undefined) return `${record.kind} ${JSON.stringify(record.text)}`
  if (record.label !== undefined) return `${record.kind} labelled ${JSON.stringify(record.label)}`
  return `${record.kind} #${record.id}`
}

function field(value: string | RegExp | undefined): string | true | undefined {
  return value instanceof RegExp ? true : value
}

function holds(value: string | RegExp | undefined, actual: string | undefined): boolean {
  return !(value instanceof RegExp) || (actual !== undefined && value.test(actual))
}

// The records the query matches under `root` (null: the whole app).
function search(root: number | null, query: Query): NodeRecord[] {
  if (query === null || typeof query !== "object") throw new TypeError("find: the query must be an object, like { text: \"Save\" }")
  let names = Object.keys(query)
  let unknown = names.find(name => name !== "text" && name !== "label" && name !== "kind")
  if (unknown !== undefined) throw new TypeError(`find: unknown query field "${unknown}"; a node is named by text, label or kind`)
  if (names.length === 0) throw new TypeError("find: the query names nothing; give it a text, a label or a kind")
  let found = gui.find(root, { kind: query.kind, text: field(query.text), label: field(query.label) }, FIND_LIMIT)
  if (found === null) return []
  let records = JSON.parse(found) as NodeRecord[]
  if (records.length === FIND_LIMIT) throw new Error(`find: ${describeQuery(query)} matches ${FIND_LIMIT} nodes or more; name fewer`)
  return records.filter(record => holds(query.text, record.text) && holds(query.label, record.label))
}

// What a failed find says was there instead: the texts and labels in scope.
function foundInstead(root: number | null): string {
  let texts = gui.find(root, { text: true }, FOUND_INSTEAD_MAX)
  let labels = gui.find(root, { label: true }, FOUND_INSTEAD_MAX)
  let parts: string[] = []
  let list = (json: string | null, pick: (record: NodeRecord) => string | undefined) =>
    (json === null ? [] : (JSON.parse(json) as NodeRecord[])).map(record => JSON.stringify(pick(record))).join(", ")
  let shown = list(texts, record => record.text)
  let named = list(labels, record => record.label)
  if (shown !== "") parts.push(`texts there: ${shown}`)
  if (named !== "") parts.push(`labels there: ${named}`)
  return parts.length === 0 ? "nothing with a text or a label is there" : parts.join("; ")
}

// A locator over `resolve`, which answers the ids it names right now.
// `what` says how it names them, for messages; `scope` is where a failed
// find looks for what was there instead.
function locatorFields(resolve: () => number[], what: string, scope: () => number | null): PropertyDescriptorMap {
  let one = (): number => {
    let ids = resolve()
    if (ids.length === 1) return ids[0]!
    if (ids.length === 0) throw new Error(`No node matches ${what}; ${foundInstead(scope())}`)
    throw new Error(`${ids.length} nodes match ${what}; a locator names one node (findAll returns them all)`)
  }
  let record = (): NodeRecord => {
    let id = one()
    let json = gui.node(id, 0)
    if (json === null) throw new Error(`The node ${what} named is gone`)
    return JSON.parse(json) as NodeRecord
  }
  return {
    find: { value: (query: Query) => findIn(() => one(), query) },
    findAll: { value: (query: Query) => search(one(), query).map(found => fixed(found.id, `${describeQuery(query)} (${describeNode(found)})`)) },
    exists: { get: () => resolve().length === 1 },
    visible: {
      get: () => {
        let ids = resolve()
        if (ids.length > 1) one()
        return ids.length === 1 && gui.visible(ids[0]!)
      },
    },
    text: { get: () => record().text },
    box: {
      get: () => {
        let { x, y, width, height } = record()
        return { x, y, width, height }
      },
    },
    props: { get: () => record().props ?? {} },
    record: { get: record },
    outline: {
      value: (options: { props?: boolean } = {}) => {
        let text = gui.outline(one(), options.props === true)
        if (text === null) throw new Error(`The node ${what} named is gone`)
        return text
      },
    },
    pixels: { value: () => capture(one()) },
    pixel: {
      value: (x: number, y: number) => {
        let { width, height } = record()
        if (!(x >= 0 && x < width && y >= 0 && y < height)) throw new RangeError(`pixel: ${x}, ${y} is outside ${what}, which is ${width} x ${height}`)
        let image = capture(one())
        let column = Math.min(image.width - 1, Math.floor((x * image.width) / width))
        let row = Math.min(image.height - 1, Math.floor((y * image.height) / height))
        let at = (row * image.width + column) * RGBA
        return Array.from(image.data.subarray(at, at + RGBA))
      },
    },
  }
}

function locator(resolve: () => number[], what: string, scope: () => number | null): Locator {
  return Object.defineProperties({}, locatorFields(resolve, what, scope)) as Locator
}

// A locator for one node by id, which stays that node: gone once it is.
function fixed(id: number, what: string): Locator {
  return locator(() => (gui.node(id, 0) === null ? [] : [id]), what, () => null)
}

// A locator for what `query` names under `root` (null: the whole app).
function findIn(root: () => number | null, query: Query): Locator {
  // Checked now, so a malformed query fails where it is written.
  search(null, query)
  return locator(() => search(root(), query).map(found => found.id), describeQuery(query), root)
}

// -- Input --

function isLocator(target: Locator | Point): target is Locator {
  return "record" in target
}

// Where a gesture on `target` lands, and that it lands on it: the center of
// a node's painted box, which must reach the node when it is hit tested.
function pointOf(target: Locator | Point, verb: string): Point {
  if (!isLocator(target)) return target
  let record = target.record
  let at = { x: record.x + record.width / 2, y: record.y + record.height / 2 }
  if (!gui.visible(record.id)) throw new Error(`${verb}: ${describeNode(record)} is not visible (its painted box is ${record.width} x ${record.height} at ${record.x}, ${record.y})`)
  let hit = gui.hit(at.x, at.y)
  let landed = hit[hit.length - 1]
  let own = gui.path(record.id) ?? []
  // Reaching the node, something inside it, or the container it sits in
  // (a node that takes no pointer events itself) is reaching it.
  if (landed === undefined) throw new Error(`${verb}: nothing takes a pointer at ${at.x}, ${at.y}, the center of ${describeNode(record)}`)
  if (!hit.includes(record.id) && !own.includes(landed)) {
    let cover = gui.node(landed, 0)
    let there = cover === null ? `node #${landed}` : describeNode(JSON.parse(cover) as NodeRecord)
    throw new Error(`${verb}: ${describeNode(record)} is covered at its center (${at.x}, ${at.y}) by ${there}`)
  }
  return at
}

function framesFor(ms: number): number {
  return Math.ceil((ms * frameRate()) / 1000 - FRAME_COUNT_EPSILON)
}

// -- The app --

let mounted = false

let app: TestApp = {
  async mount(ui) {
    if (typeof ui !== "function") throw new TypeError("mount: pass a function that returns the UI, like () => <Counter />")
    if (mounted) throw new Error("mount: this test already mounted its app; a test is one app")
    mounted = true
    let root = 0
    render(() => {
      let content = ui() as { id: number; elementType?: string } | undefined
      if (content && content.elementType === "window") {
        root = content.id
        return content
      }
      let win = createElement("window")
      insert(win, () => content)
      root = win.id
      return win
    })
    // The window builds its first frame on its size, which the engine may
    // not have been told yet.
    await windowReady()
    return fixed(root, "the mounted window")
  },
  async load(entry) {
    if (typeof entry !== "function") throw new TypeError("load: pass a function that imports the entry, like () => import(\"../src/index.tsx\")")
    if (mounted) throw new Error("load: this test already mounted its app; a test is one app")
    mounted = true
    await entry()
    await windowReady()
    let windows = search(null, { kind: "window" })
    if (windows.length === 0) throw new Error("load: the entry rendered no window; its top level has to call render()")
    return fixed(windows[0]!.id, "the app's window")
  },
  async link(target) {
    if (typeof target !== "string" || target === "") throw new TypeError("link: pass the link as a non-empty string")
    if (!link(target)) throw new Error(`link: the app does not listen for links (no onLink handler), so ${JSON.stringify(target)} went nowhere`)
    await app.frame()
  },
  async debug(name, args) {
    let result = debug(name, args === undefined ? null : JSON.stringify(args))
    await app.frame()
    return JSON.parse(result)
  },
  gpu: (options = {}) => JSON.parse(gui.gpu(options.label ?? null, options.draw ?? null)),
  find: query => findIn(() => null, query),
  findAll: query => search(null, query).map(found => fixed(found.id, `${describeQuery(query)} (${describeNode(found)})`)),
  ref() {
    let id: number | null = null
    let ref = (node: { id: number }) => {
      id = node.id
    }
    let resolve = () => {
      if (id === null) throw new Error("This ref names no node yet: put it on an element (ref={it}) and mount that")
      return gui.node(id, 0) === null ? [] : [id]
    }
    return Object.defineProperties(ref, locatorFields(resolve, "the ref", () => null)) as RefLocator
  },
  async frame(count = 1) {
    if (!Number.isInteger(count) || count < 0) throw new TypeError(`frame: the count must be a non-negative integer, got ${count}`)
    for (let i = 0; i < count; i++) await stepFrame()
  },
  painted(target) {
    // A locator is an object, a ref locator the ref function itself.
    let locator = target !== null && (typeof target === "object" || typeof target === "function") && "record" in target
    if (!locator) throw new TypeError("painted: give it a locator, the node whose paint to read")
    return paintFrame(target.record.id)
  },
  advance(ms) {
    if (typeof ms !== "number" || !(ms >= 0) || !Number.isFinite(ms)) throw new TypeError(`advance: the time must be a non-negative number of milliseconds, got ${ms}`)
    return app.frame(framesFor(ms))
  },
  get time() {
    return time()
  },
  settle(options = {}) {
    let maxMs = options.maxMs ?? SETTLE_MAX_MS
    if (typeof maxMs !== "number" || !(maxMs >= 0) || !Number.isFinite(maxMs)) throw new TypeError(`settle: maxMs must be a non-negative number of milliseconds, got ${maxMs}`)
    return settle(maxMs)
  },
  tap(target, options = {}) {
    let at = pointOf(target, "tap")
    return app.input([{ type: "pointer", action: "tap", ...at, ...options }])
  },
  drag(from, to, options = {}) {
    let start = pointOf(from, "drag")
    let end = isLocator(to) ? pointOf(to, "drag") : to
    return app.input([{ type: "pointer", action: "drag", ...start, to: end, ...options }])
  },
  key(key, options = {}) {
    return app.input([{ type: "key", action: "tap", key, ...options }])
  },
  type(text) {
    return app.input([{ type: "text", text }])
  },
  back() {
    return app.input([{ type: "back" }])
  },
  async input(events) {
    let waits = inputPlan(JSON.stringify(events))
    for (let i = 0; i < waits.length; i++) {
      let [ms, frames] = waits[i]!
      await app.frame(Math.max(frames, framesFor(ms)))
      inputStep(i)
    }
    // The frame the last event lands in: the tree is current after it.
    await app.frame()
  },
}

/**
 * Registers an app test. `fn` receives the app under test and passes when
 * it returns, or when the promise it returns fulfills. Flat like the base:
 * no `describe`, no hooks; every test runs in an engine of its own, on the
 * file evaluated afresh, with app time at 0.
 */
export function test(name: string, fn: (app: TestApp) => void | Promise<void>, options: TestOptions = {}): void {
  register(name, () => {
    if (options.fps !== undefined) setFrameRate(options.fps)
    return fn(app)
  })
}
