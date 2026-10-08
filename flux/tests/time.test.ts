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

// ----- setImmediate -----

test("setImmediate runs after the current job and its microtasks", async () => {
  let order: string[] = []
  await new Promise<void>((resolve) => {
    setImmediate(() => {
      order.push("immediate")
      resolve()
    })
    queueMicrotask(() => order.push("microtask"))
    order.push("sync")
  })
  expect(order).toEqual(["sync", "microtask", "immediate"])
})

test("setImmediate returns a numeric id", () => {
  let id = setImmediate(() => {})
  expect(typeof id).toBe("number")
  clearImmediate(id)
})

test("clearImmediate cancels a pending immediate", async () => {
  let fired = false
  let id = setImmediate(() => {
    fired = true
  })
  clearImmediate(id)
  await sleep(20)
  expect(fired).toBe(false)
})

test("clearImmediate of an unknown or missing id is a no-op", () => {
  expect(() => clearImmediate(999)).not.toThrow()
  expect(() => clearImmediate()).not.toThrow()
})

test("a chain of immediates makes progress at CPU speed", async () => {
  // The chunking use: each step yields to the loop and comes straight back,
  // with no delay floor between steps.
  let start = performance.now()
  let steps = 0
  await new Promise<void>((resolve) => {
    function step() {
      if (++steps < 1000) setImmediate(step)
      else resolve()
    }
    setImmediate(step)
  })
  expect(steps).toBe(1000)
  expect(performance.now() - start).toBeLessThan(1000)
})

// ----- requestIdleCallback -----
// Headless flux has no frames to be idle between: the idle period is the
// next turn, with the full budget.

test("requestIdleCallback runs with an idle deadline", async () => {
  let deadline = await new Promise<IdleDeadline>((resolve) => requestIdleCallback(resolve))
  expect(deadline.didTimeout).toBe(false)
  let remaining = deadline.timeRemaining()
  expect(remaining).toBeGreaterThan(0)
  expect(remaining).toBeLessThanOrEqual(50)
})

test("requestIdleCallback runs after the current job and its microtasks", async () => {
  let order: string[] = []
  await new Promise<void>((resolve) => {
    requestIdleCallback(() => {
      order.push("idle")
      resolve()
    })
    queueMicrotask(() => order.push("microtask"))
    order.push("sync")
  })
  expect(order).toEqual(["sync", "microtask", "idle"])
})

test("idle callbacks run in registration order", async () => {
  let order: number[] = []
  await new Promise<void>((resolve) => {
    requestIdleCallback(() => order.push(1))
    requestIdleCallback(() => order.push(2))
    requestIdleCallback(() => {
      order.push(3)
      resolve()
    })
  })
  expect(order).toEqual([1, 2, 3])
})

test("cancelIdleCallback cancels a pending idle callback", async () => {
  let fired = false
  let id = requestIdleCallback(() => {
    fired = true
  })
  cancelIdleCallback(id)
  await sleep(20)
  expect(fired).toBe(false)
})

test("cancelIdleCallback of an unknown or missing id is a no-op", () => {
  expect(() => cancelIdleCallback(999)).not.toThrow()
  expect(() => cancelIdleCallback()).not.toThrow()
})

test("an idle callback with a timeout runs once, from the idle period", async () => {
  // The period comes first here; the timeout must then be dropped rather
  // than run the callback a second time.
  let runs = 0
  let didTimeout: boolean | undefined
  requestIdleCallback(
    (deadline) => {
      runs++
      didTimeout = deadline.didTimeout
    },
    { timeout: 5 },
  )
  await sleep(40)
  expect(runs).toBe(1)
  expect(didTimeout).toBe(false)
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
