// Promises and async functions on the engine's job queue.
import { expect, test } from "flux:test"

test("Promise.resolve", async () => {
  expect(await Promise.resolve("resolved")).toBe("resolved")
})

test("a then chain", async () => {
  expect(
    await Promise.resolve(1)
      .then((v) => v + 1)
      .then((v) => v * 3),
  ).toBe(6)
})

test("catch", async () => {
  expect(await Promise.reject(new Error("boom")).catch((e: Error) => e.message)).toBe("boom")
})

test("Promise.all", async () => {
  expect(await Promise.all([Promise.resolve("a"), Promise.resolve("b"), Promise.resolve("c")])).toEqual(["a", "b", "c"])
})

test("an async function", async () => {
  let a = await Promise.resolve("hello")
  let b = await Promise.resolve(" world")
  expect(a + b).toBe("hello world")
})
