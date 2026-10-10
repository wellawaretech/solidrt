// Materials on a layer, pinned against running layers: the stock
// material's surface slot over the Sprite struct, a per-sprite style
// record the stock stage forwards (set per sprite, in bulk through the
// accessor and the mirror, blank from the material's instanceStyle,
// kept through growth and through a records layer's shift), layer
// params over a prelude's uniforms (seeded, written, inherited by a
// later view), spriteSample at another uv, a custom vertex stage under
// the stock fragment and a fragment of its own over the layer's set, the
// same material on a tile layer (per-cell style, a param change
// re-baking), the components' material/params/style props, and the
// creation-time throws (the camera contract, the sampler budget, the
// reserved names). GPU state throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { createTexture } from "@solidrt/core"
import type { DecodedImage } from "@solidrt/core"
import { glsl, limits, readTexture, SCREEN_SIZE_GLSL } from "@solidrt/core/gpu"
import type { TextureBindings, TextureId } from "@solidrt/core/gpu"
import {
  addSprite,
  createAtlas,
  createRecordLayer,
  createSpriteLayer,
  createTileLayer,
  destroySprite,
  fullFrame,
  instanceAttribute,
  records,
  setInstanceStyle,
  shaderMaterial,
  shaderMaterialClass,
  Sprite,
  SpriteLayer,
  unlit,
  updateRecords,
} from "../src/index.ts"
import type { Material, ViewHandle } from "../src/index.ts"
import { SPRITE_VARYINGS, SPRITE_VERTEX, SPRITE_VERTEX_BODY, unlitFragment } from "../src/glsl.ts"

const SIZE = 64
// The atlas: one opaque white texel.
const ATLAS: DecodedImage = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// A two-texel sheet, red then blue, for the resampling test.
const RED_BLUE: DecodedImage = { width: 2, height: 1, data: new Uint8Array([255, 0, 0, 255, 0, 0, 255, 255]) }
// Squares a quarter of the view wide, centred a quarter, half and three
// quarters of the way across the view's middle row.
const SPRITE = SIZE / 4
const LEFT = SIZE / 4
const MID = SIZE / 2
const RIGHT = (3 * SIZE) / 4
const WHITE = [255, 255, 255, 255]
const GREEN = [0, 255, 0, 255]
const BLUE = [0, 0, 255, 255]
const RED = [255, 0, 0, 255]
const CLEAR = [0, 0, 0, 0]
// 8-bit rounding slack on a read-back channel.
const CHANNEL_SLACK = 1

// The palette material: a float per sprite the stock stage forwards as
// vPalette, 1 painting the sprite green whatever its texel.
const PALETTE_SURFACE = glsl`
  void surface(inout Sprite s) {
    if (vPalette > 0.5) s.color = vec4(0.0, 1.0, 0.0, 1.0) * s.color.a;
  }
`
let palette = (opts: { blend?: "add" } = {}) =>
  unlit({ ...opts, instanceBuffers: [{ attributes: [{ name: "iPalette", format: "float32" }] }], instanceStyle: [0], surface: PALETTE_SURFACE, label: "palette" })

// The flash material: a layer param mixing every sprite toward white.
const FLASH_PRELUDE = glsl`
  uniform float uFlash;
`
const FLASH_SURFACE = glsl`
  void surface(inout Sprite s) {
    s.color = mix(s.color, vec4(1.0), uFlash);
  }
`

async function mounted<T extends { texture: TextureId }>(app: TestApp, build: () => T): Promise<T> {
  let built!: T
  await app.mount(() => {
    built = build()
    return <texture src={built.texture} width={SIZE} height={SIZE} />
  })
  return built
}

function pixel(texture: TextureId, x: number, y: number): number[] {
  let { width, data } = readTexture(texture)
  let at = (y * width + x) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!]
}

/** Every channel of `got` within the rounding slack of `want`. */
function expectPixel(got: number[], want: number[]): void {
  for (let i = 0; i < 4; i++) {
    if (Math.abs(got[i]! - want[i]!) > CHANNEL_SLACK) throw new Error(`pixel [${got}] is not [${want}] within ${CHANNEL_SLACK}`)
  }
}

let square = (x: number, tint?: [number, number, number, number]) => ({ x, y: MID, w: SPRITE, h: SPRITE, ...(tint ? { tint } : {}) })

