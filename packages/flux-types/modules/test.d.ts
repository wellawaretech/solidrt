/**
 * Tests for a flux program: `test` registers one, `expect` asserts inside
 * it, and `run` runs what was registered. `srt test` is the usual way to
 * run a test file; it calls `run` and reports.
 *
 * The names are the familiar ones and the semantics are simplified. Tests
 * are flat: there is no `describe` and there are no hooks. The file is the
 * group, the test name is a sentence that names its subject, and shared
 * setup is a plain function a test calls.
 *
 * A test runs on real time: timers, `performance.now()` and `Date.now()`
 * are the wall clock's, as they are for the program under test and for
 * whatever is on the other side of its sockets. Logic with a timeout is
 * tested by waiting for it, or takes its delay as a parameter.
 *
 * `Math.random()` is seeded once this module is imported, and every test
 * draws the seed's sequence from its start: random inputs, and code under
 * test that calls `Math.random()`, are the same on every run and do not
 * depend on the tests that ran before. It is the engine's generator in
 * kind and resolution, only its start is fixed. An isolate the test spawns
 * is seeded too, with a seed derived from the test's.
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
   * Throws when the name is empty or already taken in this file, and when
   * called while tests run: register at the top level of the file.
   */
  export function test(name: string, fn: () => void | Promise<void>): void

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
     * The seed `Math.random()` starts from in every test, a non-negative
     * integer (default 0). A seed names one sequence, the same on every
     * run and platform; another seed is how the same tests try other
     * random inputs.
     */
    seed?: number
    /**
     * How long one test may take, in milliseconds of real time (default
     * 5000). A test that takes longer fails as timed out
     * and the run moves on. What the test started is not cancelled: it
     * keeps running beside the tests after it and can disturb them (a
     * timer it sets from then on belongs to whichever test runs at that
     * moment), so fix a timed-out test before trusting the results that
     * follow it in the same file. A safety cap against a test that never
     * finishes, not a wait.
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
