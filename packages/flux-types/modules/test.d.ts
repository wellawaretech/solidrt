/**
 * Tests for a flux program: `test` registers one, `expect` asserts inside
 * it, and `run` runs what was registered. `srt test` is the usual way to
 * run a test file; it calls `run` and reports.
 *
 * The names are the familiar ones and the semantics are simplified. Tests
 * are flat: there is no `describe` and there are no hooks. The file is the
 * group, the test name is a sentence that names its subject, and shared
 * setup is a plain function a test calls. A test that takes the clock
 * steps time itself, so timer logic is tested without waiting.
 *
 * Present on the `flux` binary only (capability `"test"`), not in a
 * shipping runtime.
 *
 * @example
 * import { test, expect } from "flux:test"
 *
 * test("matchPath: a param segment matches any value", () => {
 *   expect(matchPath("/users/$id", "/users/7")).toEqual({ id: "7" })
 * })
 */
declare module "flux:test" {
  /**
   * Registers a test. `fn` passes when it returns, or when the promise it
   * returns fulfills, and fails when it throws or the promise rejects.
   * Tests run one after another, in the order they were registered.
   *
   * A test runs on real time, unless its function takes the clock: then
   * it runs stepped (see {@link Clock}).
   *
   * Throws when the name is empty or already taken in this file, and when
   * called while tests run: register at the top level of the file.
   *
   * @example
   * test("a session expires after its timeout", async clock => {
   *   let session = createSession()
   *   await clock.advance(SESSION_TIMEOUT_MS)
   *   expect(session.expired).toBe(true)
   * })
   */
  export function test(name: string, fn: (clock: Clock) => void | Promise<void>): void

  /**
   * The clock of a stepped test, which is a test whose function declares
   * the parameter. For that test `setTimeout` and `setInterval` are on a
   * virtual timeline that starts at 0 and moves only when the test says
   * so: nothing fires by itself, and nothing waits on the wall clock. The
   * timeline is in place before the test's body runs, so a timer the
   * code under test registers is on it, and timers still waiting when the
   * test ends are dropped.
   *
   * Only timers are stepped. `performance.now()` and `Date.now()` stay
   * real time, and so does everything outside the process: a socket, a
   * subprocess or a file read completes when it completes. How long the
   * test may take is still measured on the wall clock.
   */
  export interface Clock {
    /** The virtual time in milliseconds since the test started. */
    readonly now: number
    /**
     * Moves the virtual time forward by `ms` and fires the timers that
     * come due, each at its own time and in order: an interval fires as
     * often as fits, and a timer registered by a fired callback fires too
     * when it falls inside the span. Everything a callback sets in motion
     * that needs no further time (a chain of promises) has run before the
     * next timer fires and before the returned promise resolves.
     *
     * Throws when timers keep re-arming with no delay, which would hold
     * time still.
     */
    advance(ms: number): Promise<void>
  }

  /**
   * Starts an assertion on `received`. Each matcher throws when it does not
   * hold, which fails the test; the message names the matcher and prints
   * the expected and the received value.
   */
  export function expect(received: unknown): Matchers

  export interface Matchers {
    /** The same matchers, inverted: each throws when it does hold. */
    not: Matchers

    /** The same value, as `Object.is` sees it: `NaN` is `NaN`, `0` is not `-0`, and two equal objects are not the same. */
    toBe(expected: unknown): void

    /**
     * Equal in value, recursively. Objects are compared by their own
     * enumerable properties, and a property whose value is `undefined`
     * counts as absent. Arrays and typed arrays are compared element by
     * element, and a typed array equals neither a plain array nor a typed
     * array of another kind. `ArrayBuffer`s are compared byte by byte,
     * `Map`s and `Set`s by their entries, `Date`s by their time, errors by
     * name and message. Leaves are compared as `toBe` does.
     */
    toEqual(expected: unknown): void

    /**
     * A number within half a unit of the last of `digits` decimal places
     * (default 2): the difference is below `10 ** -digits / 2`. The way to
     * compare floating point results.
     */
    toBeCloseTo(expected: number, digits?: number): void

    /**
     * For a function: calling it throws. With a string, the thrown error's
     * message has to contain it; with a pattern, to match it; with an error
     * class, the thrown value has to be an instance of it.
     */
    toThrow(expected?: string | RegExp | (new (...args: any[]) => unknown)): void

    /** Exactly `null`; `undefined` is not. */
    toBeNull(): void

    /**
     * A string that contains the part `item`, or an array (or any other
     * iterable) with an entry that is `item` by `===`.
     */
    toContain(item: unknown): void

    /**
     * An object that carries everything `pattern` names: each property of
     * the pattern has to be present and, for a nested plain object, match
     * in turn; anything else has to be equal as `toEqual` sees it.
     * Properties the pattern does not name are ignored. An array in the
     * pattern is matched entry by entry, at the same length.
     */
    toMatchObject(pattern: object): void

    toBeLessThan(expected: number | bigint): void
    toBeLessThanOrEqual(expected: number | bigint): void
    toBeGreaterThan(expected: number | bigint): void
    toBeGreaterThanOrEqual(expected: number | bigint): void
  }

  /** What one test came to. */
  export interface TestResult {
    name: string
    ok: boolean
    /** How long the test took, in milliseconds of real time. */
    durationMs: number
    /** Set when the test failed. */
    error?: {
      /**
       * The thrown error's name and message, `"Thrown: <value>"` for a
       * thrown value that is no error, or `"Timed out after <n> ms"`.
       */
      message: string
      /** The thrown error's stack; empty for a timeout or a non-error. */
      stack: string
    }
  }

  export interface RunOptions {
    /** Run only the tests whose name contains this text. */
    filter?: string
    /**
     * How long one test may take, in milliseconds of real time (default
     * 5000), stepped or not. A test that takes longer fails as timed out
     * and the run moves on; what the test started is not cancelled. A
     * safety cap against a test that never finishes, not a wait.
     */
    timeoutMs?: number
  }

  /**
   * Runs the registered tests and yields each one's result as it finishes.
   * A test starts when its result is asked for, so breaking out of the
   * loop stops the run. One run at a time.
   *
   * @example
   * import { run } from "flux:test"
   *
   * for await (let result of run({ filter: "matchPath" })) {
   *   console.log(result.ok ? "ok" : "FAILED", result.name)
   * }
   */
  export function run(options?: RunOptions): AsyncIterableIterator<TestResult>
}
