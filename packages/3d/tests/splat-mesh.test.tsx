// A splat cloud on the GPU: the record block parsed at bake scale, handed
// to the engine as its key column (the centers: what the depth sort reads
// and all the engine keeps of the records) and uploaded in place as the
// record texture, then painted back to front - the probe is the frame
// itself (`app.painted`), as records.test.tsx reads it. Two opaque splats
// over the view's center: the nearer one is drawn last, so its color is
// the pixel; the camera crossing to the other side re-sorts the engine's
// key column and the other splat wins; a count of 0 draws nothing until
// setRecordCount lifts the budget. A cloud above the engine's in-place
// sort size takes the same path through the sort thread: the ids land a
// sort later, which `settle` waits out. The engine's selection by chunk
// shows the same way: the LOD threshold drops a small splat and the
// pixel shows the one behind it, and a chunk behind the camera leaves
// the draw range the GPU inventory reports.

import { test, expect } from "@solidrt/test"
import type { Pixels, RefLocator, TestApp } from "@solidrt/test"
import { add, createScene, createSplatMesh, disposeInstances, parseSplat, setRecordCount, setRecordLod, SPLAT_CHUNK_MAX, SPLAT_GROUP } from "../src/index.ts"
import type { SceneHandle, SplatMeshNode, SplatShEncoding } from "../src/index.ts"

const SIZE = 64
// The camera stands this far from the origin on the z axis, either side.
const DISTANCE = 10
// The near splat sits this far toward +z of the far one (at the origin).
const NEAR_Z = 2
// A channel reads at least this to count as the splat's color: the pixel
// center sits half a pixel off the splat center, so the gaussian leaves a
// sliver of the one beneath.
const SOLID = 240
const FAINT = 16
const RED = [255, 0, 0, 255]
const BLUE = [0, 0, 255, 255]
// Above the engine's in-place sort size (16384 keys): the cloud's re-sort
// runs on the sort thread. The filler splats sit far off to the side,
// invisible, so the two at the center decide the pixel.
const LARGE_CLOUD = 20000
const FILLER_X = 1000
// The near splat's scale in the LOD test: at the camera's distance it
// projects to about two pixels of sigma (solid at the center pixel) and
// to NEAR_CONTRIBUTION pixels of contribution (its area times its
// opacity at the chunk's projected scale), between the thresholds below.
const NEAR_SCALE = 0.3
// A threshold the near splat falls under (its contribution is ~4 px)
// while the unit splat behind it (~48 px) clears it.
const LOD_CUT = 10
// Fillers past this many make a second chunk, whose box lies off to the
// side of the view and is culled whole.
const CULLED_CHUNK = SPLAT_CHUNK_MAX
// Frames a settle over a camera crossing may run: the one whose flush
// submits the re-sort and the one that lands its ids.
const SORT_LANDING_FRAMES = 2

// A .splat record: position, linear scale (1 unless given), color
// bytes, quaternion (w, x, y, z) quantized v * 128 + 128; the identity
// rotation at the largest w the byte holds.
const IDENTITY = [127 / 128, 0, 0, 0]
function dotSplat(splats: { p: number[]; c: number[]; s?: number }[]): Uint8Array {
  let out = new Uint8Array(splats.length * 32)
  let dv = new DataView(out.buffer)
  splats.forEach((g, i) => {
    for (let k = 0; k < 3; k++) {
      dv.setFloat32(i * 32 + k * 4, g.p[k]!, true)
      dv.setFloat32(i * 32 + 12 + k * 4, g.s ?? 1, true)
    }
    for (let k = 0; k < 4; k++) {
      out[i * 32 + 24 + k] = g.c[k]!
      out[i * 32 + 28 + k] = Math.round(IDENTITY[k]! * 128 + 128)
    }
  })
  return out
}

