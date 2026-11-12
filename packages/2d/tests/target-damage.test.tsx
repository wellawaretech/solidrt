// Damage rectangles through a 2d layer (D8 of
// okf/design/draw-order-and-culling.md): a write to a drawn sprite of a
// culled view, under a camera that held still, re-renders only the
// rectangles its old and new box cover, and the pixels are those of a full
// render. Each check builds a culled layer and its `cull: false` twin over
// the same grid, with the same views, writes a sprite, and reads three
// things back: each view target's render facts from the GPU inventory
// (`fullRenders`, `damageRenders`, `damagedPixels`), the rows that changed
// (a rectangle mirrored top to bottom wipes rows the write never touched
// and leaves the changed ones as they were), and every culled view against
// its twin byte for byte. The same steps under the force-full switch
// (`app.forceFullRenders`) give the same bytes from full renders; a camera
// move and the twin drawn without culling render in full. A sprite layer
// and a record layer: at oversample 1 and 2, under a rotated, zoomed and
// pivoted camera on a view that is not square, through a population change,
// and over two views of one layer.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { DecodedImage } from "@solidrt/core"
import { readTexture } from "@solidrt/core/gpu"
import { addSprite, createAtlas, createRecordLayer, createSpriteLayer, destroy, setSprite } from "../src/index.ts"
import type { CameraUpdate, SpriteHandle as Sprite, ViewHandle } from "../src/index.ts"

// The square view, in view pixels.
const VIEW = 256
// The grid: GRID x GRID sprites STEP apart, SPRITE wide, the first
// centered at STEP / 2; a view at the default camera sees the top-left
// quarter. Room for EXTRA more, so an added sprite grows no buffer.
const GRID = 16
const STEP = 32
const SPRITE = 16
const EXTRA = 1
const CELL = 128
// The written sprite: column 3, row 1, centered at (112, 48) - in the top
// half of the square view.
const COLUMN = 3
const ROW = 1
// How far the write moves it right, view pixels.
const NUDGE = 4
// The rasterization edge a damage rectangle grows by on each side, and
// the outward rounding beside it, target pixels: the slack the area bound
// allows past the two boxes.
const EDGE_SLACK = 2
// The share of the target a sparse write's damage must stay under.
const SPARSE_SHARE = 0.01
// The sprite a population change adds: in the gap between grid sprites,
// so it covers nothing of the grid (the gap is STEP - SPRITE wide).
const ADDED_X = 6 * STEP
const ADDED_Y = 2 * STEP
// A view that is not square, and two cameras over it that rotate, zoom
// and pivot (the written sprite stays in view under both).
const WIDE = 320
const LOW = 192
const TURNED: CameraUpdate = { x: 112, y: 48, zoom: 1.5, rotation: 0.4, pivotX: 120, pivotY: 70 }
const TURNED_AGAIN: CameraUpdate = { x: 100, y: 60, zoom: 1.25, rotation: -0.3, pivotX: 160, pivotY: 96 }
// The second view of the two-view case: panned, the written sprite still
// in view.
const PANNED: CameraUpdate = { x: 2 * STEP, y: STEP / 2 }
// The per-position tint: three channels cycling at different periods, so
// neighbours never share a color and a sprite drawn in the wrong place
// shows. PALETTE_STEPS levels per channel, the red and green walks
// RED_STRIDE and GREEN_STRIDE levels per column and row, the blue BLUE_LEVELS
// levels over the diagonal.
const PALETTE_STEPS = 16
const RED_STRIDE = 7
const GREEN_STRIDE = 5
const BLUE_LEVELS = 4
const WHITE: [number, number, number, number] = [1, 1, 1, 1]
const ATLAS: DecodedImage = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }

type Facts = { fullRenders: number; damageRenders: number; damagedPixels: number }
type Target = Facts & { kind: string; label?: string }
type Kind = "sprites" | "records"
type Pixels = { width: number; height: number; data: Uint8Array }

