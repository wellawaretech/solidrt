// The region handlers over the residency gate (src/residency.ts): an
// explicit region attached to a bounded core node loads, builds under
// the slice budget and shows once a viewer wants it, unloads before its
// teardown when the viewer leaves, retains its data for the next visit;
// a leave mid-load aborts the load and a leave mid-build closes the
// build's generator; a load that throws fails the region and the next
// enter retries; a measured weight reaches the gate's stats; a grid
// spawns a handler per wanted cell and ends them with the grid, takes
// its band from its cell size when none is given, and lives in the space
// of the node that places it; a package's handle reaches every callback,
// hook and listener in place of the core region. Driven
// through flux:spatial directly: a viewer key with an orthographic view
// over a strip of the plane, as the 2d tile layer pushes.

import { test, expect } from "@solidrt/test"
import { createRegion, createRegionGrid, requestFrame } from "@solidrt/core"
import type { Region, RegionState } from "@solidrt/core"
import * as spatial from "flux:spatial"
import type { NodeId, ViewKey } from "flux:spatial"

/** The viewer: an orthographic camera over `w` x `h` world units with its
 * top-left at (x, y), looking down -z (the plane z in -1..1). */
function look(key: ViewKey, x: number, y: number, w: number, h: number): void {
  let proj = new Float64Array(16)
  proj[0] = 2 / w
  proj[5] = -2 / h
  proj[10] = -1
  proj[15] = 1
  let view = new Float64Array(16)
  view[0] = view[5] = view[10] = view[15] = 1
  view[12] = -(x + w / 2)
  view[13] = -(y + h / 2)
  spatial.setView(key, view, proj)
  // A viewer push alone demands no frame (a camera's params write does
  // in a scene): ask for the flush that tests the regions.
  requestFrame()
}

/** A unit-box node at x. */
function boxAt(x: number): NodeId {
  let node = spatial.createNode(new Float64Array([x, 0, 0, 0, 0, 0, 1, 1, 1, 1]), true)
  spatial.setBounds(node, new Float32Array([-0.5, -0.5, -0.5, 0.5, 0.5, 0.5]))
  return node
}

const VIEW = 10

type Log = string[]

test("an explicit region loads, builds, shows, unloads before its teardown, and retains its data", async app => {
  await app.mount(() => <view width={8} height={8} />)
  let log: Log = []
  let states: RegionState[] = []
  let region = createRegion<{ n: number }>({
    enter: 1,
    leave: 2,
    retain: true,
    label: "r",
    load: async (r, signal) => {
      log.push("load:" + r.state + ":" + signal.aborted)
      return { n: 7 }
    },
    build: function* (r, data) {
      log.push("build:" + r.state + ":" + data.n)
      yield
      log.push("built")
    },
    unload: (r, data) => log.push("unload:" + data.n + ":" + r.state),
    hooks: {
      spawn: () => log.push("spawn"),
      shown: () => log.push("shown"),
      teardown: () => log.push("teardown"),
    },
  })
  region.onStateChange = (_, s) => states.push(s)
  let node = boxAt(0)
  region.attach(node)
  expect(region.node).toBe(node)
  // A viewer far away: wanted by nobody.
  let key = spatial.allocViewKey()
  look(key, 100, -VIEW / 2, VIEW, VIEW)
  await app.frame(2)
  expect(region.state).toBe("unwanted")
  expect(log).toEqual([])
  // The viewer over the box: enter, load (a microtask), build (sliced),
  // shown.
  look(key, -VIEW / 2, -VIEW / 2, VIEW, VIEW)
  await app.settle()
  expect(region.state).toBe("shown")
  expect(region.data).toEqual({ n: 7 })
  expect(log).toEqual(["spawn", "load:loading:false", "build:building:7", "built", "shown"])
  expect(states).toEqual(["loading", "building", "shown"])
  // Away: unload with the data, then the teardown.
  look(key, 100, -VIEW / 2, VIEW, VIEW)
  await app.settle()
  expect(region.state).toBe("unwanted")
  expect(region.data).toBe(undefined)
  expect(log.slice(5)).toEqual(["unload:7:shown", "teardown"])
  // Back: the retained data skips the load.
  log.length = 0
  look(key, -VIEW / 2, -VIEW / 2, VIEW, VIEW)
  await app.settle()
  expect(log).toEqual(["spawn", "build:building:7", "built", "shown"])
  // Detaching ends it: the leave runs now.
  region.detach()
  expect(region.state).toBe("unwanted")
  expect(log.slice(4)).toEqual(["unload:7:shown", "teardown"])
  expect(region.node).toBe(null)
  spatial.setView(key, null)
})

