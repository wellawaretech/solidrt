// Hull layers against quad layers: a layer over an atlas created with
// `mesh: "hull"` draws each sprite as its frame's mesh, and must render the
// same image as a quad layer over the same texture with the stock
// material - for flipped sprites, rotated and screen-clamped ones, a
// distance-field sprite with an outline, a mipmapped sheet drawn small and
// a nearest sheet drawn large, on the node layer and the record layer. A
// solid material (which ignores coverage) shows the mesh itself: fewer
// pixels than the quad, and the full quad again for a record whose mesh
// field is zero. GPU state throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { DecodedImage } from "@solidrt/core"
import { glsl, readTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { addSprite, createAtlas, createRecordLayer, createSpriteLayer, extrudeGrid, getSprite, grid, INSTANCE_FLOATS, MESH_FLOATS, records, setSprite, shaderMaterial, shaderMaterialClass, updateRecords } from "../src/index.ts"
import type { Atlas, AtlasOptions, Frame, GridOptions, Material, SpriteOptions } from "../src/index.ts"
import { SPRITE_VARYINGS, SPRITE_VERTEX, unlitFragment } from "../src/glsl.ts"

const SIZE = 128
// The sheet's cells: two 32 px cells side by side.
const CELL = 32
// Supersamples per texel axis for the shapes' antialiased edges.
const SUPERSAMPLE = 4
// The extruded sheet's gutter: a power of two dividing the cell, so mip
// levels 0 through 4 sample clean (a 3 px sprite of a 32 px cell reads
// levels 3 and 4).
const GUTTER = 16
// The distance field's range in texels (Atlas.sdf.range).
const FIELD_RANGE = 6
// 8-bit rounding slack on a read-back channel: the hull and the quad
// interpolate the same affine uv from different vertices.
const CHANNEL_SLACK = 2
// Float offset of the outline in a record (records.ts).
const OUTLINE_FIELD = 16
// The solid material's painted pixels on a hull layer, at most this share
// of the quad layer's: the blob covers about half its frame.
const HULL_SHARE = 0.8

/** Two 32 px cells of premultiplied RGBA8: an asymmetric blob (two
 * ellipses) with a colour gradient, and a thin sloped sliver with a long
 * tip. Edges antialiased by supersampling. */
function shapes(): DecodedImage {
  let width = CELL * 2
  let data = new Uint8Array(width * CELL * 4)
  let inBlob = (x: number, y: number) => ((x - 12) / 9) ** 2 + ((y - 14) / 11) ** 2 <= 1 || (x - 22) ** 2 + (y - 23) ** 2 <= 25
  let inSliver = (x: number, y: number) => Math.abs(y - (4 + x * 0.8)) < 1.5 + x * 0.05
  for (let y = 0; y < CELL; y++) {
    for (let x = 0; x < width; x++) {
      let local = x % CELL
      let hits = 0
      for (let sy = 0; sy < SUPERSAMPLE; sy++) {
        for (let sx = 0; sx < SUPERSAMPLE; sx++) {
          let px = local + (sx + 0.5) / SUPERSAMPLE
          let py = y + (sy + 0.5) / SUPERSAMPLE
          if (x < CELL ? inBlob(px, py) : inSliver(px, py)) hits++
        }
      }
      let a = hits / SUPERSAMPLE ** 2
      let i = (y * width + x) * 4
      data[i] = Math.round(local * 8 * a)
      data[i + 1] = Math.round(y * 8 * a)
      data[i + 2] = Math.round(128 * a)
      data[i + 3] = Math.round(255 * a)
    }
  }
  return { width, height: CELL, data }
}

/** A distance field of a disc, radius 9 in a 32 px cell, the same value
 * in every channel: 0.5 at the edge, `FIELD_RANGE` texels from 0 to 1. */
function field(): DecodedImage {
  let data = new Uint8Array(CELL * CELL * 4)
  for (let y = 0; y < CELL; y++) {
    for (let x = 0; x < CELL; x++) {
      let d = Math.hypot(x + 0.5 - 13, y + 0.5 - 15) - 9
      let v = Math.round(Math.min(1, Math.max(0, 0.5 - d / FIELD_RANGE)) * 255)
      data.set([v, v, v, v], (y * CELL + x) * 4)
    }
  }
  return { width: CELL, height: CELL, data }
}

async function mounted<T extends { texture: TextureId }>(app: TestApp, build: () => T): Promise<T> {
  let built!: T
  await app.mount(() => {
    built = build()
    return <texture src={built.texture} width={SIZE} height={SIZE} />
  })
  return built
}

type Kind = "node" | "record"
type Case = {
  image: DecodedImage
  /** The grid over the sheet, when it is not CELL-sized cells in a row
   * (an extruded sheet's options). */
  slice?: { cols: number; options: GridOptions }
  atlas?: AtlasOptions
  kind: Kind
  material?: () => Material
  /** The sprites, `frame` an index into the sheet's cells. */
  sprites: (Omit<SpriteOptions, "frame"> & { frame: number; outline?: [number, number, number, number] })[]
  camera?: { x?: number; y?: number; zoom?: number; rotation?: number }
}

/** The same sprites drawn by a hull layer and by a quad layer over the
 * same texture (an atlas literal without the mesh): both read-backs. */
async function drawBoth(app: TestApp, c: Case): Promise<{ hull: Uint8Array; quad: Uint8Array }> {
  let views = await mounted(app, () => {
    let meshed = createAtlas(c.image, { ...c.atlas, mesh: "hull", label: "hull-atlas" })
    let plain: Atlas = { texture: meshed.texture, width: meshed.width, height: meshed.height, filter: meshed.filter, sdf: meshed.sdf }
    let draw = (atlas: Atlas, label: string) => {
      let frames = c.slice ? grid(atlas, c.slice.cols, 1, c.slice.options) : grid(atlas, c.image.width / CELL, 1)
      let opts = { capacity: 8, label, material: c.material?.() }
      let layer = c.kind === "node" ? createSpriteLayer([atlas], opts) : createRecordLayer([atlas], opts)
      let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label })
      if (c.camera) view.setCamera(c.camera)
      for (let s of c.sprites) {
        let { outline, ...rest } = s
        let sprite = addSprite(layer, { ...rest, frame: frames[s.frame]! })
        if (outline !== undefined) {
          let r = records(layer)
          let stride = INSTANCE_FLOATS + (atlas.mesh ? MESH_FLOATS : 0)
          r.set(outline, sprite._slot * stride + OUTLINE_FIELD)
          updateRecords(layer)
        }
      }
      return view
    }
    let hull = draw(meshed, "hull")
    let quad = draw(plain, "quad")
    return { hull, quad, texture: hull.texture }
  })
  await app.frame()
  return { hull: readTexture(views.hull.texture).data, quad: readTexture(views.quad.texture).data }
}