// The render facts of the view target `label`.
function facts(app: TestApp, label: string): Facts {
  let pipelines = app.gpu({ label }).pipelines as Target[]
  let found = pipelines.find(p => p.kind === "draws" && p.label === label)
  if (found === undefined) throw new Error("draw target not in the inventory: " + label)
  return { fullRenders: found.fullRenders, damageRenders: found.damageRenders, damagedPixels: found.damagedPixels }
}

function tint(i: number, j: number): [number, number, number, number] {
  let top = PALETTE_STEPS - 1
  return [((i * RED_STRIDE) % PALETTE_STEPS) / top, ((j * GREEN_STRIDE) % PALETTE_STEPS) / top, ((i + j) % BLUE_LEVELS) / (BLUE_LEVELS - 1), 1]
}

// The first and last row in which `a` and `b` differ; null when they do
// not.
function changedRows(a: Pixels, b: Pixels): [number, number] | null {
  let rowBytes = a.width * 4
  let first = -1
  let last = -1
  for (let i = 0; i < a.data.length; i++) {
    if (a.data[i] === b.data[i]) continue
    let row = Math.floor(i / rowBytes)
    if (first < 0) first = row
    last = row
  }
  return first < 0 ? null : [first, last]
}

function sameBytes(a: Pixels, b: Pixels): void {
  expect(a.width).toBe(b.width)
  expect(a.height).toBe(b.height)
  expect(changedRows(a, b)).toBe(null)
}

type ViewSpec = { width: number; height: number; camera?: CameraUpdate }

// A culled layer of `kind` over the grid and its `cull: false` twin, each
// with one view per spec (at `oversample`), labelled `<label>-<k>-<v>`
// with k 0 for the culled layer and 1 for the twin.
type Rig = {
  label: string
  kind: Kind
  views: ViewHandle[][]
  layers: ReturnType<typeof createSpriteLayer | typeof createRecordLayer>[]
  // The written sprite of each layer.
  sprites: Sprite[]
}

function rig(kind: Kind, label: string, specs: ViewSpec[], oversample = 1): Rig {
  let atlas = createAtlas(ATLAS, { label: label + "-atlas" })
  let r: Rig = { label, kind, views: [], layers: [], sprites: [] }
  for (let [k, cull] of [[0, { cellSize: CELL }], [1, false]] as const) {
    let opts = { capacity: GRID * GRID + EXTRA, label: `${label}-${k}`, cull }
    let layer = kind === "sprites" ? createSpriteLayer([atlas], opts) : createRecordLayer([atlas], opts)
    r.layers.push(layer)
    r.views.push(specs.map((spec, v) => layer.createView({ ...spec, oversample, clearColor: [0, 0, 0, 0], label: `${label}-${k}-${v}` })))
    for (let i = 0; i < GRID; i++) {
      for (let j = 0; j < GRID; j++) {
        let sprite = addSprite(layer, { x: i * STEP + STEP / 2, y: j * STEP + STEP / 2, w: SPRITE, h: SPRITE, tint: tint(i, j) })
        if (i === COLUMN && j === ROW) r.sprites.push(sprite)
      }
    }
  }
  return r
}

async function mountRig(app: TestApp, build: () => Rig): Promise<Rig> {
  let r!: Rig
  await app.mount(() => {
    r = build()
    return (
      <view>
        {r.views.flat().map(v => (
          <texture src={v.texture} width={v.size().width} height={v.size().height} />
        ))}
      </view>
    )
  })
  await app.frame(2)
  return r
}

let culledLabel = (r: Rig, v: number) => `${r.label}-0-${v}`
let twinLabel = (r: Rig, v: number) => `${r.label}-1-${v}`

// Every culled view against its twin.
function matchesTwin(r: Rig): void {
  r.views[0]!.forEach((view, v) => sameBytes(readTexture(view.texture), readTexture(r.views[1]![v]!.texture)))
}