test("a leave mid-load aborts the load and drops its result; a leave after a build tears the content down", async app => {
  await app.mount(() => <view width={8} height={8} />)
  let resolve!: (v: number) => void
  let signals: AbortSignal[] = []
  let built = 0
  let region = createRegion<number>({
    enter: 1,
    leave: 2,
    load: (_, signal) => {
      signals.push(signal)
      return new Promise<number>(r => (resolve = r))
    },
    build: function* () {
      for (let i = 0; i < 4; i++) {
        built++
        yield
      }
    },
  })
  region.attach(boxAt(0))
  let key = spatial.allocViewKey()
  look(key, -VIEW / 2, -VIEW / 2, VIEW, VIEW)
  await app.frame(2)
  expect(region.state).toBe("loading")
  look(key, 100, -VIEW / 2, VIEW, VIEW)
  await app.frame(2)
  expect(signals[0]!.aborted).toBe(true)
  expect(region.state).toBe("unwanted")
  // The stale result arrives: ignored, nothing builds.
  resolve(1)
  await app.frame(2)
  expect(built).toBe(0)
  expect(region.state).toBe("unwanted")
  // Back, and this load resolves before the next frame: the build is
  // queued for it and runs in its drain (every step free under the test
  // clock), ahead of the leave the same frame's flush produces; that
  // leave, delivered the frame after, tears the built content down.
  look(key, -VIEW / 2, -VIEW / 2, VIEW, VIEW)
  await app.frame(2)
  expect(region.state).toBe("loading")
  resolve(2)
  await Promise.resolve()
  await Promise.resolve()
  expect(region.state).toBe("building")
  expect(built).toBe(0)
  look(key, 100, -VIEW / 2, VIEW, VIEW)
  await app.frame(3)
  expect(built).toBe(4)
  expect(region.state).toBe("unwanted")
  expect(region.data).toBe(undefined)
  // And once more, left alone: the build runs whole and shows.
  look(key, -VIEW / 2, -VIEW / 2, VIEW, VIEW)
  await app.frame(2)
  resolve(3)
  await app.settle()
  expect(region.state).toBe("shown")
  expect(built).toBe(8)
  spatial.setView(key, null)
})

test("a load that throws fails the region, reports to onError, skips unload, and the next enter retries", async app => {
  await app.mount(() => <view width={8} height={8} />)
  let attempts = 0
  let errors: unknown[] = []
  let unloaded = 0
  let region = createRegion<number>({
    enter: 1,
    leave: 2,
    load: async () => {
      attempts++
      if (attempts === 1) throw new Error("no file")
      return attempts
    },
    build: () => {},
    unload: () => unloaded++,
  })
  region.onError = (_, e) => errors.push(e)
  region.attach(boxAt(0))
  let key = spatial.allocViewKey()
  look(key, -VIEW / 2, -VIEW / 2, VIEW, VIEW)
  await app.settle()
  expect(region.state).toBe("failed")
  expect((errors[0] as Error).message).toBe("no file")
  expect((region.error as Error).message).toBe("no file")
  look(key, 100, -VIEW / 2, VIEW, VIEW)
  await app.settle()
  expect(unloaded).toBe(0)
  expect(region.state).toBe("unwanted")
  look(key, -VIEW / 2, -VIEW / 2, VIEW, VIEW)
  await app.settle()
  expect(region.state).toBe("shown")
  expect(attempts).toBe(2)
  expect(region.data).toBe(2)
  spatial.setView(key, null)
})

test("a measured weight reaches the gate's stats and the limits; the gate's states show through", async app => {
  await app.mount(() => <view width={8} height={8} />)
  spatial.setResidencyLimit(100)
  let region = createRegion<undefined>({ enter: 1, leave: 2, weight: 10, build: r => r.measure(60) })
  region.attach(boxAt(0))
  let key = spatial.allocViewKey()
  look(key, -VIEW / 2, -VIEW / 2, VIEW, VIEW)
  await app.settle()
  expect(region.state).toBe("shown")
  let stats = app.stats() as { residentWeight: number; regionsResident: number }
  expect([stats.regionsResident, stats.residentWeight]).toEqual([1, 60])
  // A second region at 50 does not fit beside 60 under a cap of 100 and
  // is further away, so it stalls.
  let other = createRegion<undefined>({ enter: 1, leave: 2, weight: 50, build: () => {} })
  other.attach(boxAt(3))
  await app.settle()
  expect(other.state).toBe("stalled")
  expect(region.state).toBe("shown")
  region.detach()
  spatial.setView(key, null)
  spatial.setResidencyLimit(null)
})

