// Regions through the package (region.ts, components/region.tsx, the
// Residency section of AGENTS.md): a region grid's cell under the
// camera loads, builds one slice per frame with its content hidden and
// its proxy drawn, then shows and measures its GPU bytes into the gate;
// the camera leaving unloads before the content goes and takes the cell
// node with it; a re-entered cell takes its geometry buffers back from
// the pool; buildModel builds a model across steps and the model's
// bytes count; an explicit region declares itself while in a scene and
// leaves when removed; a streaming source wants cells no camera sees;
// the components mount their JSX per cell. The gate and the content are
// GPU state over live scene nodes, so these are app tests.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { BufferId } from "@solidrt/core/gpu"
import {
  add,
  box,
  buildModel,
  createMesh,
  createRegion,
  createRegionGrid,
  createScene,
  createStreamingSource,
  geometryBytes,
  Mesh,
  PerspectiveCamera,
  RegionGrid,
  remove,
  Scene,
  setTransform,
  standard,
} from "../src/index.ts"
import type { MeshNode, Model, SceneHandle } from "../src/index.ts"
import type { Scene as SceneApi } from "../src/scene.ts"
import type { ModelData } from "../src/gltf.ts"

const SIZE = 64
/** Cells 100 across on XY and a slab DEPTH thick in z; the camera over
 * cell (0, 0, 0) at this height looking down -z, within ENTER of the
 * slab's top and further than LEAVE from every neighbour; FAR_AWAY is
 * the same column past LEAVE. */
const CELL = 100
const DEPTH = 10
const EYE: [number, number, number] = [50, 50, 20]
const FAR_AWAY: [number, number, number] = [50, 50, 100]
const ENTER = 15
const LEAVE = 25
/** The cell-index box: one layer of cells (k = 0), so the column above
 * the camera is one cell and nothing stacks behind the eye. */
const ONE_LAYER: [number, number, number, number, number, number] = [-1000, -1000, 0, 1000, 1000, 0]

async function mounted(app: TestApp): Promise<SceneApi> {
  let scene!: SceneApi
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { label: "res3d" })
    scene.setCamera({ position: EYE, target: [EYE[0], EYE[1], 0], far: 60 })
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

let gray = () => standard({ color: [0.5, 0.5, 0.5] })

test("a grid cell under the camera loads, builds hidden behind its proxy, shows and measures; leaving unloads first and takes the node", async app => {
  let scene = await mounted(app)
  let log: string[] = []
  let meshes: MeshNode[] = []
  let proxy: MeshNode | null = null
  let grid = createRegionGrid<{ cell: string }>({
    cell: [CELL, CELL, DEPTH],
    bounds: ONE_LAYER,
    enter: ENTER,
    leave: LEAVE,
    label: "world",
    load: async r => ({ cell: r.cell!.join(",") }),
    build: function* (r, data) {
      // Building: the content hidden, the proxy there.
      log.push("build " + data.cell + " " + r.state + " " + r.content.visible + " " + (r.proxy !== null))
      for (let i = 0; i < 2; i++) {
        let m = createMesh(box({ width: 2, height: 2, depth: 2 }), gray())
        setTransform(m, { position: [CELL / 2 + i * 3, CELL / 2, DEPTH / 2] })
        meshes.push(m)
        r.warm(m.geometry)
        add(r.content, m)
        yield
      }
    },
    unload: (r, data) => log.push("unload " + data.cell + " children " + r.content.children.length),
    proxy: () => {
      proxy = createMesh(box({ width: CELL, height: CELL, depth: 1 }), gray())
      return proxy
    },
  })
  add(scene.root, grid)
  await app.frame(2)
  let cell = grid.cellAt(0, 0, 0)
  expect(cell === null).toBe(false)
  expect(cell!.cell).toEqual([0, 0, 0])
  expect(cell!.parent).toBe(grid)
  expect(cell!.position).toEqual([0, 0, 0])
  expect(proxy === null).toBe(false)
  await app.settle()
  expect(cell!.state).toBe("shown")
  expect(log).toEqual(["build 0,0,0 building false true"])
  expect(meshes.length).toBe(2)
  expect(scene.cullState(meshes[0]!)).toBe("on")
  expect(cell!.proxy).toBe(null)
  expect(proxy!._destroyed).toBe(true)
  expect(cell!.content.children.length).toBe(2)
  let stats = app.stats() as { regionsResident: number; residentWeight: number; app: { regionsShown: number } }
  expect(stats.regionsResident).toBe(1)
  expect(stats.residentWeight).toBe(geometryBytes(meshes[0]!.geometry) + geometryBytes(meshes[1]!.geometry))
  expect(stats.app.regionsShown).toBe(1)
  expect(cell!.measure()).toBe(stats.residentWeight)
  // The camera goes: unload sees the content, then the cell is gone.
  scene.setCamera({ position: FAR_AWAY, target: [FAR_AWAY[0], FAR_AWAY[1], 0] })
  await app.settle()
  expect(log[1]).toBe("unload 0,0,0 children 2")
  expect(grid.cellAt(0, 0, 0)).toBe(null)
  expect(cell!._destroyed).toBe(true)
  expect(grid.cells()).toEqual([])
  expect((app.stats() as { regionsResident: number }).regionsResident).toBe(0)
  grid.dispose()
})