// A gunzipped .spz (version 2) of `splats` at SH degree 1: positions as
// 24-bit fixed point at 12 fractional bits, opacity 255, the DC band at
// zero (base color 0.5), unit scales, the identity rotation, and nine SH
// bytes per splat (coefficient-major, rgb interleaved: v * 128 + 128).
// y-up like every spz, so nothing flips.
function spzSh1(splats: { p: number[]; sh: number[] }[]): Uint8Array {
  let n = splats.length
  let out = new Uint8Array(16 + n * (9 + 1 + 3 + 3 + 3 + 9))
  let dv = new DataView(out.buffer)
  dv.setUint32(0, 0x5053474e, true)
  dv.setUint32(4, 2, true)
  dv.setUint32(8, n, true)
  out[12] = 1
  out[13] = 12
  let positions = 16
  let alphas = positions + n * 9
  let colors = alphas + n
  let scales = colors + n * 3
  let rotations = scales + n * 3
  let sh = rotations + n * 3
  splats.forEach((g, i) => {
    for (let k = 0; k < 3; k++) {
      let fixed = Math.round(g.p[k]! * 4096)
      out[positions + (i * 3 + k) * 3] = fixed & 255
      out[positions + (i * 3 + k) * 3 + 1] = (fixed >> 8) & 255
      out[positions + (i * 3 + k) * 3 + 2] = (fixed >> 16) & 255
      // DC 0: byte 0.5 * 255; log scale 0: (0 + 10) * 16; rotation x, y, z 0.
      out[colors + i * 3 + k] = 128
      out[scales + i * 3 + k] = 160
      out[rotations + i * 3 + k] = 128
    }
    out[alphas + i] = 255
    for (let j = 0; j < 9; j++) out[sh + i * 9 + j] = Math.round(g.sh[j]! * 128 + 128)
  })
  return out
}

function scene(): SceneHandle {
  let s = createScene(SIZE, SIZE, { blendSpace: "display", clearColor: [0, 0, 0, 1], label: "splat-mesh" })
  s.setCamera({ position: [0, 0, DISTANCE], target: [0, 0, 0] })
  return s
}

async function mounted<T extends { scene: SceneHandle }>(app: TestApp, build: () => T): Promise<T & { leaf: RefLocator }> {
  let leaf = app.ref()
  let w!: T
  await app.mount(() => {
    w = build()
    return <texture ref={leaf} src={w.scene.texture} width={SIZE} height={SIZE} />
  })
  return { ...w, leaf }
}

/** The present index of the latest frame drawn. */
function frame(app: TestApp): number {
  return (app.stats() as { frame: number }).frame
}

function center(pixels: Pixels): number[] {
  let i = ((SIZE / 2) * SIZE + SIZE / 2) * 4
  return [pixels.data[i]!, pixels.data[i + 1]!, pixels.data[i + 2]!, pixels.data[i + 3]!]
}

function expectColor(pixel: number[], color: number[]) {
  for (let k = 0; k < 3; k++) {
    if (color[k] === 255) expect(pixel[k]!).toBeGreaterThanOrEqual(SOLID)
    else expect(pixel[k]!).toBeLessThan(FAINT)
  }
  expect(pixel[3]).toBe(255)
}

test("a splat cloud paints back to front from the engine's key column, re-sorted as the camera crosses, after the count admits it", async app => {
  let { scene: s, mesh, leaf } = await mounted(app, () => {
    let s = scene()
    let data = parseSplat(dotSplat([{ p: [0, 0, 0], c: RED }, { p: [0, 0, NEAR_Z], c: BLUE }]), { keepOrientation: true })
    let mesh: SplatMeshNode = createSplatMesh(data, { count: 0, label: "splat-mesh" })
    add(s.root, mesh)
    return { scene: s, mesh }
  })
  // Nothing published at count 0.
  expect(center(await app.painted(leaf))).toEqual([0, 0, 0, 255])
  setRecordCount(mesh, 2)
  // The near (blue) splat is drawn last from +z.
  expectColor(center(await app.painted(leaf)), BLUE)
  // From -z the far (red) one is nearer: the engine re-sorts its column.
  s.setCamera({ position: [0, 0, -DISTANCE], target: [0, 0, 0] })
  expectColor(center(await app.painted(leaf)), RED)
  disposeInstances(mesh)
})

