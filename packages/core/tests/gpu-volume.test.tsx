// 3D and array textures against a running GPU: a 3D texture sampled by
// position reads the hardware blend between two slices (trilinear, the
// filter the volume exists for), an array texture sampled by layer reads
// each layer unblended, a box upload changes exactly the texels it names,
// and the shape and limit errors throw at the call site. Everything is
// read back through a fragment target, since readTexture is 2D-shaped.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import {
  createArrayTexture,
  createMutableTexture,
  createShaderTexture,
  createTexture3D,
  glsl,
  limits,
  readTexture,
  resizeTexture,
  uploadTexture,
} from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"

// The volume: 2x2 texels per slice, two slices (or two layers).
const SIDE = 2
const SLICES = 2
// The target: one output pixel per probe.
const PROBES = 4
// A channel read is within this of the expected blend (8-bit rounding).
const SLACK = 2

const RED = [255, 0, 0, 255]
const BLUE = [0, 0, 255, 255]
const GREEN = [0, 255, 0, 255]

function solid(texels: number, rgba: number[]): Uint8Array {
  let data = new Uint8Array(texels * 4)
  for (let i = 0; i < texels; i++) data.set(rgba, i * 4)
  return data
}

/** Slice 0 red, slice 1 blue: the volume both shapes start from. */
function twoSlices(): Uint8Array {
  let data = new Uint8Array(SIDE * SIDE * SLICES * 4)
  data.set(solid(SIDE * SIDE, RED), 0)
  data.set(solid(SIDE * SIDE, BLUE), SIDE * SIDE * 4)
  return data
}

function pixel(texture: TextureId, x: number): number[] {
  let { data } = readTexture(texture)
  let at = x * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!]
}

/** A solid read is exact; a blend is within the rounding slack per channel. */
function expectBlend(got: number[], want: number[]): void {
  let off = Math.max(...got.map((channel, i) => Math.abs(channel - want[i]!)))
  expect(off).toBeLessThanOrEqual(SLACK)
}

// One probe per output pixel: the third coordinate of the sample comes
// from the uniform array, the first two sit at the volume's centre texel
// seam (0.5, 0.5), where bilinear within a slice is a wash of equal
// texels and only the third axis decides the read.
const VOLUME_PROBE = glsl`
  uniform sampler3D uVolume;
  uniform float uW[${PROBES}];
  void main() {
    int i = int(vUV.x * float(${PROBES}));
    fragColor = texture(uVolume, vec3(0.5, 0.5, uW[i]));
  }
`

const LAYER_PROBE = glsl`
  uniform sampler2DArray uLayers;
  uniform float uLayer[${PROBES}];
  void main() {
    int i = int(vUV.x * float(${PROBES}));
    fragColor = texture(uLayers, vec3(0.5, 0.5, uLayer[i]));
  }
`

// A nearest probe of one texel of a 2x2x2 volume per output pixel: the
// texel's centre on every axis, so the read is that texel alone.
const TEXEL_PROBE = glsl`
  uniform sampler3D uVolume;
  uniform vec3 uTexel[${PROBES}];
  void main() {
    int i = int(vUV.x * float(${PROBES}));
    fragColor = texture(uVolume, (uTexel[i] + 0.5) / ${SIDE}.0);
  }
`

async function mounted<T extends { texture: TextureId }>(app: TestApp, build: () => T): Promise<T> {
  let built!: T
  await app.mount(() => {
    built = build()
    return <texture src={built.texture} width={PROBES} height={1} />
  })
  return built
}

test("a 3D texture sampled by position blends between slices in hardware", async app => {
  let { texture } = await mounted(app, () => {
    let volume = createTexture3D(twoSlices(), SIDE, SIDE, SLICES, { label: "volume" })
    // Slice centres at 0.25 and 0.75, the midpoint at 0.5, and a quarter
    // of the way from slice 0 to slice 1 at 0.375.
    let texture = createShaderTexture(VOLUME_PROBE, PROBES, 1, { uW: [0.25, 0.75, 0.5, 0.375] }, {
      textures: { uVolume: volume },
      label: "volume-probe",
    })
    return { texture }
  })
  expect(pixel(texture, 0)).toEqual(RED)
  expect(pixel(texture, 1)).toEqual(BLUE)
  expectBlend(pixel(texture, 2), [128, 0, 128, 255])
  expectBlend(pixel(texture, 3), [191, 0, 64, 255])
})

test("an array texture sampled by layer reads each layer unblended", async app => {
  let { texture } = await mounted(app, () => {
    let layers = createArrayTexture(twoSlices(), SIDE, SIDE, SLICES, { label: "layers" })
    // The layer coordinate is an index: 0 and 1 are the layers, 0.4 rounds
    // to layer 0 and 1.6 clamps to layer 1 - never a blend.
    let texture = createShaderTexture(LAYER_PROBE, PROBES, 1, { uLayer: [0, 1, 0.4, 1.6] }, {
      textures: { uLayers: layers },
      label: "layer-probe",
    })
    return { texture }
  })
  expect(pixel(texture, 0)).toEqual(RED)
  expect(pixel(texture, 1)).toEqual(BLUE)
  expect(pixel(texture, 2)).toEqual(RED)
  expect(pixel(texture, 3)).toEqual(BLUE)
})

