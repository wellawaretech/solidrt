// Timers, microtasks and performance.now() on the engine loop.
import { expect, test } from "flux:test"

let sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

test("clearTimeout of an unknown id is a no-op", () => {
  // Node and the browser silently ignore an unknown id; flux matches that
  // rather than throwing (an "invalid timer id" throw used to break
  // debounce patterns).
  expect(() => clearTimeout(999)).not.toThrow()
})

test("clearTimeout of undefined is a no-op", () => {
  // The classic `let t; ...; clearTimeout(t)` debounce reset: clearing
  // before a timer is ever set passes undefined. Must no-op, not fail
  // numeric conversion.
  expect(() => clearTimeout(undefined)).not.toThrow()
  expect(() => clearTimeout()).not.toThrow()
})

test("clearTimeout twice is a no-op", () => {
  let id = setTimeout(() => {}, 100000)
  clearTimeout(id)
  expect(() => clearTimeout(id)).not.toThrow()
})

test("clearTimeout after the timer fired is a no-op", async () => {
  // Re-clearing a stale handle after the timer has fired is exactly the
  // debounce case from the LAN-map app; it must not throw.
  await new Promise<void>((resolve) => {
    let id = setTimeout(() => {
      clearTimeout(id)
      resolve()
    }, 10)
  })
})

test("clearTimeout of a timer not yet fired cancels it", async () => {
  let fired = false
  let id = setTimeout(() => {
    fired = true
  }, 20)
  clearTimeout(id)
  await sleep(60)
  expect(fired).toBe(false)
})

test("setTimeout returns a numeric id", () => {
  let id = setTimeout(() => {}, 1)
  expect(typeof id).toBe("number")
  clearTimeout(id)
})

test("setInterval returns a numeric id", () => {
  let id = setInterval(() => {}, 1)
  expect(typeof id).toBe("number")
  clearInterval(id)
})

test("queueMicrotask runs before timers", async () => {
  let order: string[] = []
  setTimeout(() => order.push("timeout"), 0)
  queueMicrotask(() => order.push("microtask"))
  await sleep(50)
  expect(order).toEqual(["microtask", "timeout"])
})

test("setTimeout fires", async () => {
  let fired = false
  await new Promise<void>((resolve) =>
    setTimeout(() => {
      fired = true
      resolve()
    }, 10),
  )
  expect(fired).toBe(true)
})

test("setTimeout chains", async () => {
  let fired = await new Promise<string>((resolve) => {
    setTimeout(() => {
      setTimeout(() => resolve("chained"), 10)
    }, 10)
  })
  expect(fired).toBe("chained")
})

test("a promise resolved by a timer", async () => {
  let p = new Promise((resolve) => setTimeout(() => resolve("ok"), 10))
  expect(await p).toBe("ok")
})

test("multiple concurrent timers fire in order", async () => {
  let results: string[] = []
  setTimeout(() => results.push("a"), 10)
  setTimeout(() => results.push("b"), 20)
  await new Promise<void>((resolve) =>
    setTimeout(() => {
      results.push("c")
      resolve()
    }, 30),
  )
  expect(results).toEqual(["a", "b", "c"])
})

test("a microtask after a timer", async () => {
  let seen = await new Promise<string>((resolve) => {
    setTimeout(() => {
      Promise.resolve().then(() => resolve("microtask"))
    }, 10)
  })
  expect(seen).toBe("microtask")
})

test("a deep promise chain after a timer", async () => {
  let seen = await new Promise<string>((resolve) => {
    setTimeout(() => {
      Promise.resolve("a")
        .then((v) => v + ",b")
        .then((v) => v + ",c")
        .then((v) => v + ",d")
        .then(resolve)
    }, 10)
  })
  expect(seen).toBe("a,b,c,d")
})

test("queueMicrotask after a timer", async () => {
  let seen = await new Promise<string>((resolve) => {
    setTimeout(() => {
      queueMicrotask(() => {
        queueMicrotask(() => resolve("nested microtask"))
      })
    }, 10)
  })
  expect(seen).toBe("nested microtask")
})

test("a microtask updates state a later timer reads", async () => {
  let state = "initial"
  let seen = await new Promise<string>((resolve) => {
    setTimeout(() => {
      Promise.resolve().then(() => {
        state = "updated"
      })
      setTimeout(() => resolve(state), 50)
    }, 10)
  })
  expect(seen).toBe("updated")
})

test("async/await after a timer", async () => {
  async function work() {
    let result = await new Promise<string>((resolve) => setTimeout(() => resolve("step1"), 10))
    result = await Promise.resolve(result + ",step2")
    result = await Promise.resolve(result + ",step3")
    return result
  }
  expect(await work()).toBe("step1,step2,step3")
})

// ----- performance.now() -----

test("performance.now returns a number", () => {
  expect(typeof performance.now()).toBe("number")
})

test("performance.now is monotonic", () => {
  let a = performance.now()
  let b = performance.now()
  expect(b).toBeGreaterThanOrEqual(a)
})

test("performance.now advances after a timeout", async () => {
  let start = performance.now()
  await sleep(20)
  expect(performance.now()).toBeGreaterThan(start)
})

test("performance.timeOrigin tracks the wall clock", () => {
  // timeOrigin is the wall-clock ms of the process origin, so
  // timeOrigin + now() tracks Date.now() like the browser.
  expect(Math.abs(performance.timeOrigin + performance.now() - Date.now())).toBeLessThan(1000)
})
