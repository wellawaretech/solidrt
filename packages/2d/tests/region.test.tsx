// Regions over a sprite layer (region.ts, components/region.tsx, the
// Residency section of AGENTS.md): a grid cell under a view loads,
// builds hidden behind its proxy, shows and measures its record bytes
// into the gate; the view leaving unloads before the content goes, snaps
// the teardown (no exit plays: the slot recycles at once) and takes the
// cell's group with it; a sprite re-homed in `unload` with keepWorld
// outlives its cell at the same world pose; a leave aborts a load in
// flight; a failed load is fail-soft and retries on the next enter; a
// grid limit stalls what does not fit; priority orders two grids under a
// budget of one enter per frame; an explicit region moves with its
// group and keeps its data under retain; a streaming source wants cells
// no view sees; the components mount their JSX per cell. The gate and
// the content are live spatial nodes, so these are app tests.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { setResidencyBudget } from "@solidrt/core"
import {
  addGroup,
  addSprite,
  createAtlas,
  createRegion,
  createRegionGrid,
  createSpriteLayer,
  createStreamingSource,
  fullFrame,
  Group,
  Region,
  RegionGrid,
  setGroup,
  setSpriteParent,
  setSpriteTransition,
  Sprite,
  SpriteLayer,
  worldPosition,
  POSE_FLOATS,
  SPRITE_FLOATS,
} from "../src/index.ts"
import type { RegionHandle, SpriteHandle, SpriteGroup, SpriteLayerHandle as LayerHandle } from "../src/index.ts"

// The atlas: one opaque white texel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
/** Cells 64 px square; a 32 x 32 view at the origin sits inside cell
 * (0, 0) and, with a strict enter band, wants no other. */
const CELL = 64
const VIEW = 32
const BAND = { enter: 0, leave: CELL }
/** The grids are bounded to the four cells of row 0 (an unbounded grid
 * has a cell wherever the view goes), and FAR_X puts the view box
 * [2000, 2032] far past LEAVE from every one of them. */
const BOUNDS: [number, number, number, number] = [0, 0, 3, 0]
const FAR_X = 2000
/** The record bytes of one sprite under the stock material (no style
 * streams): its pose and sprite records. */
const SPRITE_BYTES = (POSE_FLOATS + SPRITE_FLOATS) * Float32Array.BYTES_PER_ELEMENT
/** A sprite size and a pose inside the cell. */
const SIZE = 8
const AT = CELL / 2
/** Decimals a kept world pose is compared to. */
const DIGITS = 4
/** An exit long enough that a played one would still be running when
 * the test reads the slot. */
const EXIT_MS = 5000

type Stats = { regionsResident: number; regionsStalled: number; residentWeight: number; app: { regionsShown: number; regionsFailed: number } }

/** A layer with one 32 x 32 view at the origin, built inside the
 * mounted app. */
async function mounted(app: TestApp): Promise<{ layer: LayerHandle; view: ReturnType<LayerHandle["createView"]> }> {
  let built!: { layer: LayerHandle; view: ReturnType<LayerHandle["createView"]> }
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "region-atlas" })
    let layer = createSpriteLayer([atlas], { label: "region" })
    let view = layer.createView({ width: VIEW, height: VIEW })
    built = { layer, view }
    return <view width={VIEW} height={VIEW} />
  })
  return built
}

let stats = (app: TestApp) => app.stats() as Stats
let cellsOf = <T,>(cells: RegionHandle<T>[]) => cells.map(c => c.cell!.join(",")).sort()