test("a re-entered cell takes its geometry buffers back from the pool", async app => {
  let scene = await mounted(app)
  let ids: BufferId[] = []
  let grid = createRegionGrid<undefined>({
    cell: [CELL, CELL, DEPTH],
    bounds: ONE_LAYER,
    enter: ENTER,
    leave: LEAVE,
    build: r => {
      let m = createMesh(box({ width: 3, height: 3, depth: 3 }), gray())
      add(r.content, m)
    },
  })
  add(scene.root, grid)
  await app.settle()
  let first = grid.cellAt(0, 0, 0)!.content.children[0] as MeshNode
  ids.push(first._buffers!.buffers[0]!, first._buffers!.index)
  scene.setCamera({ position: FAR_AWAY, target: [FAR_AWAY[0], FAR_AWAY[1], 0] })
  await app.settle()
  let misses = (app.stats() as { app: { poolMisses: number } }).app.poolMisses
  scene.setCamera({ position: EYE, target: [EYE[0], EYE[1], 0] })
  await app.settle()
  let second = grid.cellAt(0, 0, 0)!.content.children[0] as MeshNode
  expect(second === first).toBe(false)
  expect([second._buffers!.buffers[0]!, second._buffers!.index]).toEqual(ids)
  expect((app.stats() as { app: { poolMisses: number } }).app.poolMisses).toBe(misses)
  grid.dispose()
})

/** Two boxes under a level group, the first placed twice. */
function hangar(): ModelData {
  let identity = { rotation: [0, 0, 0, 1] as [number, number, number, number], scale: [1, 1, 1] as [number, number, number] }
  return {
    nodes: [
      { name: "level", parent: null, position: [0, 0, 0], ...identity },
      { name: "crate", parent: 0, position: [1, 0, 0], ...identity },
      { name: "crate2", parent: 0, position: [3, 0, 0], ...identity },
      { name: "slab", parent: 0, position: [0, -1, 0], ...identity },
    ],
    parts: [
      { name: "crate", node: 1, placements: [2], skin: null, material: 0, geometry: box({ width: 1, height: 1, depth: 1 }) },
      { name: "slab", node: 3, skin: null, material: 0, geometry: box({ width: 4, height: 0.2, depth: 4 }) },
    ],
    skins: [],
    clips: [],
    materials: [
      {
        name: "paint",
        color: [1, 0.5, 0, 1],
        map: null,
        doubleSided: false,
        transparent: false,
        alphaMode: "OPAQUE",
        alphaCutoff: 0.5,
        normalMap: null,
        normalScale: 1,
        emissive: [0, 0, 0],
        emissiveIntensity: 1,
        emissiveMap: null,
        metalness: 0,
        roughness: 1,
        metalnessRoughnessMap: null,
      },
    ],
    images: [],
    bounds: new Float32Array([-2, -1.1, -2, 3.5, 0.5, 2]),
  }
}

test("buildModel builds a model across steps under the region, whose bytes the region measures, and disposes it at leave", async app => {
  let scene = await mounted(app)
  let steps = 0
  let model: Model | null = null
  let grid = createRegionGrid<ModelData>({
    cell: [CELL, CELL, DEPTH],
    bounds: ONE_LAYER,
    enter: ENTER,
    leave: LEAVE,
    label: "models",
    load: () => hangar(),
    build: function* (r, data) {
      let it = buildModel(r, data)
      let result = it.next()
      while (!result.done) {
        steps++
        yield
        result = it.next()
      }
      model = result.value
    },
  })
  add(scene.root, grid)
  await app.settle()
  let cell = grid.cellAt(0, 0, 0)!
  expect(cell.state).toBe("shown")
  // Two parts, each a step, each a warm step.
  expect(steps).toBe(4)
  expect(model!.parts.length).toBe(2)
  expect(model!.parent).toBe(cell.content)
  let weight = (app.stats() as { residentWeight: number }).residentWeight
  let parts = hangar().parts
  let geometry = geometryBytes(parts[0]!.geometry) + geometryBytes(parts[1]!.geometry)
  // The population's matrix buffer (capacity 2) counts too.
  expect(weight > geometry).toBe(true)
  expect(weight).toBe(cell.measure())
  scene.setCamera({ position: FAR_AWAY, target: [FAR_AWAY[0], FAR_AWAY[1], 0] })
  await app.settle()
  expect(grid.cells()).toEqual([])
  grid.dispose()
})