// The write and its undo, on both layers.
function write(r: Rig): void {
  let x = COLUMN * STEP + STEP / 2 + NUDGE
  for (let sprite of r.sprites) setSprite(sprite, { x, tint: WHITE })
}

function undo(r: Rig): void {
  let x = COLUMN * STEP + STEP / 2
  for (let sprite of r.sprites) setSprite(sprite, { x, tint: tint(COLUMN, ROW) })
}

function camera(r: Rig, v: number, update: CameraUpdate): void {
  for (let views of r.views) views[v]!.setCamera(update)
}

// Run `step` and a frame, expecting every culled view to render from
// damage rectangles (and no full render) and every twin to render in
// full; every culled view then matches its twin. Returns each culled
// view's damaged pixels.
async function damageStep(app: TestApp, r: Rig, step: () => void): Promise<number[]> {
  let count = r.views[0]!.length
  let before = [...Array(count).keys()].map(v => facts(app, culledLabel(r, v)))
  let twins = [...Array(count).keys()].map(v => facts(app, twinLabel(r, v)))
  step()
  await app.frame()
  let damaged: number[] = []
  for (let v = 0; v < count; v++) {
    let after = facts(app, culledLabel(r, v))
    expect(after.damageRenders).toBe(before[v]!.damageRenders + 1)
    expect(after.fullRenders).toBe(before[v]!.fullRenders)
    damaged.push(after.damagedPixels - before[v]!.damagedPixels)
    // The twin drawn without culling has no boxes: a full render.
    let twin = facts(app, twinLabel(r, v))
    expect(twin.fullRenders).toBe(twins[v]!.fullRenders + 1)
    expect(twin.damageRenders).toBe(twins[v]!.damageRenders)
  }
  matchesTwin(r)
  return damaged
}

// Run `step` and a frame, expecting view `v` to render in full.
async function fullStep(app: TestApp, r: Rig, v: number, step: () => void): Promise<void> {
  let before = facts(app, culledLabel(r, v))
  step()
  await app.frame()
  let after = facts(app, culledLabel(r, v))
  expect(after.fullRenders).toBe(before.fullRenders + 1)
  expect(after.damageRenders).toBe(before.damageRenders)
  matchesTwin(r)
}

// The whole sequence on a square view at the default camera: a damage
// render of about the two boxes on exactly the sprite's rows, the same
// steps under the switch from full renders with the same bytes, back to
// damage, then a camera move in full.
async function sequence(app: TestApp, kind: Kind, oversample: number): Promise<void> {
  let r = await mountRig(app, () => rig(kind, `${kind}-x${oversample}`, [{ width: VIEW, height: VIEW }], oversample))
  let view = r.views[0]![0]!
  let side = VIEW * oversample
  let p0 = readTexture(view.texture)
  expect(p0.width).toBe(side)
  matchesTwin(r)

  let [damaged] = await damageStep(app, r, () => write(r))
  let p1 = readTexture(view.texture)
  let box = SPRITE * oversample
  let reach = (SPRITE + NUDGE) * oversample + 2 * EDGE_SLACK
  expect(damaged!).toBeGreaterThanOrEqual(box * box)
  expect(damaged!).toBeLessThanOrEqual(reach * (box + 2 * EDGE_SLACK))
  expect(damaged!).toBeLessThan(side * side * SPARSE_SHARE)
  // The changed rows are the sprite's rows, top to bottom.
  let rows = changedRows(p0, p1)
  expect(rows).not.toBe(null)
  expect(rows![0]).toBe((ROW * STEP + (STEP - SPRITE) / 2) * oversample)
  expect(rows![1]).toBe((ROW * STEP + (STEP + SPRITE) / 2) * oversample - 1)

  // Under the switch the undo and the write again render in full, to the
  // same bytes.
  expect(app.forceFullRenders(true)).toBe(true)
  await fullStep(app, r, 0, () => undo(r))
  sameBytes(readTexture(view.texture), p0)
  await fullStep(app, r, 0, () => write(r))
  sameBytes(readTexture(view.texture), p1)
  expect(app.forceFullRenders(false)).toBe(false)

  // Off again, the undo is a damage render back to the first bytes.
  await damageStep(app, r, () => undo(r))
  sameBytes(readTexture(view.texture), p0)

  // A camera move is a full render.
  await fullStep(app, r, 0, () => camera(r, 0, { x: STEP * 2, y: STEP }))
}