test("a box upload changes exactly the texels it names", async app => {
  let { texture, volume } = await mounted(app, () => {
    let volume = createTexture3D(twoSlices(), SIDE, SIDE, SLICES, { filter: "nearest", label: "bricks" })
    // Probes: the written texel (1, 0, 1), its neighbours along each axis
    // in the same slice and in slice 0.
    let texture = createShaderTexture(
      TEXEL_PROBE,
      PROBES,
      1,
      { uTexel: [1, 0, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0] },
      { textures: { uVolume: volume }, label: "brick-probe" },
    )
    return { texture, volume }
  })
  uploadTexture(volume, solid(1, GREEN), { x: 1, y: 0, z: 1, width: 1, height: 1, depth: 1 })
  await app.frame()
  expect(pixel(texture, 0)).toEqual(GREEN)
  expect(pixel(texture, 1)).toEqual(BLUE)
  expect(pixel(texture, 2)).toEqual(BLUE)
  expect(pixel(texture, 3)).toEqual(RED)
  // A whole-volume upload replaces every slice; a box of a whole slice
  // replaces that slice alone.
  uploadTexture(volume, solid(SIDE * SIDE * SLICES, BLUE))
  uploadTexture(volume, solid(SIDE * SIDE, GREEN), { x: 0, y: 0, z: 0, width: SIDE, height: SIDE, depth: 1 })
  await app.frame()
  expect(pixel(texture, 0)).toEqual(BLUE)
  expect(pixel(texture, 3)).toEqual(GREEN)
})

test("shape and size mistakes throw at the call site", async app => {
  let volume!: TextureId
  let layers!: TextureId
  let flat!: TextureId
  await app.mount(() => {
    volume = createTexture3D(twoSlices(), SIDE, SIDE, SLICES, { label: "volume-bad" })
    layers = createArrayTexture(twoSlices(), SIDE, SIDE, SLICES, { label: "layers-bad" })
    flat = createMutableTexture(solid(SIDE * SIDE, RED), SIDE, SIDE, { label: "flat-bad" })
    return <texture src={flat} width={SIDE} height={SIDE} />
  })
  // The upload form must fit the shape.
  expect(() => uploadTexture(volume, solid(1, GREEN), { x: 0, y: 0, width: 1, height: 1 })).toThrow(/box/)
  expect(() => uploadTexture(flat, solid(1, GREEN), { x: 0, y: 0, z: 0, width: 1, height: 1, depth: 1 })).toThrow(/rect/)
  // A box outside the volume, of the wrong size, or empty.
  expect(() => uploadTexture(volume, solid(1, GREEN), { x: 0, y: 0, z: SLICES, width: 1, height: 1, depth: 1 })).toThrow(/outside/)
  expect(() => uploadTexture(layers, solid(2, GREEN), { x: 0, y: 0, z: 0, width: 1, height: 1, depth: 1 })).toThrow(/bytes/)
  expect(() => uploadTexture(volume, solid(0, GREEN), { x: 0, y: 0, z: 0, width: 1, height: 0, depth: 1 })).toThrow(/at least 1/)
  // The whole-volume form takes the whole volume.
  expect(() => uploadTexture(volume, solid(SIDE * SIDE, GREEN))).toThrow(/bytes/)
  // The create takes exactly one volume, within the device's ceilings.
  expect(() => createTexture3D(solid(SIDE * SIDE, RED), SIDE, SIDE, SLICES)).toThrow(/bytes/)
  expect(() => createTexture3D(solid(1, RED), 1, 1, limits.max3DTextureSize + 1)).toThrow(/max 3D texture size/)
  expect(() => createArrayTexture(solid(1, RED), 1, 1, limits.maxArrayTextureLayers + 1)).toThrow(/max array texture layers/)
  expect(() => createTexture3D(solid(0, RED), SIDE, 0, SLICES)).toThrow(/at least 1/)
  // The 2D-shaped verbs refuse a volume, naming its sampler.
  expect(() => readTexture(volume)).toThrow(/sampler3D/)
  expect(() => resizeTexture(layers, solid(SIDE * SIDE * SLICES, RED), SIDE, SIDE)).toThrow(/sampler2DArray/)
  // A binding must match the declared sampler, both ways.
  expect(() => createShaderTexture(VOLUME_PROBE, 1, 1, { uW: [0, 0, 0, 0] }, { textures: { uVolume: flat } })).toThrow(/createTexture3D/)
  expect(() => createShaderTexture(VOLUME_PROBE, 1, 1, { uW: [0, 0, 0, 0] }, { textures: { uVolume: layers } })).toThrow(/array texture/)
  expect(() => createShaderTexture(LAYER_PROBE, 1, 1, { uLayer: [0, 0, 0, 0] }, { textures: { uLayers: volume } })).toThrow(/3D texture/)
  expect(() => createShaderTexture(glsl`
    uniform sampler2D uTex;
    void main() { fragColor = texture(uTex, vUV); }
  `, 1, 1, null, { textures: { uTex: volume } })).toThrow(/sampler3D/)
  // The limits report the volume ceilings at or above the GLES 3.0 floors.
  expect(limits.max3DTextureSize >= 256).toBe(true)
  expect(limits.maxArrayTextureLayers >= 256).toBe(true)
})