test("an explicit region declares itself while in a scene, keeps its proxy until shown, and leaves when removed", async app => {
  let scene = await mounted(app)
  let log: string[] = []
  let region = createRegion<undefined>({
    bounds: [-1, -1, -1, 1, 1, 1],
    enter: ENTER + 5,
    leave: LEAVE + 5,
    retain: true,
    label: "section",
    build: r => {
      log.push("build")
      add(r.content, createMesh(box({ width: 1, height: 1, depth: 1 }), gray()))
    },
    unload: () => log.push("unload"),
    proxy: () => createMesh(box({ width: 2, height: 2, depth: 2 }), gray()),
  })
  setTransform(region, { position: [EYE[0], EYE[1], 0] })
  expect(region.state).toBe("unwanted")
  expect(region.proxy === null).toBe(false)
  let proxy = region.proxy as MeshNode
  add(scene.root, region)
  await app.frame(2)
  expect(["pending", "loading", "building", "shown"]).toContain(region.state)
  await app.settle()
  expect(region.state).toBe("shown")
  expect(region.proxy).toBe(null)
  expect(proxy._destroyed).toBe(true)
  expect(log).toEqual(["build"])
  let mesh = region.content.children[0] as MeshNode
  expect(scene.cullState(mesh)).toBe("on")
  remove(region)
  expect(region.state).toBe("unwanted")
  expect(log).toEqual(["build", "unload"])
  expect(region.content.children.length).toBe(0)
  expect(region.proxy === null).toBe(false)
  // Back in: the retained data builds again.
  add(scene.root, region)
  await app.settle()
  expect(region.state).toBe("shown")
  expect(log).toEqual(["build", "unload", "build"])
  region.dispose()
  expect(region._destroyed).toBe(true)
  expect(() => createRegion({ bounds: [0, 0, 0, 1, 1], enter: 1, leave: 2, build: () => {} })).toThrow()
  expect(() => createRegion({ bounds: [1, 0, 0, 0, 1, 1], enter: 1, leave: 2, build: () => {} })).toThrow()
})

test("a streaming source wants the cells around it that no camera sees", async app => {
  let scene = await mounted(app)
  scene.setCamera({ position: FAR_AWAY, target: [FAR_AWAY[0], FAR_AWAY[1], 0] })
  let grid = createRegionGrid<undefined>({ cell: [CELL, CELL, DEPTH], bounds: ONE_LAYER, enter: ENTER, leave: LEAVE, build: () => {} })
  add(scene.root, grid)
  await app.settle()
  expect(grid.cells()).toEqual([])
  let source = createStreamingSource({ position: [150, 50, DEPTH / 2], radius: 5 })
  await app.settle()
  expect(grid.cells().map(c => c.cell!.join(","))).toEqual(["1,0,0"])
  source.setPosition([250, 50, DEPTH / 2])
  await app.settle()
  expect(grid.cells().map(c => c.cell!.join(","))).toEqual(["2,0,0"])
  expect(source.position).toEqual([250, 50, DEPTH / 2])
  source.dispose()
  await app.settle()
  expect(grid.cells()).toEqual([])
  expect(() => createStreamingSource({ position: [0, 0, 0], radius: 0 })).toThrow()
  grid.dispose()
})

test("the component mounts its build JSX per cell under the cell's content and its proxy until shown", async app => {
  let scene!: SceneHandle
  let built: MeshNode[] = []
  let proxies: MeshNode[] = []
  let states: string[] = []
  await app.mount(() => (
    <Scene width={SIZE} height={SIZE} ref={s => (scene = s)}>
      <PerspectiveCamera position={EYE} lookAt={[EYE[0], EYE[1], 0]} far={60} />
      <RegionGrid<{ tag: string }>
        cell={[CELL, CELL, DEPTH]}
        bounds={ONE_LAYER}
        enter={ENTER}
        leave={LEAVE}
        load={r => ({ tag: r.cell!.join(",") })}
        build={(_, data) => <Mesh geometry={box({ width: 2, height: 2, depth: 2 })} material={gray()} ref={m => built.push(m)} position={[CELL / 2 + Number(data.tag[0]), CELL / 2, DEPTH / 2]} />}
        proxy={() => <Mesh geometry={box({ width: CELL, height: CELL, depth: 1 })} material={gray()} ref={m => proxies.push(m)} />}
        onStateChange={(r, s) => states.push(r.cell!.join(",") + ":" + s)}
      />
    </Scene>
  ))
  await app.settle()
  expect([built.length, proxies.length, states]).toEqual([1, 1, states])
  expect(proxies.length).toBe(1)
  expect(proxies[0]!._destroyed).toBe(true)
  expect(built[0]!.parent!.kind).toBe("group")
  expect(scene.cullState(built[0]!)).toBe("on")
  expect(states[states.length - 1]).toBe("0,0,0:shown")
  // The scene's default camera stood for a flush before the component's
  // camera wrote: the gate collapses the enters it wanted with their
  // leaves, so no other cell ever loaded.
  expect(states.every(s => s.startsWith("0,0,0:"))).toBe(true)
  let weight = (app.stats() as { residentWeight: number }).residentWeight
  expect(weight).toBe(geometryBytes(built[0]!.geometry))
  scene.setCamera({ position: FAR_AWAY, target: [FAR_AWAY[0], FAR_AWAY[1], 0] })
  await app.settle()
  expect(built[0]!._destroyed).toBe(true)
  expect(states[states.length - 1]).toBe("0,0,0:unwanted")
})