test("a grid cell under the view loads, builds hidden behind its proxy, shows and measures; leaving unloads first, snaps the teardown and takes the group", async app => {
  let { layer, view } = await mounted(app)
  let log: string[] = []
  let sprites: SpriteHandle[] = []
  let proxy: SpriteHandle | null = null
  let grid = createRegionGrid<{ cell: string }>(layer, {
    cell: [CELL, CELL],
    bounds: BOUNDS,
    ...BAND,
    label: "world",
    load: async r => ({ cell: r.cell!.join(",") }),
    build: function* (r, data) {
      // Building: the content hidden, the proxy there under the group.
      log.push("build " + data.cell + " " + r.state + " " + r.content._visible + " " + (r.proxy !== null && r.proxy._parent === r.group))
      for (let i = 0; i < 2; i++) {
        let s = addSprite(layer, { parent: r.content, x: AT + i * SIZE, y: AT, w: SIZE, h: SIZE })
        setSpriteTransition(s, { position: { duration: EXIT_MS, exit: [0, 0] } })
        sprites.push(s)
        yield
      }
    },
    unload: (r, data) => log.push("unload " + data.cell + " live " + sprites.filter(s => s.layer !== null).length + " under " + r.content._children.size),
    proxy: r => {
      proxy = addSprite(layer, { parent: r.group, x: AT, y: AT, w: CELL, h: CELL })
      return proxy
    },
  })
  await app.frame(2)
  let cell = grid.cellAt(0, 0)
  expect(cell === null).toBe(false)
  expect(cell!.cell).toEqual([0, 0])
  expect(cell!.group._parent).toBe(grid.group)
  expect([cell!.group._x, cell!.group._y]).toEqual([0, 0])
  expect(proxy === null).toBe(false)
  await app.settle()
  expect(cell!.state).toBe("shown")
  expect(log).toEqual(["build 0,0 building false true"])
  expect(sprites.length).toBe(2)
  expect(cell!.content._visible).toBe(true)
  expect(cell!.proxy).toBe(null)
  expect(proxy!.layer).toBe(null)
  // The content is picked once shown, at its cell-local place - by a
  // marquee wholly inside the sprite too (a sprite is a solid to the
  // volume queries).
  expect(layer.pickRect(AT - SIZE, AT - SIZE, SIZE, SIZE)).toEqual([sprites[0]])
  expect(layer.pickRect(AT - 1, AT - 1, 2, 2)).toEqual([sprites[0]])
  let s = stats(app)
  expect(s.regionsResident).toBe(1)
  expect(s.residentWeight).toBe(2 * SPRITE_BYTES)
  expect(s.app.regionsShown).toBe(1)
  expect(cell!.measure()).toBe(s.residentWeight)
  // The view goes: unload sees the content alive, then the cell is gone
  // and its slots recycle at once (the declared exit never plays).
  let slot = sprites[1]!._slot
  view.setCamera({ x: FAR_X })
  await app.frame(2)
  expect(log[1]).toBe("unload 0,0 live 2 under 2")
  expect(grid.cellAt(0, 0)).toBe(null)
  expect(cell!.group.layer).toBe(null)
  expect(sprites.every(sp => sp.layer === null)).toBe(true)
  expect(grid.cells()).toEqual([])
  expect(addSprite(layer, { x: 0, y: 0, w: 1, h: 1 })._slot).toBe(slot)
  expect(stats(app).regionsResident).toBe(0)
  grid.dispose()
  expect(grid.group.layer).toBe(null)
})

test("a sprite re-homed in unload with keepWorld outlives its cell at the same world pose", async app => {
  let { layer, view } = await mounted(app)
  let kept: SpriteHandle | null = null
  let world: [number, number] | null = null
  let grid = createRegionGrid<undefined>(layer, {
    cell: [CELL, CELL],
    bounds: BOUNDS,
    ...BAND,
    build: r => {
      kept = addSprite(layer, { parent: r.content, x: AT, y: AT, w: SIZE, h: SIZE })
    },
    unload: () => {
      world = worldPosition(kept!)
      setSpriteParent(kept!, null, { keepWorld: true })
    },
  })
  await app.settle()
  expect(grid.cells().length).toBe(1)
  view.setCamera({ x: FAR_X })
  await app.settle()
  expect(grid.cells()).toEqual([])
  expect(kept!.layer).toBe(layer)
  expect(kept!._parent).toBe(null)
  expect(world).toEqual([AT, AT])
  expect([kept!._x, kept!._y]).toEqual([AT, AT])
  expect(worldPosition(kept!)).toEqual([AT, AT])
  grid.dispose()
})