for (let kind of ["sprites", "records"] as const) {
  let what = kind === "sprites" ? "sprite" : "record"
  for (let oversample of [1, 2]) {
    test(`a write to one ${what} of a culled view renders its rectangles at oversample ${oversample}`, app => sequence(app, kind, oversample))
  }

  test(`a ${what} write under a rotated, zoomed, pivoted camera on a wide view renders its rectangles, before and after a camera move`, async app => {
    let r = await mountRig(app, () => rig(kind, `${kind}-turned`, [{ width: WIDE, height: LOW, camera: TURNED }]))
    let view = r.views[0]![0]!
    matchesTwin(r)
    // The rotated box's bounding rectangle is at most its diagonal square.
    let bound = (zoom: number) => (Math.hypot(SPRITE + NUDGE, SPRITE) * zoom + 2 * EDGE_SLACK) ** 2
    let p0 = readTexture(view.texture)
    let [first] = await damageStep(app, r, () => write(r))
    expect(first!).toBeGreaterThan(0)
    expect(first!).toBeLessThanOrEqual(bound(TURNED.zoom!))
    expect(changedRows(p0, readTexture(view.texture))).not.toBe(null)
    await fullStep(app, r, 0, () => camera(r, 0, TURNED_AGAIN))
    let p1 = readTexture(view.texture)
    let [second] = await damageStep(app, r, () => undo(r))
    expect(second!).toBeGreaterThan(0)
    expect(second!).toBeLessThanOrEqual(bound(TURNED_AGAIN.zoom!))
    expect(changedRows(p1, readTexture(view.texture))).not.toBe(null)
  })

  test(`adding and removing a ${what} in a culled view render its rectangles`, async app => {
    let r = await mountRig(app, () => rig(kind, `${kind}-population`, [{ width: VIEW, height: VIEW }]))
    let view = r.views[0]![0]!
    let p0 = readTexture(view.texture)
    let added: Sprite[] = []
    await damageStep(app, r, () => {
      for (let layer of r.layers) added.push(addSprite(layer, { x: ADDED_X, y: ADDED_Y, w: SPRITE, h: SPRITE, tint: WHITE }))
    })
    expect(changedRows(p0, readTexture(view.texture))).not.toBe(null)
    await damageStep(app, r, () => {
      for (let sprite of added) destroy(sprite)
    })
    sameBytes(readTexture(view.texture), p0)
  })

  test(`a ${what} write renders its rectangles in both views of one layer`, async app => {
    let r = await mountRig(app, () => rig(kind, `${kind}-two`, [{ width: VIEW, height: VIEW }, { width: VIEW, height: VIEW, camera: PANNED }]))
    let damaged = await damageStep(app, r, () => write(r))
    for (let d of damaged) {
      expect(d).toBeGreaterThan(0)
      expect(d).toBeLessThan(VIEW * VIEW * SPARSE_SHARE)
    }
  })
}

// The switch is the alloy context's and outlives a test engine; the test
// host turns it off ahead of every test, so a test torn down with it on
// cannot leave the next one rendering in full.
test("a test may leave the force-full switch on", app => {
  expect(app.forceFullRenders(true)).toBe(true)
})

test("the next test starts with the force-full switch off", app => {
  expect(app.forceFullRenders()).toBe(false)
})