// The SH band-1 basis constant, and the second band-1 coefficient's
// red value: the shader adds SH_C1 * z * c1 to the color for the view
// direction d (splat center minus camera), so from +z (d.z = -1) the red
// rises by -SH_C1 * SH_RED and from -z it drops by the same.
const SH_C1 = 0.4886025119029199
const SH_RED = -0.5
// The base color byte spz's DC byte 128 decodes to (0.5 + SH_C0 * dc with
// dc = (128 / 255 - 0.5) / 0.15), as the shader sees it.
const SH_BASE = 0.5 + 0.28209479177387814 * ((128 / 255 - 0.5) / 0.15)
// The center pixel's alpha spans this: the gaussian cap (0.99) down to
// what the existing tests accept as solid (SOLID / 255).
const ALPHA_MAX = 0.99
// A pixel read within this many steps of its prediction agrees.
const SH_TOLERANCE = 3

// The predicted center pixel channel for a base value plus an SH shift,
// over black at an alpha between SOLID / 255 and ALPHA_MAX: the range of
// bytes it may read.
let predicted = (value: number): [number, number] => [Math.floor(value * 255 * (SOLID / 255)) - SH_TOLERANCE, Math.ceil(value * 255 * ALPHA_MAX) + SH_TOLERANCE]
let expectWithin = (byte: number, range: [number, number]) => {
  expect(byte).toBeGreaterThanOrEqual(range[0])
  expect(byte).toBeLessThanOrEqual(range[1])
}

for (let encoding of ["u8", "pq"] as SplatShEncoding[]) {
  test(`the SH bands shade a splat by view direction under the ${encoding} encoding`, async app => {
    // The center splat carries the band-1 coefficient; a second splat far
    // off screen carries a different vector, so under pq the two labels
    // and palette entries differ and a crossed label would show.
    let center1 = [0, 0, 0, SH_RED, 0, 0, 0, 0, 0]
    let other = [0.25, -0.25, 0.5, 0, 0.5, 0, -0.5, 0, 0.25]
    let { scene: s, mesh, leaf } = await mounted(app, () => {
      let s = scene()
      let data = parseSplat(spzSh1([{ p: [0, 0, 0], sh: center1 }, { p: [FILLER_X, 0, 0], sh: other }]), { shDegree: 1, shEncoding: encoding })
      expect(data.shEncoding).toBe(encoding)
      let mesh: SplatMeshNode = createSplatMesh(data, { label: "splat-sh-" + encoding })
      add(s.root, mesh)
      return { scene: s, mesh }
    })
    let front = center(await app.painted(leaf))
    expectWithin(front[0]!, predicted(SH_BASE - SH_C1 * SH_RED))
    expectWithin(front[1]!, predicted(SH_BASE))
    expectWithin(front[2]!, predicted(SH_BASE))
    s.setCamera({ position: [0, 0, -DISTANCE], target: [0, 0, 0] })
    let back = center(await app.painted(leaf))
    expectWithin(back[0]!, predicted(SH_BASE + SH_C1 * SH_RED))
    expectWithin(back[1]!, predicted(SH_BASE))
    expect(front[0]! - back[0]!).toBeGreaterThan(-2 * SH_C1 * SH_RED * SOLID - 2 * SH_TOLERANCE)
    disposeInstances(mesh)
  })
}