test("keepWorld composes the pose through a rotated, scaled group; the default keeps the local fields", async app => {
  let { layer } = await mounted(app)
  let frame = addGroup(layer, { x: 100, y: 50, rotation: 0.3, scale: 2 })
  let inner = addGroup(layer, { x: 10, y: 5, rotation: 0.1, parent: frame })
  let sprite = addSprite(layer, { parent: inner, x: 3, y: 4, rotation: 0.2, w: SIZE, h: SIZE })
  let before = worldPosition(sprite)!
  setSpriteParent(sprite, null, { keepWorld: true })
  let after = worldPosition(sprite)!
  expect(after[0]).toBeCloseTo(before[0], DIGITS)
  expect(after[1]).toBeCloseTo(before[1], DIGITS)
  expect(sprite._rot).toBeCloseTo(0.3 + 0.1 + 0.2, DIGITS)
  expect(sprite._w).toBeCloseTo(2 * SIZE, DIGITS)
  expect(sprite._h).toBeCloseTo(2 * SIZE, DIGITS)
  // Back under the inner group, world kept: the local pose is the
  // original again.
  setSpriteParent(sprite, inner, { keepWorld: true })
  expect(sprite._x).toBeCloseTo(3, DIGITS)
  expect(sprite._y).toBeCloseTo(4, DIGITS)
  expect(sprite._rot).toBeCloseTo(0.2, DIGITS)
  expect(sprite._w).toBeCloseTo(SIZE, DIGITS)
  // A group too, and the plain re-parent keeps the LOCAL fields.
  let groupBefore = worldPosition(inner)!
  setGroup(inner, { parent: null, keepWorld: true })
  expect(worldPosition(inner)![0]).toBeCloseTo(groupBefore[0], DIGITS)
  expect(worldPosition(inner)![1]).toBeCloseTo(groupBefore[1], DIGITS)
  expect(inner._scale).toBeCloseTo(2, DIGITS)
  expect(inner._rot).toBeCloseTo(0.4, DIGITS)
  setGroup(inner, { parent: frame })
  expect(worldPosition(inner)![0]).not.toBeCloseTo(groupBefore[0], DIGITS)
  expect(inner._scale).toBeCloseTo(2, DIGITS)
})

test("a leave aborts a load in flight, and a failed load is fail-soft: the next enter retries", async app => {
  let { layer, view } = await mounted(app)
  let resolve: ((data: string) => void) | null = null
  let signal: AbortSignal | null = null
  let builds = 0
  let loads = 0
  let errors: unknown[] = []
  let grid = createRegionGrid<string>(layer, {
    cell: [CELL, CELL],
    bounds: BOUNDS,
    ...BAND,
    load: (_, s) => {
      loads++
      if (loads === 1) {
        signal = s
        return new Promise<string>(r => (resolve = r))
      }
      if (loads === 2) throw new Error("no such cell")
      return "ok"
    },
    build: () => {
      builds++
    },
  })
  grid.onError = (_, e) => errors.push(e)
  await app.frame(2)
  let cell = grid.cellAt(0, 0)!
  expect(cell.state).toBe("loading")
  // Away while loading: the signal aborts and the late data builds nothing.
  view.setCamera({ x: FAR_X })
  await app.settle()
  expect(signal!.aborted).toBe(true)
  resolve!("late")
  await app.settle()
  expect(builds).toBe(0)
  expect(grid.cells()).toEqual([])
  // Back: the load throws, the cell is failed and empty, the error reported.
  view.setCamera({ x: 0 })
  await app.settle()
  cell = grid.cellAt(0, 0)!
  expect(cell.state).toBe("failed")
  expect(String(cell.error)).toContain("no such cell")
  expect(errors.length).toBe(1)
  expect(stats(app).app.regionsFailed).toBe(1)
  expect(cell.content._visible).toBe(false)
  // Away and back again: the third load succeeds.
  view.setCamera({ x: FAR_X })
  await app.settle()
  view.setCamera({ x: 0 })
  await app.settle()
  expect(loads).toBe(3)
  expect(builds).toBe(1)
  expect(grid.cellAt(0, 0)!.state).toBe("shown")
  expect(stats(app).app.regionsFailed).toBe(0)
  grid.dispose()
})

test("a grid limit stalls the cell that does not fit until the limit is lifted", async app => {
  let { layer, view } = await mounted(app)
  // The view straddles cells (0, 0) and (1, 0); one cell's estimate fits
  // the limit, two do not.
  view.setCamera({ x: CELL - VIEW / 2 })
  let grid = createRegionGrid<undefined>(layer, {
    cell: [CELL, CELL],
    bounds: BOUNDS,
    ...BAND,
    weight: SPRITE_BYTES,
    limit: SPRITE_BYTES + 1,
    build: r => {
      addSprite(layer, { parent: r.content, x: AT, y: AT, w: SIZE, h: SIZE })
    },
  })
  await app.settle()
  expect(grid.cells().length).toBe(1)
  expect(grid.cells()[0]!.state).toBe("shown")
  expect(stats(app).regionsStalled).toBe(1)
  grid.setLimit(null)
  await app.settle()
  expect(cellsOf(grid.cells())).toEqual(["0,0", "1,0"])
  expect(stats(app).residentWeight).toBe(2 * SPRITE_BYTES)
  grid.dispose()
})