test("a per-sprite style record reaches the surface: set per sprite, blank from instanceStyle, written in bulk, kept through growth", async app => {
  let { layer, view } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "palette-atlas" })
    // Capacity 1: the second add grows every stream, the third again.
    let layer = createSpriteLayer([atlas], { capacity: 1, material: palette(), label: "palette" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "palette" })
    return { layer, view, texture: view.texture }
  })
  let a = addSprite(layer, square(LEFT))
  let b = addSprite(layer, square(MID))
  setInstanceStyle(layer, b, [1])
  let c = addSprite(layer, square(RIGHT))
  // The bulk path: stream 1 is the material's record, a Float32Array
  // over an all-float layout, slot-indexed like the layer's own.
  let mirror = records(layer, 1)
  expect(mirror instanceof Float32Array).toBe(true)
  ;(mirror as Float32Array)[c._slot] = 1
  updateRecords(layer, { stream: 1, first: c._slot, count: 1 })
  await app.frame()
  expectPixel(pixel(view.texture, LEFT, MID), WHITE)
  expectPixel(pixel(view.texture, MID, MID), GREEN)
  expectPixel(pixel(view.texture, RIGHT, MID), GREEN)
  // The accessor reads what was written, and a recycled slot starts
  // blank again.
  expect(instanceAttribute(layer, "iPalette")!.get(b._slot, 0)).toBe(1)
  expect(instanceAttribute(layer, "iMissing")).toBe(null)
  destroySprite(b)
  let d = addSprite(layer, square(MID))
  expect(d._slot).toBe(b._slot)
  await app.frame()
  expectPixel(pixel(view.texture, MID, MID), WHITE)
  expect(() => setInstanceStyle(layer, a, [1, 2])).toThrow()
  expect(() => records(layer, 2)).toThrow()
  expect(() => updateRecords(layer, { stream: 2 })).toThrow()
})

test("a packed style format encodes through its codec: a unorm8x4 tint written by attribute, read back as 0..1", async app => {
  let { layer, view } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "unorm-atlas" })
    let material = unlit({
      instanceBuffers: [{ attributes: [{ name: "iColor", format: "unorm8x4" }] }],
      instanceStyle: [1, 1, 1, 1],
      surface: glsl`
        void surface(inout Sprite s) {
          s.color *= vColor;
        }
      `,
      label: "unorm",
    })
    let layer = createRecordLayer([atlas], { capacity: 4, material, label: "unorm" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "unorm" })
    return { layer, view, texture: view.texture }
  })
  addSprite(layer, square(LEFT))
  let b = addSprite(layer, square(MID))
  let color = instanceAttribute(layer, "iColor")!
  expect(color.format).toBe("unorm8x4")
  expect(records(layer, 1) instanceof Uint8Array).toBe(true)
  color.set(b._slot, 0, 0)
  color.set(b._slot, 1, 0)
  updateRecords(layer, { stream: 1, first: b._slot, count: 1 })
  expect(color.get(b._slot, 2)).toBe(1)
  await app.frame()
  expectPixel(pixel(view.texture, LEFT, MID), WHITE)
  expectPixel(pixel(view.texture, MID, MID), BLUE)
})

test("a records layer shifts its style records with its sprites on destroy", async app => {
  let { layer, view } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "shift-atlas" })
    let layer = createRecordLayer([atlas], { capacity: 4, material: palette(), label: "shift" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "shift" })
    return { layer, view, texture: view.texture }
  })
  let a = addSprite(layer, square(LEFT))
  let b = addSprite(layer, square(MID))
  setInstanceStyle(layer, b, [1])
  await app.frame()
  expectPixel(pixel(view.texture, LEFT, MID), WHITE)
  expectPixel(pixel(view.texture, MID, MID), GREEN)
  destroySprite(a)
  await app.frame()
  expect(b._slot).toBe(0)
  expectPixel(pixel(view.texture, LEFT, MID), CLEAR)
  expectPixel(pixel(view.texture, MID, MID), GREEN)
})

test("layer params drive a prelude's uniforms: seeded at creation, written later, inherited by a view created after", async app => {
  let { layer, view } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "flash-atlas" })
    let material = unlit({ prelude: FLASH_PRELUDE, surface: FLASH_SURFACE, params: { uFlash: 0 }, label: "flash" })
    let layer = createSpriteLayer([atlas], { capacity: 4, material, label: "flash" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "flash" })
    addSprite(layer, square(MID, [1, 0, 0, 1]))
    return { layer, view, texture: view.texture }
  })
  await app.frame()
  expectPixel(pixel(view.texture, MID, MID), RED)
  layer.setParams({ uFlash: 1 })
  await app.frame()
  expectPixel(pixel(view.texture, MID, MID), WHITE)
  let later = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "flash-later" })
  await app.frame()
  expectPixel(pixel(later.texture, MID, MID), WHITE)
  expect(() => layer.setParams({ uFlash: Number.NaN })).toThrow()
})