test("the LOD threshold drops a small splat before a large one, and 0 draws both", async app => {
  let { mesh, leaf } = await mounted(app, () => {
    let s = scene()
    // The small blue splat is nearer: drawn last, it wins the pixel -
    // unless the threshold cuts it, when the red one behind shows.
    let data = parseSplat(dotSplat([{ p: [0, 0, 0], c: RED }, { p: [0, 0, NEAR_Z], c: BLUE, s: NEAR_SCALE }]), { keepOrientation: true })
    let mesh: SplatMeshNode = createSplatMesh(data, { lodThreshold: LOD_CUT, label: "splat-mesh-lod" })
    add(s.root, mesh)
    return { scene: s, mesh }
  })
  expectColor(center(await app.painted(leaf)), RED)
  setRecordLod(mesh, 0)
  expectColor(center(await app.painted(leaf)), BLUE)
  setRecordLod(mesh, LOD_CUT)
  expectColor(center(await app.painted(leaf)), RED)
  disposeInstances(mesh)
})

test("a chunk behind the view is culled from the draw range; the budget caps it", async app => {
  let { mesh, leaf } = await mounted(app, () => {
    let s = scene()
    // Two chunks: the first holds the two center splats and the fillers
    // nearest them, the second the rest of the fillers, off to the side
    // and out of view.
    let splats = [{ p: [0, 0, 0], c: RED }, { p: [0, 0, NEAR_Z], c: BLUE }]
    for (let i = 2; i < CULLED_CHUNK + 40; i++) splats.push({ p: [FILLER_X + i, 0, 0], c: RED })
    let data = parseSplat(dotSplat(splats), { keepOrientation: true })
    expect(data.runs.length).toBe(2)
    let mesh: SplatMeshNode = createSplatMesh(data, { lodThreshold: 0, label: "splat-mesh-cull" })
    add(s.root, mesh)
    return { scene: s, mesh }
  })
  expectColor(center(await app.painted(leaf)), BLUE)
  // The draw range is the selection's: the first chunk's records in
  // groups of SPLAT_GROUP, not the cloud's.
  expect(instancesDrawn(app)).toBe(Math.ceil(CULLED_CHUNK / SPLAT_GROUP))
  // A budget under the visible chunk thins it to the budget.
  setRecordCount(mesh, SPLAT_GROUP * 2)
  await app.painted(leaf)
  expect(instancesDrawn(app)).toBe(2)
  disposeInstances(mesh)
})

// The instance count of the scene's splat entry as the raster thread
// holds it (the GPU inventory of the scene target).
function instancesDrawn(app: TestApp): number {
  let targets = app.gpu({ label: "splat-mesh" }).pipelines as { kind: string; draws?: { label?: string; instanceCount?: number }[] }[]
  let view = targets.find(t => t.kind === "draws")
  let entry = view?.draws?.find(d => d.label === "splat-quads")
  if (entry === undefined || entry.instanceCount === undefined) throw new Error("the splat entry is not in the inventory: " + JSON.stringify(targets))
  return entry.instanceCount
}

test("a large cloud sorts on the sort thread: the camera crossing lands after a settle", async app => {
  let { scene: s, mesh, leaf } = await mounted(app, () => {
    let s = scene()
    let splats = [{ p: [0, 0, 0], c: RED }, { p: [0, 0, NEAR_Z], c: BLUE }]
    for (let i = 2; i < LARGE_CLOUD; i++) splats.push({ p: [FILLER_X + i, 0, 0], c: RED })
    let data = parseSplat(dotSplat(splats), { keepOrientation: true })
    let mesh: SplatMeshNode = createSplatMesh(data, { label: "splat-mesh-large" })
    add(s.root, mesh)
    return { scene: s, mesh }
  })
  await app.settle()
  expectColor(center(await app.painted(leaf)), BLUE)
  s.setCamera({ position: [0, 0, -DISTANCE], target: [0, 0, 0] })
  let before = frame(app)
  await app.settle()
  // The sort is waited out on the wall, not with frames: the settle runs
  // the frame that submits it and the one that lands it, no more, however
  // long the sort thread takes.
  expect(frame(app) - before).toBeLessThanOrEqual(SORT_LANDING_FRAMES)
  expectColor(center(await app.painted(leaf)), RED)
  disposeInstances(mesh)
})