/** Every pixel of the two images within the rounding slack, and the image
 * not empty. */
function expectSame(what: string, hull: Uint8Array, quad: Uint8Array): void {
  let drawn = 0
  let worst = 0
  let at = -1
  for (let i = 0; i < quad.length; i++) {
    let d = Math.abs(hull[i]! - quad[i]!)
    if (d > worst) (worst = d), (at = i)
    if (i % 4 === 3 && quad[i]! > 0) drawn++
  }
  if (drawn === 0) throw new Error(`${what}: the quad layer drew nothing`)
  if (worst > CHANNEL_SLACK) {
    let p = at >> 2
    throw new Error(`${what}: pixel (${p % SIZE}, ${Math.floor(p / SIZE)}) differs by ${worst}: hull [${[...hull.subarray(p * 4, p * 4 + 4)]}] quad [${[...quad.subarray(p * 4, p * 4 + 4)]}]`)
  }
}

function painted(image: Uint8Array): number {
  let n = 0
  for (let i = 3; i < image.length; i += 4) if (image[i]! > 0) n++
  return n
}

// Placements with fractional positions, every flip, rotations and a
// screen-size clamp, drawn large enough to show the mesh's edge.
const POSED: Case["sprites"] = [
  { frame: 0, x: 30.3, y: 30.7, w: 48, h: 48 },
  { frame: 0, x: 94.6, y: 30.2, w: 48, h: 48, flipX: true },
  { frame: 0, x: 30.5, y: 94.4, w: 44, h: 52, flipY: true, rotation: 0.6 },
  { frame: 1, x: 94.1, y: 94.8, w: 50, h: 46, flipX: true, flipY: true, rotation: -1.1 },
  { frame: 1, x: 64.2, y: 64.6, w: 6, h: 6, minScreenPx: 30 },
]

