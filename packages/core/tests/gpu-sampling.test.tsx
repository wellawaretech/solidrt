// The live halves of a texture's sampling and a target's multisampling
// against a running GPU: setTextureSampling changes the declared filter,
// anisotropy and base level of an id in place (read back off the /gpu
// inventory, which reports every id's sampling), under the same rules as
// creation - a base level needs a chain and a level the chain has, the
// mip flag is storage; setTargetSamples reallocates a draw target's
// storage at a new count under the same id, and a sampleable depth stays
// single-sample.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { createDrawTarget, createTexture, setTargetSamples, setTextureSampling } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"

type Sampler = { filter: string; wrap: string; mipmap: boolean; anisotropy: number; baseLevel: number }
type Texture = { label?: string; sampler: Sampler }
type Pipeline = { kind: string; label?: string; samples?: number }

// A 4x4 texture: a chain of three levels (4, 2, 1), so level 2 is the
// last one a base level may name.
const SIDE = 4
const LEVELS = 3

function sampler(app: TestApp, label: string): Sampler {
  let textures = app.gpu({ label }).textures as Texture[]
  let found = textures.find(t => t.label === label)
  if (found === undefined) throw new Error("texture not in the inventory: " + label)
  return found.sampler
}

function samples(app: TestApp, label: string): number | undefined {
  let pipelines = app.gpu({ label }).pipelines as Pipeline[]
  let found = pipelines.find(p => p.kind === "draws" && p.label === label)
  if (found === undefined) throw new Error("draw target not in the inventory: " + label)
  return found.samples
}

test("setTextureSampling changes filter, anisotropy and base level in place", async app => {
  let id!: TextureId
  await app.mount(() => {
    id = createTexture(new Uint8Array(SIDE * SIDE * 4), SIDE, SIDE, { mipmap: true, label: "sampling" })
    return <texture src={id} width={SIDE} height={SIDE} />
  })
  expect(sampler(app, "sampling")).toEqual({ filter: "linear", wrap: "clamp", mipmap: true, anisotropy: 1, baseLevel: 0 })
  setTextureSampling(id, { filter: "nearest", anisotropy: 8, baseLevel: 1 })
  expect(sampler(app, "sampling")).toEqual({ filter: "nearest", wrap: "clamp", mipmap: true, anisotropy: 8, baseLevel: 1 })
  // Absent fields keep their values.
  setTextureSampling(id, { wrap: "repeat" })
  expect(sampler(app, "sampling")).toEqual({ filter: "nearest", wrap: "repeat", mipmap: true, anisotropy: 8, baseLevel: 1 })
  // Past the chain, and the chain itself, are refused.
  expect(() => setTextureSampling(id, { baseLevel: LEVELS })).toThrow("past the mip chain")
  expect(() => setTextureSampling(id, { mipmap: false } as never)).toThrow("fixed at creation")
  expect(sampler(app, "sampling").baseLevel).toBe(1)
})

test("a base level needs a chain, at creation and later", async app => {
  let id!: TextureId
  await app.mount(() => {
    id = createTexture(new Uint8Array(SIDE * SIDE * 4), SIDE, SIDE, { label: "flat" })
    return <texture src={id} width={SIDE} height={SIDE} />
  })
  expect(() => setTextureSampling(id, { baseLevel: 1 })).toThrow("needs a mip chain")
  expect(() => createTexture(new Uint8Array(SIDE * SIDE * 4), SIDE, SIDE, { baseLevel: 1 })).toThrow("needs a mip chain")
  expect(() => createTexture(new Uint8Array(SIDE * SIDE * 4), SIDE, SIDE, { mipmap: true, baseLevel: LEVELS })).toThrow("past the mip chain")
})

test("setTargetSamples reallocates a draw target's storage at the new count", async app => {
  let target!: TextureId
  let sampleable!: TextureId
  await app.mount(() => {
    target = createDrawTarget(SIDE, SIDE, null, { depth: true, label: "resample" })
    sampleable = createDrawTarget(SIDE, SIDE, null, { depth: "texture", label: "resample-depth" })
    return <texture src={target} width={SIDE} height={SIDE} />
  })
  expect(samples(app, "resample")).toBeUndefined()
  setTargetSamples(target, 4)
  expect(samples(app, "resample")).toBe(4)
  setTargetSamples(target, 1)
  expect(samples(app, "resample")).toBeUndefined()
  expect(() => setTargetSamples(sampleable, 4)).toThrow("cannot be multisampled")
  expect(() => setTargetSamples(target, 3 as never)).toThrow("1, 2, 4 or 8")
})
