// `fail` (src/lib/fail.ts) throws rather than exits, so a rule that calls it
// is testable where it lives; main.ts turns the throw into the message and
// exit code 1.
import { test, expect } from "flux:test"
import { fail, UsageError } from "../src/lib/fail.ts"

test("fail throws a UsageError carrying the message", () => {
  let caught: unknown = null
  try {
    fail("the message")
  } catch (e) {
    caught = e
  }
  expect(caught instanceof UsageError).toBe(true)
  expect((caught as Error).message).toBe("the message")
})