test("priority orders two grids under a budget of one enter per frame", async app => {
  let { layer } = await mounted(app)
  setResidencyBudget(1)
  let order: string[] = []
  let make = (label: string, priority: number) =>
    createRegionGrid<undefined>(layer, {
      cell: [CELL, CELL],
      bounds: BOUNDS,
      ...BAND,
      priority,
      label,
      build: () => {
        order.push(label)
      },
    })
  let low = make("low", 0)
  let high = make("high", 1)
  await app.settle()
  expect(order).toEqual(["high", "low"])
  low.dispose()
  high.dispose()
})

test("an explicit region moves with its group, keeps its data under retain, and ends at dispose", async app => {
  let { layer } = await mounted(app)
  let log: string[] = []
  let proxies: SpriteHandle[] = []
  let region = createRegion<{ n: number }>(layer, {
    x: 0,
    y: 0,
    width: CELL,
    height: CELL,
    ...BAND,
    retain: true,
    label: "section",
    load: () => ({ n: log.length }),
    build: (r, data) => {
      log.push("build " + data.n)
      addSprite(layer, { parent: r.content, x: AT, y: AT, w: SIZE, h: SIZE })
    },
    unload: () => log.push("unload"),
    proxy: r => {
      let p = addSprite(layer, { parent: r.group, x: AT, y: AT, w: CELL, h: CELL })
      proxies.push(p)
      return p
    },
  })
  expect(["unwanted", "pending"]).toContain(region.state)
  expect(region.proxy === null).toBe(false)
  expect(proxies.length).toBe(1)
  await app.settle()
  expect(region.state).toBe("shown")
  expect(region.proxy).toBe(null)
  expect(proxies[0]!.layer).toBe(null)
  expect(log).toEqual(["build 0"])
  expect(region.content._children.size).toBe(1)
  expect(stats(app).residentWeight).toBe(SPRITE_BYTES)
  // Moved away: the content leaves, the proxy stands again.
  setGroup(region.group, { x: FAR_X })
  await app.settle()
  expect(region.state).toBe("unwanted")
  expect(log).toEqual(["build 0", "unload"])
  expect(region.content._children.size).toBe(0)
  expect(region.proxy).toBe(proxies[1])
  // Back: the retained data builds again without a load.
  setGroup(region.group, { x: 0 })
  await app.settle()
  expect(region.state).toBe("shown")
  expect(log).toEqual(["build 0", "unload", "build 0"])
  region.dispose()
  expect(region.group.layer).toBe(null)
  expect(stats(app).regionsResident).toBe(0)
  expect(() => createRegion(layer, { x: 0, y: 0, width: -1, height: 1, enter: 1, leave: 2, build: () => {} })).toThrow()
  expect(() => createRegionGrid(layer, { cell: [0, 0], enter: 1, leave: 2, build: () => {} })).toThrow()
  expect(() => createRegionGrid(layer, { cell: [CELL, CELL], enter: 2, leave: 1, build: () => {} })).toThrow()
})

test("a streaming source wants the cells around it that no view sees", async app => {
  let { layer, view } = await mounted(app)
  view.setCamera({ x: FAR_X })
  // A short leave band: the source moves one cell over, where the
  // cell it left is still within a cell's leave.
  let grid = createRegionGrid<undefined>(layer, { cell: [CELL, CELL], bounds: BOUNDS, enter: 0, leave: SIZE, build: () => {} })
  await app.settle()
  expect(grid.cells()).toEqual([])
  let source = createStreamingSource(layer, { x: 2 * CELL + AT, y: AT, radius: 5 })
  await app.settle()
  expect(cellsOf(grid.cells())).toEqual(["2,0"])
  source.setPosition(3 * CELL + AT, AT)
  await app.settle()
  expect(cellsOf(grid.cells())).toEqual(["3,0"])
  expect([source.x, source.y, source.radius]).toEqual([3 * CELL + AT, AT, 5])
  source.dispose()
  await app.settle()
  expect(grid.cells()).toEqual([])
  expect(() => createStreamingSource(layer, { x: 0, y: 0, radius: 0 })).toThrow()
  grid.dispose()
})

