// The JavaScript half of flux:test: the registry behind `test`, the `expect`
// matchers, the stepped clock, the seed of `Math.random` and `run`. Evaluated once per context by the
// module definition (mod.rs), which calls this function with the natives it
// needs the engine for and exports what it returns. Plain JS on purpose:
// the flux build has no bundling step (okf/plans/test-harness.md, D21).
(native) => {
  // How long one test may take before it fails as timed out, when `run` is
  // given no `timeoutMs`. A safety cap against a test that never finishes,
  // not a wait: a test that finishes is never held to it.
  const DEFAULT_TIMEOUT_MS = 5000
  // `toBeCloseTo` without `digits`: equal to two decimal places.
  const DEFAULT_CLOSE_DIGITS = 2
  // How deep a value is printed in a failure message before it is cut to
  // its kind, and how many entries of one collection are printed before
  // the rest is counted. Keeps the message of a large buffer readable.
  const FORMAT_MAX_DEPTH = 6
  const FORMAT_MAX_ITEMS = 50
  // How many task-queue turns `clock.advance` takes at one virtual instant
  // before it gives up: timers that keep re-arming with no delay would
  // otherwise hold time still forever.
  const CLOCK_MAX_TURNS_PER_INSTANT = 1000
  // The seed `Math.random` runs on when `run` is given none. Any fixed
  // number does: what matters is that it is the same on every run.
  const DEFAULT_SEED = 0

  // Seeded from here on, so that what a test file draws while it loads is
  // reproducible too; each test then restarts the sequence (see runOne).
  native.seedRandom(DEFAULT_SEED)

  let tests = []
  let running = false

  function test(name, fn) {
    if (typeof name !== "string" || name === "") throw new TypeError("test: the name must be a non-empty string")
    if (typeof fn !== "function") throw new TypeError(`test: "${name}" needs a function`)
    if (running) throw new Error(`test: "${name}" is registered while tests run; register tests at the top level`)
    if (tests.some((entry) => entry.name === name)) throw new Error(`test: "${name}" is registered twice`)
    tests.push({ name, fn })
  }

  // -- Printing a value --

  function tagOf(value) {
    return Object.prototype.toString.call(value)
  }

  function isTypedArray(value) {
    return ArrayBuffer.isView(value) && !(value instanceof DataView)
  }

  function formatItems(items, total, open, close) {
    if (total > items.length) items.push(`... ${total - items.length} more`)
    return items.length === 0 ? open + close : `${open}${items.join(", ")}${close}`
  }

  function format(value, depth = 0, seen = []) {
    switch (typeof value) {
      case "string":
        return JSON.stringify(value)
      case "number":
        return Object.is(value, -0) ? "-0" : String(value)
      case "bigint":
        return `${value}n`
      case "symbol":
        return value.toString()
      case "function":
        return `[Function ${value.name || "anonymous"}]`
      case "object":
        break
      default:
        return String(value)
    }
    if (value === null) return "null"
    if (seen.includes(value)) return "[Circular]"
    if (value instanceof Error) return `[${value.name}: ${value.message}]`
    if (value instanceof Date) return Number.isNaN(value.getTime()) ? "Invalid Date" : value.toISOString()
    if (value instanceof RegExp) return String(value)
    let name = value.constructor?.name
    if (depth >= FORMAT_MAX_DEPTH) return `[${name ?? "Object"}]`
    let inner = (item) => format(item, depth + 1, [...seen, value])
    if (value instanceof ArrayBuffer) value = new Uint8Array(value)
    if (value instanceof DataView) value = new Uint8Array(value.buffer, value.byteOffset, value.byteLength)
    if (Array.isArray(value) || isTypedArray(value)) {
      let items = []
      for (let i = 0; i < value.length && i < FORMAT_MAX_ITEMS; i++) items.push(inner(value[i]))
      let prefix = Array.isArray(value) ? "" : `${name} `
      return formatItems(items, value.length, `${prefix}[`, "]")
    }
    if (value instanceof Map) {
      let items = []
      for (let [key, item] of value) {
        if (items.length === FORMAT_MAX_ITEMS) break
        items.push(`${inner(key)} => ${inner(item)}`)
      }
      return formatItems(items, value.size, "Map {", "}")
    }
    if (value instanceof Set) {
      let items = []
      for (let item of value) {
        if (items.length === FORMAT_MAX_ITEMS) break
        items.push(inner(item))
      }
      return formatItems(items, value.size, "Set {", "}")
    }
    let keys = Object.keys(value)
    let items = keys.slice(0, FORMAT_MAX_ITEMS).map((key) => `${key}: ${inner(value[key])}`)
    let prefix = name && name !== "Object" ? `${name} ` : ""
    return formatItems(items, keys.length, `${prefix}{`, "}")
  }

  // -- Comparing values --

  function definedKeys(object) {
    return Object.keys(object).filter((key) => object[key] !== undefined)
  }

  // `pairs` holds the object pairs being compared further up, so a cycle
  // ends where it closes instead of recursing forever.
  function equals(a, b, pairs = []) {
    if (Object.is(a, b)) return true
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false
    let tag = tagOf(a)
    if (tag !== tagOf(b)) return false
    for (let [left, right] of pairs) {
      if (left === a) return right === b
    }
    let inner = (left, right) => equals(left, right, [...pairs, [a, b]])
    switch (tag) {
      case "[object Date]":
        return Object.is(a.getTime(), b.getTime())
      case "[object RegExp]":
        return a.source === b.source && a.flags === b.flags
      case "[object Error]":
        return a.name === b.name && a.message === b.message
      case "[object Boolean]":
      case "[object Number]":
      case "[object String]":
        return Object.is(a.valueOf(), b.valueOf())
      case "[object ArrayBuffer]":
        return inner(new Uint8Array(a), new Uint8Array(b))
      case "[object DataView]":
        return inner(
          new Uint8Array(a.buffer, a.byteOffset, a.byteLength),
          new Uint8Array(b.buffer, b.byteOffset, b.byteLength),
        )
      case "[object Map]":
        if (a.size !== b.size) return false
        for (let [key, item] of a) {
          if (b.has(key)) {
            if (!inner(item, b.get(key))) return false
          } else if (![...b].some(([otherKey, other]) => inner(key, otherKey) && inner(item, other))) {
            return false
          }
        }
        return true
      case "[object Set]":
        if (a.size !== b.size) return false
        for (let item of a) {
          if (!b.has(item) && ![...b].some((other) => inner(item, other))) return false
        }
        return true
    }
    if (Array.isArray(a) || isTypedArray(a)) {
      if (a.length !== b.length) return false
      for (let i = 0; i < a.length; i++) {
        if (!inner(a[i], b[i])) return false
      }
      return true
    }
    let keys = definedKeys(a)
    if (keys.length !== definedKeys(b).length) return false
    return keys.every((key) => Object.hasOwn(b, key) && inner(a[key], b[key]))
  }

  // Whether `received` carries everything `pattern` names: a plain object
  // in the pattern is matched property by property, recursively, and
  // anything else has to be equal.
  function matches(received, pattern) {
    if (Array.isArray(pattern)) {
      if (!Array.isArray(received) || received.length !== pattern.length) return false
      return pattern.every((item, i) => matches(received[i], item))
    }
    if (tagOf(pattern) === "[object Object]" && typeof received === "object" && received !== null) {
      return Object.keys(pattern).every((key) => key in received && matches(received[key], pattern[key]))
    }
    return equals(received, pattern)
  }

  // -- Matchers --

  // What a failed expectation throws. `run` reports its message and stack
  // like any other error's.
  class AssertionError extends Error {
    constructor(message) {
      super(message)
      this.name = "AssertionError"
    }
  }

  function messageOf(thrown) {
    return thrown !== null && typeof thrown === "object" && typeof thrown.message === "string"
      ? thrown.message
      : String(thrown)
  }

  function needNumber(matcher, what, value) {
    if (typeof value !== "number" && typeof value !== "bigint") {
      throw new TypeError(`${matcher}: the ${what} value must be a number, got ${format(value)}`)
    }
  }

  function ordering(matcher, sign, holds) {
    return (received, expected) => {
      needNumber(matcher, "received", received)
      needNumber(matcher, "expected", expected)
      return { pass: holds(received, expected), expected: `${sign} ${format(expected)}` }
    }
  }

  // Each matcher returns whether it holds, what it expected in words, and
  // (where the received value alone is not the useful half) what it got.
  const MATCHERS = {
    toBe: (received, expected) => ({ pass: Object.is(received, expected), expected: format(expected) }),

    toEqual: (received, expected) => ({ pass: equals(received, expected), expected: format(expected) }),

    toBeCloseTo(received, expected, digits = DEFAULT_CLOSE_DIGITS) {
      if (typeof received !== "number") throw new TypeError(`toBeCloseTo: the received value must be a number, got ${format(received)}`)
      if (typeof expected !== "number") throw new TypeError(`toBeCloseTo: the expected value must be a number, got ${format(expected)}`)
      if (!Number.isInteger(digits) || digits < 0) throw new TypeError(`toBeCloseTo: digits must be a non-negative integer, got ${format(digits)}`)
      let tolerance = 10 ** -digits / 2
      // The equality test is what lets an infinity be close to itself.
      let pass = received === expected || Math.abs(expected - received) < tolerance
      return { pass, expected: `${format(expected)} (within ${tolerance})` }
    },

    toBeNull: (received) => ({ pass: received === null, expected: "null" }),

    toContain(received, item) {
      if (typeof received === "string") {
        if (typeof item !== "string") throw new TypeError(`toContain: a string contains strings only, got ${format(item)}`)
        return { pass: received.includes(item), expected: `a string containing ${format(item)}` }
      }
      if (received === null || received === undefined || typeof received[Symbol.iterator] !== "function") {
        throw new TypeError(`toContain: the received value must be a string or iterable, got ${format(received)}`)
      }
      return { pass: [...received].some((entry) => entry === item), expected: `a collection containing ${format(item)}` }
    },

    toMatchObject(received, pattern) {
      if (typeof pattern !== "object" || pattern === null) throw new TypeError(`toMatchObject: the pattern must be an object, got ${format(pattern)}`)
      return { pass: matches(received, pattern), expected: `an object matching ${format(pattern)}` }
    },

    toBeLessThan: ordering("toBeLessThan", "<", (a, b) => a < b),
    toBeLessThanOrEqual: ordering("toBeLessThanOrEqual", "<=", (a, b) => a <= b),
    toBeGreaterThan: ordering("toBeGreaterThan", ">", (a, b) => a > b),
    toBeGreaterThanOrEqual: ordering("toBeGreaterThanOrEqual", ">=", (a, b) => a >= b),

    toThrow(received, expected) {
      if (typeof received !== "function") throw new TypeError(`toThrow: the received value must be a function, got ${format(received)}`)
      let wanted
      let check
      if (expected === undefined) {
        wanted = "a thrown error"
        check = () => true
      } else if (typeof expected === "string") {
        wanted = `an error whose message contains ${format(expected)}`
        check = (thrown) => messageOf(thrown).includes(expected)
      } else if (expected instanceof RegExp) {
        wanted = `an error whose message matches ${expected}`
        check = (thrown) => expected.test(messageOf(thrown))
      } else if (typeof expected === "function") {
        wanted = `an instance of ${expected.name || "the given class"}`
        check = (thrown) => thrown instanceof expected
      } else {
        throw new TypeError(`toThrow: expected a message part, a pattern, an error class or nothing, got ${format(expected)}`)
      }
      try {
        received()
      } catch (thrown) {
        return { pass: check(thrown), expected: wanted, received: format(thrown) }
      }
      return { pass: false, expected: wanted, received: "the function did not throw" }
    },
  }

  class Expectation {
    #received
    #negated

    constructor(received, negated) {
      this.#received = received
      this.#negated = negated
    }

    get not() {
      return new Expectation(this.#received, !this.#negated)
    }

    static check(expectation, name, args) {
      let received = expectation.#received
      let negated = expectation.#negated
      let outcome = MATCHERS[name](received, ...args)
      if (outcome.pass !== negated) return
      throw new AssertionError(
        `expect(received)${negated ? ".not" : ""}.${name}(${args.length > 0 ? "expected" : ""})\n` +
          `Expected: ${negated ? "not " : ""}${outcome.expected}\n` +
          `Received: ${outcome.received ?? format(received)}`,
      )
    }
  }

  for (let name of Object.keys(MATCHERS)) {
    Expectation.prototype[name] = function (...args) {
      Expectation.check(this, name, args)
    }
  }

  function expect(received) {
    return new Expectation(received, false)
  }

  // -- The stepped clock --

  // The clock of one stepped test. Virtual time is installed for the
  // test's whole body (the caller does that before the body runs) and
  // moves only through `advance`. `end` cuts an advance still walking when
  // the test is over, a timed-out one's included.
  function steppedClock() {
    let live = true
    let alive = (verb) => {
      if (!live) throw new Error(`clock.${verb}: the test this clock belongs to has ended`)
    }
    let clock = {
      get now() {
        alive("now")
        return native.clockNow()
      },
      async advance(ms) {
        alive("advance")
        if (typeof ms !== "number" || !Number.isFinite(ms) || ms < 0) {
          throw new TypeError(`clock.advance: the time must be a non-negative number of milliseconds, got ${format(ms)}`)
        }
        let target = native.clockNow() + ms
        let turns = 0
        // Deadline by deadline, not in one jump: each stop is a task-queue
        // turn of its own, so a timer sees the time it was set for, an
        // interval fires as often as fits, and what one timer starts has
        // run before the next fires.
        while (true) {
          let next = native.nextDeadline()
          if (next === undefined || next > target) break
          turns = next > native.clockNow() ? 0 : turns + 1
          if (turns > CLOCK_MAX_TURNS_PER_INSTANT) {
            throw new Error(`clock.advance: timers keep re-arming at ${next} ms without letting time pass`)
          }
          native.advanceTo(next)
          await native.turn()
          alive("advance")
        }
        native.advanceTo(target)
        await native.turn()
      },
    }
    return {
      clock,
      end() {
        live = false
      },
    }
  }

  // -- Running --

  function describeError(thrown) {
    if (thrown instanceof Error) return { message: `${thrown.name}: ${thrown.message}`, stack: thrown.stack ?? "" }
    return { message: `Thrown: ${format(thrown)}`, stack: "" }
  }

  async function runOne(entry, timeoutMs, seed) {
    // Every test draws the seed's sequence from its start, so what it draws
    // does not depend on the tests before it, or on a filter.
    native.seedRandom(seed)
    // A test that takes the clock runs stepped: its timers are virtual from
    // before its body runs, since a timer registered on the wall clock
    // stays there.
    let stepped = entry.fn.length > 0 ? steppedClock() : undefined
    if (stepped) native.installClock()
    let start = performance.now()
    let timer
    // The cap rides the wall clock whatever the test does to time. It is
    // also what keeps the engine alive while a test waits on a promise that
    // never settles: without it the process would end cleanly with the test
    // unreported.
    let timeout = new Promise((resolve) => {
      timer = native.wallTimeout(() => resolve({ message: `Timed out after ${timeoutMs} ms`, stack: "" }), timeoutMs)
    })
    let outcome = Promise.resolve()
      .then(() => entry.fn(stepped?.clock))
      .then(
        () => undefined,
        (thrown) => describeError(thrown),
      )
    let error = await Promise.race([outcome, timeout])
    clearTimeout(timer)
    if (stepped) {
      stepped.end()
      native.uninstallClock()
    }
    let durationMs = performance.now() - start
    return error ? { name: entry.name, ok: false, durationMs, error } : { name: entry.name, ok: true, durationMs }
  }

  async function* iterate(filter, timeoutMs, seed) {
    if (running) throw new Error("run: a run is already in progress")
    running = true
    try {
      for (let entry of [...tests]) {
        if (filter !== undefined && !entry.name.includes(filter)) continue
        yield await runOne(entry, timeoutMs, seed)
      }
    } finally {
      running = false
    }
  }

  function run(options = {}) {
    if (options === null || typeof options !== "object") throw new TypeError("run: the options must be an object")
    let { filter, timeoutMs = DEFAULT_TIMEOUT_MS, seed = DEFAULT_SEED } = options
    if (filter !== undefined && typeof filter !== "string") throw new TypeError("run: filter must be a string")
    if (typeof timeoutMs !== "number" || !(timeoutMs > 0) || !Number.isFinite(timeoutMs)) {
      throw new TypeError("run: timeoutMs must be a positive number")
    }
    if (!Number.isSafeInteger(seed) || seed < 0) throw new TypeError("run: seed must be a non-negative integer")
    return iterate(filter, timeoutMs, seed)
  }

  return { test, expect, run }
}