test("a grid spawns a handler per wanted cell, tears them down with the grid, and validates", async app => {
  await app.mount(() => <view width={8} height={8} />)
  let log: Log = []
  let grid = createRegionGrid<string>({
    origin: [0, 0, 0],
    cell: [10, 10, 0],
    enter: 0,
    leave: 10,
    retain: 1,
    load: r => "cell " + r.cell!.join(","),
    build: (r, data) => {
      log.push("build " + data)
      r.measure(1)
    },
    unload: (_, data) => log.push("unload " + data),
    hooks: { spawn: r => log.push("spawn " + r.cell!.join(",")), teardown: r => log.push("teardown " + r.cell!.join(",")) },
  })
  let states: string[] = []
  grid.onStateChange = (r, s) => states.push(r.cell!.join(",") + ":" + s)
  expect(grid.grid).toBe(null)
  grid.start()
  expect(grid.grid === null).toBe(false)
  let key = spatial.allocViewKey()
  // Over cells 0 and 1 of row 0.
  look(key, 0, 0, 20, 10)
  await app.settle()
  expect(grid.cells().map(r => r.cell!.join(",")).sort()).toEqual(["0,0,0", "1,0,0"])
  expect(grid.cellAt(0, 0, 0)!.state).toBe("shown")
  expect(grid.cellAt(0, 0, 0)!.data).toBe("cell 0,0,0")
  expect(grid.cellAt(5, 0, 0)).toBe(null)
  expect(log.filter(l => l.startsWith("spawn")).sort()).toEqual(["spawn 0,0,0", "spawn 1,0,0"])
  expect(states.filter(s => s.endsWith("shown")).length).toBe(2)
  // Two cells right: 0 leaves (unload, teardown), 2 and 3 enter.
  look(key, 20, 0, 20, 10)
  await app.settle()
  expect(grid.cells().map(r => r.cell!.join(",")).sort()).toEqual(["1,0,0", "2,0,0", "3,0,0"])
  expect(log.filter(l => l.startsWith("unload"))).toEqual(["unload cell 0,0,0"])
  expect(log.indexOf("unload cell 0,0,0") < log.indexOf("teardown 0,0,0")).toBe(true)
  // The grid ends: every held cell leaves.
  grid.stop()
  expect(grid.cells()).toEqual([])
  expect(log.filter(l => l.startsWith("teardown")).length).toBe(4)
  expect(grid.grid).toBe(null)
  grid.dispose()
  expect(() => grid.start()).toThrow()
  expect(() => createRegionGrid({ origin: [0, 0, 0], cell: [1, 1, 1], enter: 2, leave: 1, build: () => {} })).toThrow()
  expect(() => createRegionGrid({ origin: [0, 0, 0], cell: [1, 1, 1], enter: 0, leave: 1, retain: -1, build: () => {} })).toThrow()
  expect(() => createRegion({ enter: -1, leave: 1, build: () => {} })).toThrow()
  expect(() => createRegion({ enter: 0, leave: 1, priority: 0.5, build: () => {} })).toThrow()
  let region: Region<undefined> = createRegion({ enter: 0, leave: 1, build: () => {} })
  expect(() => region.measure(-1)).toThrow()
  spatial.setView(key, null)
})

test("a grid takes its band from its cell size when none is given, and validates its shape", async app => {
  await app.mount(() => <view width={8} height={8} />)
  // Cells of 10: enter defaults to 5 and leave to 15, so a view over
  // cell (0, 0) wants the ring around it too.
  let grid = createRegionGrid<undefined>({ origin: [0, 0, 0], cell: [10, 10, 0], build: () => {} })
  grid.start()
  let key = spatial.allocViewKey()
  look(key, 0, 0, 10, 10)
  await app.settle()
  expect(grid.cells().map(r => r.cell!.join(",")).sort()).toEqual(["-1,-1,0", "-1,0,0", "-1,1,0", "0,-1,0", "0,0,0", "0,1,0", "1,-1,0", "1,0,0", "1,1,0"])
  grid.dispose()
  spatial.setView(key, null)
  let shape = { origin: [0, 0, 0] as [number, number, number], cell: [10, 10, 0] as [number, number, number], build: () => {} }
  expect(() => createRegionGrid({ ...shape, cell: [0, 0, 0] })).toThrow("unbounded on every axis")
  expect(() => createRegionGrid({ ...shape, cell: [10, -1, 0] })).toThrow("not negative")
  expect(() => createRegionGrid({ ...shape, origin: [0, NaN, 0] })).toThrow("origin")
  expect(() => createRegionGrid({ ...shape, bounds: [0, 0, 0, 1] as unknown as [number, number, number, number, number, number] })).toThrow("bounds")
  expect(() => createRegionGrid({ ...shape, root: boxAt(0), node: boxAt(1) })).toThrow("root or node")
  expect(() => createRegionGrid({ ...shape, enter: 20, leave: 10 })).toThrow("leave")
})

