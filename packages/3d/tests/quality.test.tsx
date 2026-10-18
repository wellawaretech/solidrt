// The quality tier through the engine: a scene's knobs default from the
// row of the app tier (core's setQuality) or of its own `quality`, read
// back where they land - the scene target's multisample count and its
// shadow tap params in the /gpu inventory, the shadow atlas's size (map
// edge times cascade count), the fill cap in the target size - and every
// explicit option stays put when the row moves. Rows resolve as the
// table says (okf/done/3d-quality-tier.md); the app tier is restored to
// medium at the end of every test that moves it, since the signal is the
// engine's.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { quality, setQuality } from "@solidrt/core"
import { DirectionalLight, Mesh, QUALITY_TIERS, Scene, box, createScene, resolveQuality, standard } from "../src/index.ts"
import type { QualityInput, SceneHandle, ShadowOptions } from "../src/index.ts"

type Pipeline = { kind: string; label?: string; samples?: number; params: Record<string, unknown> }
type Texture = { label?: string; width: number; height: number }

// A small fixed target: the row's multisample count is read off the
// inventory, not off pixels, so the size does not matter.
const SIDE = 32
// The light's map edge in the tests that pin one, kept small so a 2048
// row never allocates a 4096-wide atlas in the test client.
const SMALL_MAP = 128

/** The scene's own draw target as the inventory reports it. */
function target(app: TestApp, label: string): Pipeline {
  let pipelines = app.gpu({ label }).pipelines as Pipeline[]
  let found = pipelines.find(p => p.kind === "draws" && p.label === label)
  if (found === undefined) throw new Error("The scene's draw target is not in the inventory: " + label)
  return found
}

/** The scene's shadow atlas texture, or null while nothing casts. */
function atlas(app: TestApp, label: string): Texture | null {
  let name = label + "-shadow-atlas"
  let textures = app.gpu({ label: name }).textures as Texture[]
  return textures.find(t => t.label === name) ?? null
}

async function mounted(
  app: TestApp,
  label: string,
  props: { quality?: QualityInput; samples?: 1 | 2 | 4 | 8; lodBias?: number; shadows?: { quality?: "hard" | "low" | "medium" | "high" } } = {},
  shadow: ShadowOptions = {},
): Promise<SceneHandle> {
  let scene!: SceneHandle
  await app.mount(() => (
    <Scene label={label} width={SIDE} height={SIDE} ref={s => (scene = s)} {...props}>
      <DirectionalLight castShadow position={[0, 10, 0]} shadow={shadow} />
      <Mesh geometry={box()} material={standard({ color: [1, 1, 1] })} castShadow />
    </Scene>
  ))
  await app.settle()
  return scene
}

test("the medium row by default: one sample, a 5x5 kernel, two 1024 cascades", async app => {
  let scene = await mounted(app, "q-medium")
  expect(quality()).toBe("medium")
  expect(scene.quality()).toEqual(QUALITY_TIERS.medium)
  let t = target(app, "q-medium")
  // Single-sample is reported by absence, like every default.
  expect(t.samples).toBeUndefined()
  expect(t.params.uShadowFilterTaps).toBe(16)
  // Two cascades of 1024 tile a 2 x 1 atlas.
  expect(atlas(app, "q-medium")).toMatchObject({ width: 2048, height: 1024 })
})

test("the app tier moves every unset knob live, and back", async app => {
  let scene = await mounted(app, "q-app")
  try {
    setQuality("low")
    await app.settle()
    expect(scene.quality()).toEqual(QUALITY_TIERS.low)
    let t = target(app, "q-app")
    expect(t.samples).toBeUndefined()
    expect(t.params.uShadowFilterTaps).toBe(0)
    expect(atlas(app, "q-app")).toMatchObject({ width: 512, height: 512 })
    setQuality("medium")
    await app.settle()
    expect(target(app, "q-app").samples).toBeUndefined()
    expect(target(app, "q-app").params.uShadowFilterTaps).toBe(16)
    expect(atlas(app, "q-app")).toMatchObject({ width: 2048, height: 1024 })
  } finally {
    setQuality("medium")
  }
})