for (let kind of ["node", "record"] as const) {
  test(`a ${kind} hull layer renders what a quad layer renders: flips, rotation, the screen-size clamp`, async app => {
    let { hull, quad } = await drawBoth(app, { image: shapes(), kind, sprites: POSED })
    expectSame(`${kind} posed`, hull, quad)
  })
}

test("a rotated, zoomed camera: the same image", async app => {
  let { hull, quad } = await drawBoth(app, { image: shapes(), kind: "node", sprites: POSED, camera: { x: 10, y: -6, zoom: 1.3, rotation: 0.4 } })
  expectSame("camera", hull, quad)
})

test("a distance-field sprite with an outline: the push covers the outline", async app => {
  let { hull, quad } = await drawBoth(app, {
    image: field(),
    atlas: { sdf: { range: FIELD_RANGE } },
    kind: "record",
    sprites: [
      { frame: 0, x: 40.4, y: 40.6, w: 64, h: 64, tint: [1, 0.8, 0.2, 1], outline: [0.1, 0.2, 0.9, 4] },
      { frame: 0, x: 96.3, y: 92.2, w: 40, h: 40, tint: [0.2, 1, 0.4, 1], outline: [1, 0, 0, 2], flipX: true },
    ],
  })
  expectSame("sdf outline", hull, quad)
})

// Every size from a few pixels up to the cell's own, at fractional
// positions and a turn, over the mipmapped sheet: each a different blend
// of mip levels.
const SWEEP_SIZES = [3, 4, 5, 6, 7, 9, 11, 13, 16, 20, 26, 32]

test("a mipmapped sheet drawn small: the same image at every minification", async app => {
  let sprites: Case["sprites"] = SWEEP_SIZES.map((size, i) => ({
    frame: i % 2,
    x: 12.3 + (i % 4) * 32,
    y: 16.6 + Math.floor(i / 4) * 40,
    w: size,
    h: size * 1.2,
    rotation: i * 0.37,
    flipX: i % 3 === 0,
  }))
  // Extruded, as a mipmapped sheet must be: un-extruded, a coarse level
  // bleeds the neighbour cell into the frame's edge, which the quad
  // shows and the hull (rightly) does not.
  let extruded = extrudeGrid(shapes(), 2, 1, GUTTER)
  let { hull, quad } = await drawBoth(app, { image: extruded.image, slice: { cols: 2, options: extruded.options }, atlas: { mipmap: true }, kind: "node", sprites })
  expectSame("mipmapped small", hull, quad)
})

test("a nearest sheet drawn large: the same image", async app => {
  let { hull, quad } = await drawBoth(app, {
    image: shapes(),
    atlas: { filter: "nearest" },
    kind: "record",
    sprites: [
      { frame: 0, x: 60.4, y: 62.3, w: 120, h: 120, rotation: 0.2 },
      { frame: 1, x: 70.2, y: 66.7, w: 90, h: 90, flipY: true },
    ],
  })
  expectSame("nearest large", hull, quad)
})

// A solid fill that ignores coverage: what it paints is the drawn
// geometry, the frame mask's discard included.
let solid = () =>
  shaderMaterial({
    vertex: SPRITE_VERTEX,
    fragment: glsl`
      void main() {
        Sprite s = spriteOf();
        fragColor = vec4(1.0);
      }
    `,
    label: "solid",
  })

