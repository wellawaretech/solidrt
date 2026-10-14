// Tests for the resource pool (src/pool.ts): a take misses on an empty
// class and hands back the most recently given id of its class, classes
// never mix, the cap frees the oldest held id of any class through the
// destroyer registered for its kind, drain frees everything oldest first
// and leaves the pool usable, a cap of 0 frees at once, a kind's
// destroyer is registered once per pool and a give under an unregistered
// kind throws, and the options are validated.
// Pure-module input only, so it runs headless on flux: `sol test
// packages/core`.

import { test, expect } from "flux:test"
import { createResourcePool } from "../src/pool.ts"

/** A pool of `cap` ids with kinds `a` and `b` registered, recording what
 * it frees as `[id, kind]` pairs. */
function pool(cap: number): { freed: [number, string][]; take: (cls: string) => number | undefined; give: (cls: string, id: number) => void; drain: () => void; size: () => number } {
  let freed: [number, string][] = []
  let p = createResourcePool({ cap })
  p.destroyWith("a", id => freed.push([id, "a"]))
  p.destroyWith("b", id => freed.push([id, "b"]))
  return { freed, take: p.take, give: p.give, drain: p.drain, size: () => p.size }
}

test("a take misses on an empty class and returns the most recently given id of its class", () => {
  let p = pool(8)
  expect(p.take("a")).toBe(undefined)
  p.give("a", 1)
  p.give("a", 2)
  p.give("b", 10)
  expect(p.take("a")).toBe(2)
  expect(p.take("a")).toBe(1)
  expect(p.take("a")).toBe(undefined)
  expect(p.take("b")).toBe(10)
  expect(p.size()).toBe(0)
  expect(p.freed).toEqual([])
})

test("past the cap the oldest held id is freed, of whatever class, and a freed id is never handed out", () => {
  let p = pool(2)
  p.give("a", 1)
  p.give("b:16", 2)
  p.give("a", 3)
  expect(p.freed).toEqual([[1, "a"]])
  expect(p.size()).toBe(2)
  // A take re-opens a slot: nothing is freed by the next give. The
  // detail after the kind is part of the class.
  expect(p.take("b")).toBe(undefined)
  expect(p.take("b:16")).toBe(2)
  p.give("a", 4)
  expect(p.freed).toEqual([[1, "a"]])
  expect(p.take("a")).toBe(4)
  expect(p.take("a")).toBe(3)
  expect(p.take("a")).toBe(undefined)
})

test("drain frees everything oldest first and the pool stays usable", () => {
  let p = pool(8)
  p.give("a", 1)
  p.give("b", 2)
  p.give("a", 3)
  p.drain()
  expect(p.freed).toEqual([[1, "a"], [2, "b"], [3, "a"]])
  expect(p.size()).toBe(0)
  expect(p.take("a")).toBe(undefined)
  p.give("a", 5)
  expect(p.take("a")).toBe(5)
})

test("a cap of 0 frees every given id at once", () => {
  let p = pool(0)
  p.give("a", 7)
  expect(p.freed).toEqual([[7, "a"]])
  expect(p.size()).toBe(0)
  expect(p.take("a")).toBe(undefined)
})

test("a kind's destroyer is registered once per pool, the same function again is a no-op, and a give under an unregistered kind throws", () => {
  let p = createResourcePool({ cap: 4 })
  let freed: number[] = []
  let destroy = (id: number) => freed.push(id)
  p.destroyWith("tex", destroy)
  p.destroyWith("tex", destroy)
  expect(() => p.destroyWith("tex", id => freed.push(-id))).toThrow(/another consumer/)
  expect(() => p.give("buf:8", 1)).toThrow(/destroyWith/)
  expect(p.size).toBe(0)
  p.give("tex:3", 7)
  p.drain()
  expect(freed).toEqual([7])
  expect(() => p.destroyWith("buf", null as never)).toThrow()
})

test("the options are validated", () => {
  expect(() => createResourcePool({ cap: -1 })).toThrow()
  expect(() => createResourcePool({ cap: 1.5 })).toThrow()
})