test("a light that authored a camera box stays one map under a cascading row", async app => {
  await mounted(app, "q-box", {}, { camera: { far: 20 } })
  expect(atlas(app, "q-box")).toMatchObject({ width: 1024, height: 1024 })
})

test("explicit options win over the row, whichever way it moves", async app => {
  let scene = await mounted(app, "q-explicit", { quality: "high", samples: 1, shadows: { quality: "low" }, lodBias: 0.5 }, { mapSize: SMALL_MAP })
  let t = target(app, "q-explicit")
  expect(t.samples).toBeUndefined()
  expect(t.params.uShadowFilterTaps).toBe(8)
  // The high row's two cascades at the light's own edge.
  expect(atlas(app, "q-explicit")).toMatchObject({ width: 2 * SMALL_MAP, height: SMALL_MAP })
  // The row in force is still high; the explicit knobs sit beside it.
  expect(scene.quality()).toEqual(QUALITY_TIERS.high)
  scene.setQuality("low")
  await app.settle()
  t = target(app, "q-explicit")
  expect(t.samples).toBeUndefined()
  expect(t.params.uShadowFilterTaps).toBe(8)
  expect(atlas(app, "q-explicit")).toMatchObject({ width: SMALL_MAP, height: SMALL_MAP })
})

test("a row object overrides the app tier field by field; a name replaces it whole", async app => {
  let scene = await mounted(app, "q-row", { quality: { samples: 2 } }, { mapSize: SMALL_MAP })
  expect(scene.quality()).toEqual(resolveQuality({ samples: 2 }, "medium"))
  expect(target(app, "q-row").samples).toBe(2)
  expect(target(app, "q-row").params.uShadowFilterTaps).toBe(16)
  scene.setQuality("low")
  await app.settle()
  expect(target(app, "q-row").samples).toBeUndefined()
  expect(target(app, "q-row").params.uShadowFilterTaps).toBe(0)
  expect(atlas(app, "q-row")).toMatchObject({ width: SMALL_MAP, height: SMALL_MAP })
  // Back to following the app tier.
  scene.setQuality(null)
  await app.settle()
  expect(target(app, "q-row").samples).toBeUndefined()
})

test("setSamples fixes the count; the row no longer moves it", async app => {
  let scene = await mounted(app, "q-samples", {}, { mapSize: SMALL_MAP })
  scene.setSamples(2)
  await app.settle()
  expect(target(app, "q-samples").samples).toBe(2)
  scene.setQuality("low")
  await app.settle()
  expect(target(app, "q-samples").samples).toBe(2)
  expect(target(app, "q-samples").params.uShadowFilterTaps).toBe(0)
})

test("a fill scene's pixel cap defaults to the row's", async app => {
  let scene!: SceneHandle
  await app.mount(() => (
    <view width={200} height={100}>
      <Scene label="q-fill" quality={{ maxPixels: 5000 }} ref={s => (scene = s)} />
    </view>
  ))
  await app.settle()
  // 200 x 100 leaf pixels at any display scale, capped to 5000 with one
  // factor on both axes: half the leaf's logical size.
  expect(scene.size()).toEqual({ width: 100, height: 50 })
})

test("an invalid quality throws where it enters", async app => {
  let scene = await mounted(app, "q-invalid")
  expect(() => scene.setQuality("ultra" as never)).toThrow('quality must be "low", "medium", "high" or a row object')
  expect(() => scene.setQuality({ samples: 3 as never })).toThrow("quality.samples must be 1, 2, 4 or 8")
  expect(() => scene.setQuality({ cascades: 0 })).toThrow("quality.cascades")
  expect(() => createScene(SIDE, SIDE, { quality: { mipLimit: -1 } })).toThrow("quality.mipLimit")
})