test("spriteSample resamples the fragment's own atlas at another uv, clamped into the frame", async app => {
  let { view } = await mounted(app, () => {
    let atlas = createAtlas(RED_BLUE, { filter: "nearest", label: "red-blue" })
    // Every fragment samples three quarters of the way across its frame:
    // the blue texel, wherever the fragment sits.
    let material = unlit({
      surface: glsl`
        void surface(inout Sprite s) {
          s.color = spriteSample(mix(s.frame.xy, s.frame.zw, vec2(0.75, 0.5)));
        }
      `,
      label: "resample",
    })
    let layer = createSpriteLayer([atlas], { capacity: 4, material, label: "resample" })
    let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "resample" })
    addSprite(layer, { ...square(MID), frame: fullFrame(atlas) })
    return { view, texture: view.texture }
  })
  await app.frame()
  expectPixel(pixel(view.texture, MID - SPRITE / 4, MID), BLUE)
  expectPixel(pixel(view.texture, MID + SPRITE / 4, MID), BLUE)
})

test("a custom vertex stage under the stock fragment, and a fragment of its own over the layer's set", async app => {
  // Tier 1: the stock placement shifted a quarter view to the right.
  let shifted = shaderMaterialClass({
    vertex: glsl`
      in vec2 aPos;
      in vec2 iPos;
      in float iRot;
      in vec2 iScale;
      in vec4 iUv;
      in vec4 iTint;
      in vec2 iScreenPx;
      in float iAtlas;
      in vec4 iOutline;
      ${SPRITE_VARYINGS}
      ${SCREEN_SIZE_GLSL}
      void main() {
        float clampScale = screenSizeScale(iScale, uCamera.z, iScreenPx);
        vec2 corner = aPos * iScale * clampScale;
        vec2 center = iPos + vec2(${(SIZE / 4).toFixed(1)}, 0.0);
        float rot = iRot;
        ${SPRITE_VERTEX_BODY}
      }
    `,
    fragment: unlitFragment(),
    label: "shifted",
  })
  // Tier 3: blue wherever the sprite covers, through the layer tint.
  let blue = shaderMaterial({
    vertex: SPRITE_VERTEX,
    fragment: glsl`
      void main() {
        Sprite s = spriteOf();
        fragColor = vec4(0.0, 0.0, 1.0, 1.0) * s.color.a * uTint;
      }
    `,
    label: "blue",
  })
  let { left, right } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "custom-atlas" })
    let a = createSpriteLayer([atlas], { capacity: 4, material: shifted.instance(), label: "shifted" })
    let b = createSpriteLayer([atlas], { capacity: 4, material: blue, label: "blue" })
    let left = a.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "shifted" })
    let right = b.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label: "blue" })
    addSprite(a, square(LEFT))
    addSprite(b, square(MID))
    return { left, right, texture: left.texture }
  })
  await app.frame()
  expectPixel(pixel(left.texture, LEFT, MID), CLEAR)
  expectPixel(pixel(left.texture, MID, MID), WHITE)
  expectPixel(pixel(right.texture, MID, MID), BLUE)
  shifted.dispose()
  blue.dispose!()
})

test("a tile layer bakes with the material: a per-cell style, and a param change re-bakes every resident chunk", async app => {
  let { layer } = await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "tiles-palette-atlas" })
    let material = unlit({
      prelude: FLASH_PRELUDE,
      instanceBuffers: [{ attributes: [{ name: "iPalette", format: "float32" }] }],
      instanceStyle: [0],
      surface: glsl`
        void surface(inout Sprite s) {
          if (vPalette > 0.5) s.color = vec4(0.0, 1.0, 0.0, 1.0) * s.color.a;
          s.color = mix(s.color, vec4(1.0), uFlash);
        }
      `,
      params: { uFlash: 0 },
      label: "tiles-palette",
    })
    let layer = createTileLayer(2, 2, 8, 8, [atlas], { material, label: "tiles-palette" })
    layer.setTile(0, 0, fullFrame(atlas), { style: [1] })
    layer.setTile(1, 0, fullFrame(atlas))
    layer.setTiles(0, 1, 2, 1, [fullFrame(atlas), fullFrame(atlas)], { style: [1] })
    return { layer, texture: layer.chunks[0]!.texture }
  })
  await app.frame()
  let chunk = layer.chunks[0]!.texture
  expectPixel(pixel(chunk, 4, 4), GREEN)
  expectPixel(pixel(chunk, 12, 4), WHITE)
  expectPixel(pixel(chunk, 4, 12), GREEN)
  expectPixel(pixel(chunk, 12, 12), GREEN)
  layer.setParams({ uFlash: 1 })
  await app.frame()
  expectPixel(pixel(chunk, 4, 4), WHITE)
  expectPixel(pixel(chunk, 12, 12), WHITE)
  let plain = createTileLayer(2, 2, 8, 8, layer.atlases as never, { label: "tiles-plain" })
  expect(() => plain.setTile(0, 0, fullFrame(layer.atlases[0]!), { style: [1] })).toThrow()
})