test("a package's handle reaches every callback, hook and listener in place of the core region", async app => {
  await app.mount(() => <view width={8} height={8} />)
  type Handle = { name: string }
  let log: Log = []
  let errors: string[] = []
  let grid = createRegionGrid<string, Handle>({
    origin: [0, 0, 0],
    cell: [10, 10, 0],
    enter: 0,
    leave: 10,
    handle: r => ({ name: "cell " + r.cell!.join(",") }),
    load: h => {
      if (h.name === "cell 1,0,0") throw new Error("boom")
      return h.name
    },
    build: (h, data) => {
      log.push("build " + h.name + " with " + data)
    },
    unload: h => log.push("unload " + h.name),
    hooks: { spawn: h => log.push("spawn " + h.name), shown: h => log.push("shown " + h.name), teardown: h => log.push("teardown " + h.name) },
  })
  let states: string[] = []
  grid.onStateChange = (h, s) => states.push(h.name + ":" + s)
  // No listener on the cell: the error falls through to the grid's.
  grid.onError = (h, e) => errors.push(h.name + " " + String(e))
  grid.start()
  let key = spatial.allocViewKey()
  look(key, 0, 0, 20, 10)
  await app.settle()
  expect(grid.cells().map(h => h.name).sort()).toEqual(["cell 0,0,0", "cell 1,0,0"])
  let held = grid.cellAt(0, 0, 0)!
  expect(held.name).toBe("cell 0,0,0")
  expect(log.filter(l => l.endsWith("0,0,0") || l.includes("0,0,0 "))).toEqual(["spawn cell 0,0,0", "build cell 0,0,0 with cell 0,0,0", "shown cell 0,0,0"])
  expect(errors).toEqual(["cell 1,0,0 Error: boom"])
  expect(states).toContain("cell 0,0,0:shown")
  expect(states).toContain("cell 1,0,0:failed")
  grid.stop()
  expect(log).toContain("unload cell 0,0,0")
  expect(log).toContain("teardown cell 0,0,0")
  grid.dispose()
  // An explicit region: the handle made once, at creation.
  let made = 0
  let region = createRegion<undefined, Handle>({
    enter: 1,
    leave: 2,
    handle: () => {
      made++
      return { name: "section" }
    },
    build: h => {
      log.push("explicit " + h.name)
    },
  })
  let seen: string[] = []
  region.onStateChange = (h, s) => seen.push(h.name + ":" + s)
  expect(made).toBe(1)
  region.attach(boxAt(0))
  look(key, -VIEW / 2, -VIEW / 2, VIEW, VIEW)
  await app.settle()
  expect(log).toContain("explicit section")
  expect(seen[seen.length - 1]).toBe("section:shown")
  region.detach()
  expect(made).toBe(1)
  spatial.setView(key, null)
})

test("a grid placed by a node lives in the node's space and ends with it", async app => {
  await app.mount(() => <view width={8} height={8} />)
  // The node at x = 100: the view over x 102..108 is the grid's 2..8.
  let node = spatial.createNode(new Float64Array([100, 0, 0, 0, 0, 0, 1, 1, 1, 1]), true)
  let grid = createRegionGrid<undefined>({ origin: [0, 0, 0], cell: [10, 10, 0], enter: 0, leave: 0, node, build: () => {} })
  grid.start()
  let key = spatial.allocViewKey()
  look(key, 102, 2, 6, 6)
  await app.settle()
  expect(grid.cells().map(r => r.cell!.join(","))).toEqual(["0,0,0"])
  // The node moves 10 right under the still view: the grid's -8..-2.
  spatial.setTransform(node, new Float64Array([110, 0, 0, 0, 0, 0, 1, 1, 1, 1]))
  requestFrame()
  await app.settle()
  expect(grid.cells().map(r => r.cell!.join(","))).toEqual(["-1,0,0"])
  // Freed first: the gate ends the grid, the held cell leaves, and the
  // stop that follows finds nothing to end.
  spatial.destroyNode(node)
  requestFrame()
  await app.settle()
  expect(grid.cells()).toEqual([])
  expect(() => grid.stop()).not.toThrow()
  grid.dispose()
  spatial.setView(key, null)
})
