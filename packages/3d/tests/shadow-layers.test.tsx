// A casting light whose layers share no bit with the scene's casts
// nothing (its shadow views filter the scene's meshes by both masks):
// the sync that finds one warns once for that light, naming both masks,
// and a light the scene's mask admits stays quiet.

import { test, expect } from "@solidrt/test"
import { createSignal } from "solid-js"
import { DirectionalLight, Mesh, Scene, box, standard } from "../src/index.ts"

const SIDE = 32
// The light's map edge, kept small for the test client.
const SMALL_MAP = 128
// The scene's own mask and a layer it does not draw.
const SCENE_LAYER = 1
const OTHER_LAYER = 2
// The warning's opening, which the console line starts with.
const DISJOINT = "Scene 'shadow-layers'"

test("a casting light disjoint from the scene's layers warns once; one the scene admits does not", async app => {
  let warnings: string[] = []
  let warn = console.warn
  console.warn = (...args: unknown[]) => {
    warnings.push(args.map(String).join(" "))
  }
  try {
    let [sceneLayers, setSceneLayers] = createSignal(SCENE_LAYER)
    await app.mount(() => (
      <Scene label="shadow-layers" width={SIDE} height={SIDE} layers={sceneLayers()}>
        <DirectionalLight castShadow position={[0, 10, 0]} layers={SCENE_LAYER} shadow={{ mapSize: SMALL_MAP }} />
        <DirectionalLight castShadow position={[0, 10, 0]} layers={OTHER_LAYER} shadow={{ mapSize: SMALL_MAP }} />
        <Mesh geometry={box()} material={standard({ color: [1, 1, 1] })} castShadow />
      </Scene>
    ))
    await app.settle()
    let disjoint = () => warnings.filter(w => w.startsWith(DISJOINT))
    expect(disjoint().length).toBe(1)
    expect(disjoint()[0]).toContain("directional light 1 casts nothing")
    expect(disjoint()[0]).toContain("layers 0x2")
    expect(disjoint()[0]).toContain("scene's layers 0x1")
    // The scene's mask moving to the other layer leaves the first light
    // disjoint: its own warning, naming its masks.
    setSceneLayers(OTHER_LAYER)
    await app.settle()
    expect(disjoint().length).toBe(2)
    expect(disjoint()[1]).toContain("directional light 0 casts nothing")
    expect(disjoint()[1]).toContain("layers 0x1")
    expect(disjoint()[1]).toContain("scene's layers 0x2")
    // Once per light: the second light turning disjoint again stays quiet.
    setSceneLayers(SCENE_LAYER)
    await app.settle()
    expect(disjoint().length).toBe(2)
  } finally {
    console.warn = warn
  }
})