test("the components carry the material, the params and a sprite's style", async app => {
  let view!: ViewHandle
  let atlas!: ReturnType<typeof createAtlas>
  let material!: Material
  await app.mount(() => {
    atlas = createAtlas(ATLAS, { label: "component-atlas" })
    material = unlit({
      prelude: FLASH_PRELUDE,
      instanceBuffers: [{ attributes: [{ name: "iPalette", format: "float32" }] }],
      instanceStyle: [0],
      surface: glsl`
        void surface(inout Sprite s) {
          if (vPalette > 0.5) s.color = vec4(0.0, 1.0, 0.0, 1.0) * s.color.a;
          s.color = mix(s.color, vec4(1.0), uFlash);
        }
      `,
      label: "component",
    })
    return (
      <SpriteLayer width={SIZE} height={SIZE} atlases={[atlas]} material={material} params={{ uFlash: 0 }} clearColor={[0, 0, 0, 0]} viewRef={v => (view = v)} label="component">
        <Sprite x={LEFT} y={MID} w={SPRITE} h={SPRITE} />
        <Sprite x={MID} y={MID} w={SPRITE} h={SPRITE} style={[1]} />
      </SpriteLayer>
    )
  })
  await app.frame()
  expectPixel(pixel(view.texture, LEFT, MID), WHITE)
  expectPixel(pixel(view.texture, MID, MID), GREEN)
})

test("creation throws name the mistake: the camera contract, reserved names, the sampler budget, a wrong style length", async app => {
  let textures!: TextureBindings
  await mounted(app, () => {
    let atlas = createAtlas(ATLAS, { label: "throws-atlas" })
    textures = {}
    for (let i = 0; i < limits.maxTextureUnits; i++) textures[`uLut${i}`] = createTexture(ATLAS.data, 1, 1, { label: `lut${i}` })
    let layer = createSpriteLayer([atlas], { capacity: 4, label: "throws" })
    // A vertex stage without the camera mapping cannot place sprites.
    expect(() => shaderMaterialClass({ vertex: "void main() { gl_Position = vec4(0.0); }", fragment: unlitFragment() })).toThrow()
    // The layer's own record names are taken.
    expect(() => shaderMaterialClass({ vertex: SPRITE_VERTEX, fragment: unlitFragment(), instanceBuffers: [{ attributes: [{ name: "iPos", format: "float32x2" }] }] })).toThrow()
    // The stock stage forwards iSomething as vSomething, nothing else.
    expect(() => unlit({ instanceBuffers: [{ attributes: [{ name: "palette", format: "float32" }] }] })).toThrow()
    // A style record is one value per component.
    expect(() => unlit({ instanceBuffers: [{ attributes: [{ name: "iPalette", format: "float32x2" }] }], instanceStyle: [0] })).toThrow()
    // An atlas sampler's name is the layer's.
    expect(() => unlit({ textures: { uAtlas0: atlas.texture } })).toThrow()
    // The material's textures and the atlases share one budget.
    let hungry = unlit({ textures, label: "hungry" })
    expect(() => createSpriteLayer([atlas], { material: hungry, label: "over-budget" })).toThrow()
    expect(() => createRecordLayer([atlas], { material: hungry, label: "over-budget-records" })).toThrow()
    expect(() => createTileLayer(1, 1, 8, 8, [atlas], { material: hungry, label: "over-budget-tiles" })).toThrow()
    // No instance buffer, no style record.
    expect(() => setInstanceStyle(layer, addSprite(layer, square(MID)), [1])).toThrow()
    let view = layer.createView({ width: SIZE, height: SIZE, label: "throws" })
    return { texture: view.texture }
  })
})