test("the components mount their build JSX per cell under the cell's content and the proxy until shown", async app => {
  let built: SpriteHandle[] = []
  let proxies: SpriteHandle[] = []
  let states: string[] = []
  let section: SpriteHandle[] = []
  let contents: SpriteGroup[] = []
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "region-atlas" })
    return (
      <SpriteLayer atlases={[atlas]} width={VIEW} height={VIEW} label="region">
        <RegionGrid<{ tag: string }>
          cell={[CELL, CELL]}
          bounds={BOUNDS}
          enter={BAND.enter}
          leave={BAND.leave}
          load={r => ({ tag: r.cell!.join(",") })}
          build={(r, data) => {
            contents.push(r.content)
            return <Sprite x={AT + Number(data.tag[0])} y={AT} w={SIZE} h={SIZE} frame={fullFrame(atlas)} ref={s => built.push(s)} />
          }}
          proxy={() => <Sprite x={AT} y={AT} w={CELL} h={CELL} frame={fullFrame(atlas)} ref={s => proxies.push(s)} />}
          onStateChange={(r, s) => states.push(r.cell!.join(",") + ":" + s)}
        />
        <Group x={0} y={0}>
          <Region x={0} y={0} width={CELL} height={CELL} enter={BAND.enter} leave={BAND.leave} build={() => <Sprite x={AT} y={AT} w={SIZE} h={SIZE} frame={fullFrame(atlas)} ref={s => section.push(s)} />} />
        </Group>
      </SpriteLayer>
    )
  })
  await app.settle()
  expect(built.length).toBe(1)
  expect(proxies.length).toBe(1)
  expect(proxies[0]!.layer).toBe(null)
  expect(built[0]!._parent).toBe(contents[0])
  expect(states[states.length - 1]).toBe("0,0:shown")
  expect(states.every(s => s.startsWith("0,0:"))).toBe(true)
  expect(section.length).toBe(1)
  expect(section[0]!.layer !== null).toBe(true)
  expect(stats(app).residentWeight).toBe(2 * SPRITE_BYTES)
})

test("a region grid inside a group throws", async app => {
  let threw = ""
  // A component body under the <Group>, calling the grid component as
  // the plain function it is, so the throw lands where it can be caught.
  let Probe = () => {
    try {
      RegionGrid({ cell: [CELL, CELL], enter: 0, leave: CELL, build: () => <></> })
    } catch (error) {
      threw = String(error)
    }
    return <></>
  }
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "region-atlas" })
    return (
      <SpriteLayer atlases={[atlas]} width={VIEW} height={VIEW}>
        <Group>
          <Probe />
        </Group>
      </SpriteLayer>
    )
  })
  expect(threw).toContain("layer root")
})

test("a view wants only its own layer's regions: two layers over the same coordinates stream apart", async app => {
  let layers: LayerHandle[] = []
  let views: ReturnType<LayerHandle["createView"]>[] = []
  await app.mount(() => {
    let atlas = createAtlas(ATLAS, { label: "region-atlas" })
    for (let label of ["a", "b"]) {
      let layer = createSpriteLayer([atlas], { label })
      layers.push(layer)
      views.push(layer.createView({ width: VIEW, height: VIEW }))
    }
    return <view width={VIEW} height={VIEW} />
  })
  let grids = layers.map(layer => createRegionGrid<undefined>(layer, { cell: [CELL, CELL], bounds: BOUNDS, ...BAND, label: layer === layers[0] ? "a" : "b", build: () => {} }))
  await app.settle()
  expect([cellsOf(grids[0]!.cells()), cellsOf(grids[1]!.cells())]).toEqual([["0,0"], ["0,0"]])
  // Layer b's view goes to the far end: only b's cell moves with it.
  views[1]!.setCamera({ x: 3 * CELL })
  await app.settle()
  expect([cellsOf(grids[0]!.cells()), cellsOf(grids[1]!.cells())]).toEqual([["0,0"], ["3,0"]])
  // A source in layer a wants a's cell there, never b's.
  let source = createStreamingSource(layers[0]!, { x: 2 * CELL + AT, y: AT, radius: 5 })
  await app.settle()
  expect([cellsOf(grids[0]!.cells()), cellsOf(grids[1]!.cells())]).toEqual([["0,0", "2,0"], ["3,0"]])
  source.dispose()
  for (let grid of grids) grid.dispose()
})