// The solid sprite both tests below draw: the blob, large.
const SOLID_SPRITE = { frame: 0, x: 64.3, y: 64.6, w: 100, h: 100 }

test("the mesh shades fewer pixels than the quad", async app => {
  let { hull, quad } = await drawBoth(app, { image: shapes(), kind: "record", material: solid, sprites: [SOLID_SPRITE] })
  let quadPixels = painted(quad)
  let hullPixels = painted(hull)
  expect(quadPixels > 0).toBe(true)
  expect(hullPixels > 0 && hullPixels < quadPixels * HULL_SHARE).toBe(true)
})

test("a zero mesh field (a raw writer that never wrote one) draws the full quad", async app => {
  let stride = INSTANCE_FLOATS + MESH_FLOATS
  let { meshed, zeroed } = await mounted(app, () => {
    let atlas = createAtlas(shapes(), { mesh: "hull", label: "raw-atlas" })
    let frame = grid(atlas, 2, 1)[0]!
    let view = (label: string, zero: boolean) => {
      let layer = createRecordLayer([atlas], { capacity: 2, material: solid(), label })
      let v = layer.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 0], label })
      let sprite = addSprite(layer, { ...SOLID_SPRITE, frame })
      if (zero) {
        records(layer).fill(0, sprite._slot * stride + INSTANCE_FLOATS, (sprite._slot + 1) * stride)
        updateRecords(layer)
      }
      return v
    }
    let meshed = view("raw-meshed", false)
    let zeroed = view("raw-zeroed", true)
    return { meshed, zeroed, texture: meshed.texture }
  })
  await app.frame()
  let meshedPixels = painted(readTexture(meshed.texture).data)
  let zeroedPixels = painted(readTexture(zeroed.texture).data)
  // The quad of a 100 px sprite at a fractional position: 100 x 100.
  expect(zeroedPixels).toBe(100 * 100)
  expect(meshedPixels < zeroedPixels * HULL_SHARE).toBe(true)
})

test("getSprite reads the mesh back, and a flip keeps it", async app => {
  let frames!: Frame[]
  let { node, record } = await mounted(app, () => {
    let atlas = createAtlas(shapes(), { mesh: "hull", label: "read-atlas" })
    frames = grid(atlas, 2, 1)
    let node = createSpriteLayer([atlas], { capacity: 2, label: "read-node" })
    let record = createRecordLayer([atlas], { capacity: 2, label: "read-record" })
    let view = node.createView({ width: SIZE, height: SIZE, label: "read" })
    return { node, record, texture: view.texture }
  })
  for (let layer of [node, record]) {
    let sprite = addSprite(layer, { x: 10, y: 10, w: 20, h: 20, frame: frames[1]! })
    let back = getSprite(sprite)!.frame
    expect([...back.mesh!.packed]).toEqual([...frames[1]!.mesh!.packed])
    setSprite(sprite, { flipX: true })
    expect([...getSprite(sprite)!.frame.mesh!.packed]).toEqual([...frames[1]!.mesh!.packed])
    // A frame read back writes back whole.
    setSprite(sprite, { frame: frames[0]! })
    setSprite(sprite, { frame: back })
    expect([...getSprite(sprite)!.frame.mesh!.packed]).toEqual([...frames[1]!.mesh!.packed])
  }
})

test("a custom vertex stage that reads aPos throws, naming spriteCorner", () => {
  let threw = ""
  try {
    shaderMaterialClass({
      vertex: glsl`
        in vec2 aPos;
        ${SPRITE_VARYINGS}
        void main() { gl_Position = vec4(aPos * uViewport * uCamera.z * uCameraRot.x, 0.0, 1.0); }
      `,
      fragment: unlitFragment(),
    })
  } catch (e) {
    threw = String(e)
  }
  expect(threw.includes("spriteCorner")).toBe(true)
})
