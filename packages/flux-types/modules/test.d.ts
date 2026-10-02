/**
 * Tests for a flux program: `test` registers one and `expect` asserts
 * inside it. A test file is run by a test host, `srt test <file>` (the
 * `flux` binary's `--test` mode underneath); imported anywhere else the
 * module throws, since nothing would run the tests.
 *
 * Every test runs in an engine of its own. The host evaluates the file
 * once to list its tests and once more for each test, so a test starts
 * from the file's freshly evaluated state: a variable, a listener or a
 * timer of one test is not there for the next, a test's result does not
 * depend on the tests before it, and an uncaught error (a throw in a timer
 * callback, a rejection nobody handles) fails the test it happened in. The
 * file's top level therefore runs once per test, plus once for the
 * listing; keep it to registering tests and cheap setup.
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
 * `Math.random()` is seeded in a test's engine, from before the file is
 * evaluated: random inputs, and code under test that calls
 * `Math.random()`, are the same on every run and do not depend on the
 * tests that ran before. It is the engine's generator in kind and
 * resolution, only its start is fixed. An isolate the test spawns is
 * seeded too, with a seed derived from the test's. `srt test --seed <n>`
 * runs the tests on another sequence.
 *
 * A test that does not finish within 5 seconds of real time fails as timed
 * out, a synchronous loop included, and a test that waits on a promise
 * nothing will settle fails at once. Either way its engine is dropped with
 * whatever it left running. A safety cap, not a wait.
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
   * Tests run one after another, in the order they were registered. The
   * file has to register the same tests every time it is evaluated.
   *
   * Throws when the name is empty or already taken in this file, and when
   * called while a test runs: register at the top level of the file.
   */
  export function test(name: string, fn: () => void | Promise<void>): void

  /**
   * Starts an assertion on `received`. Each matcher throws when it does not
   * hold, which fails the test; the message names the matcher and prints
   * the expected and the received value.
   */
  export function expect(received: unknown): Matchers

  /**
   * Fulfills once nothing the test started is still in flight: work that
   * completes by itself (a fetch, a body or file read, a query, a connect,
   * a call into an isolate) has landed, and what it woke has run and
   * started nothing more. For work a test set off without holding its
   * promise; awaiting the promise itself is the direct way when there is
   * one.
   *
   * What stands is not waited for: a listening server, an open socket or a
   * read waiting on its peer, a running child process, an event listener,
   * a timer. A test that wants a timer to fire waits for it itself.
   *
   * @example
   * save(record) // writes in the background
   * await settle()
   * expect(await file(path).text()).toBe(expected)
   */
  export function settle(): Promise<void>

  export interface Matchers {
    /** The same matchers, inverted: each throws when it does hold. */
    not: Matchers

    /**
     * For a promise: the same matchers on its rejection reason, each
     * awaiting the promise first and returning a promise to await in turn.
     * A promise that fulfills fails the matcher, with or without `not`.
     * `toThrow` sees the reason as the thrown value, so a rejection reads
     * like a throw: `await expect(p).rejects.toThrow("no such file")`.
     */
    rejects: RejectionMatchers

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

  /** The matchers of `expect(promise).rejects`: `Matchers`, each returning a promise. */
  export type RejectionMatchers = {
    [K in Exclude<keyof Matchers, "rejects">]: Matchers[K] extends (...args: infer A) => void
      ? (...args: A) => Promise<void>
      : RejectionMatchers
  }
}
