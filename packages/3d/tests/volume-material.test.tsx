// A custom material samples a 3D texture by position and an array texture
// by layer through its `textures` bindings, with no change to the package:
// the scene's draw target binds whatever shape the declared sampler takes.
// GPU state throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { createArrayTexture, createTexture3D, glsl, readTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { add, createMesh, createScene, plane, shaderMaterial } from "../src/index.ts"
import type { Material, SceneHandle } from "../src/index.ts"

const SIZE = 64
// A camera at this distance with a 90 degree vertical fov sees 2 x DEPTH
// world units over SIZE pixels; the quad fills the middle of the frame.
const DEPTH = 4
const FOV = 90
const QUAD = 4
const CLEAR: [number, number, number, number] = [0, 0, 0, 1]
// The volume: 2x2 texels per slice, two slices (or two layers).
const SIDE = 2
const SLICES = 2
// A channel of a blend is within this of the expected value.
const SLACK = 2
// The scene resolves linear light to display sRGB (the piecewise curve an
// "rgba8-srgb" target encodes with): the half blend of red and blue reads
// as the sRGB encoding of 0.5 per channel, not 128.
const HALF_SRGB = Math.round((1.055 * Math.pow(0.5, 1 / 2.4) - 0.055) * 255)

const RED = [255, 0, 0, 255]
const BLUE = [0, 0, 255, 255]

function solid(texels: number, rgba: number[]): Uint8Array {
  let data = new Uint8Array(texels * 4)
  for (let i = 0; i < texels; i++) data.set(rgba, i * 4)
  return data
}

/** Slice 0 red, slice 1 blue. */
function twoSlices(): Uint8Array {
  let data = new Uint8Array(SIDE * SIDE * SLICES * 4)
  data.set(solid(SIDE * SIDE, RED), 0)
  data.set(solid(SIDE * SIDE, BLUE), SIDE * SIDE * 4)
  return data
}

const VERTEX = glsl`
  in vec3 aPos;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  void main() { gl_Position = uViewProj * uModel * vec4(aPos, 1.0); }
`

// The whole quad reads one position of the volume: the slice seam at
// uW, where only the third axis decides the colour.
const VOLUME_FRAGMENT = glsl`
  uniform sampler3D uVolume;
  uniform float uW;
  void main() { fragColor = texture(uVolume, vec3(0.5, 0.5, uW)); }
`

const LAYER_FRAGMENT = glsl`
  uniform sampler2DArray uSheet;
  uniform float uSheetLayer;
  void main() { fragColor = texture(uSheet, vec3(0.5, 0.5, uSheetLayer)); }
`

async function mounted(app: TestApp, make: () => Material): Promise<SceneHandle> {
  let scene!: SceneHandle
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { clearColor: CLEAR, label: "volume-material" })
    scene.setCamera({ fov: FOV, position: [0, 0, DEPTH], target: [0, 0, 0] })
    add(scene.root, createMesh(plane({ width: QUAD, height: QUAD }), make()))
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

function centre(scene: SceneHandle): number[] {
  let { width, data } = readTexture(scene.texture)
  let at = ((SIZE / 2) * width + SIZE / 2) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!]
}

test("a shader material samples a 3D texture by position, blended between slices", async app => {
  let volume!: TextureId
  let scene = await mounted(app, () => {
    volume = createTexture3D(twoSlices(), SIDE, SIDE, SLICES, { label: "material-volume" })
    return shaderMaterial({ vertex: VERTEX, fragment: VOLUME_FRAGMENT, params: { uW: 0.5 }, textures: { uVolume: volume } })
  })
  let got = centre(scene)
  let off = Math.max(...got.map((channel, i) => Math.abs(channel - [HALF_SRGB, 0, HALF_SRGB, 255][i]!)))
  expect(off).toBeLessThanOrEqual(SLACK)
})

test("a shader material samples an array texture by layer", async app => {
  let layers!: TextureId
  let scene = await mounted(app, () => {
    layers = createArrayTexture(twoSlices(), SIDE, SIDE, SLICES, { label: "material-layers" })
    return shaderMaterial({ vertex: VERTEX, fragment: LAYER_FRAGMENT, params: { uSheetLayer: 1 }, textures: { uSheet: layers } })
  })
  expect(centre(scene)).toEqual(BLUE)
})

test("a shape mismatch on a material binding throws at the add", async app => {
  let layers!: TextureId
  let threw = false
  await app.mount(() => {
    let scene = createScene(SIZE, SIZE, { clearColor: CLEAR, label: "volume-mismatch" })
    layers = createArrayTexture(twoSlices(), SIDE, SIDE, SLICES, { label: "mismatch-layers" })
    let material = shaderMaterial({ vertex: VERTEX, fragment: VOLUME_FRAGMENT, params: { uW: 0.5 }, textures: { uVolume: layers } })
    try {
      add(scene.root, createMesh(plane({ width: QUAD, height: QUAD }), material))
    } catch (error) {
      threw = /array texture/.test(String(error))
    }
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  expect(threw).toBe(true)
})
