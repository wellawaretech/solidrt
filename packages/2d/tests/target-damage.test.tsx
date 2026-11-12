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
// and over two views of one layer. Then the window side: the node showing
// a view damages the window only where the rectangles land in it, and the
// window a patched frame leaves is the window a whole redraw draws.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { createSignal, Show } from "@solidrt/core"
import type { DecodedImage } from "@solidrt/core"
import { compileShader, destroyShader, glsl, linkProgram, readTexture } from "@solidrt/core/gpu"
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

// The window side: the node showing a view damages the window only where
// the write's rectangles land in it, not over its whole box. One culled
// layer alone (a twin drawn without culling renders in full, and so
// damages its whole node), its view shown SHOWN_SCALE times its size, so
// the rectangles go through the node's fit. `damagePx` is the frame's
// window damage in logical px^2.
const SHOWN_SCALE = 2
// What a mapped rectangle grows by on each side in the window, logical px:
// the filter's texel through the fit, the window edge pixel, and the
// outward rounding.
const WINDOW_SLACK = SHOWN_SCALE + 2

for (let kind of ["sprites", "records"] as const) {
  let what = kind === "sprites" ? "sprite" : "record"
  test(`a ${what} write damages the window only where the node shows its rectangles`, async app => {
    let label = `${kind}-window`
    let view!: ViewHandle
    let sprite!: Sprite
    await app.mount(() => {
      let atlas = createAtlas(ATLAS, { label: label + "-atlas" })
      let opts = { capacity: GRID * GRID, label, cull: { cellSize: CELL } }
      let layer = kind === "sprites" ? createSpriteLayer([atlas], opts) : createRecordLayer([atlas], opts)
      view = layer.createView({ width: VIEW, height: VIEW, clearColor: [0, 0, 0, 0], label: label + "-view" })
      for (let i = 0; i < GRID; i++) {
        for (let j = 0; j < GRID; j++) {
          let added = addSprite(layer, { x: i * STEP + STEP / 2, y: j * STEP + STEP / 2, w: SPRITE, h: SPRITE, tint: tint(i, j) })
          if (i === COLUMN && j === ROW) sprite = added
        }
      }
      return <texture src={view.texture} width={VIEW * SHOWN_SCALE} height={VIEW * SHOWN_SCALE} />
    })
    await app.frame(2)
    // Of the last frame that changed the picture (the stats window over
    // one frame): the latest-frame figure reads 0 once a skipped frame
    // follows.
    let damagePx = () => (app.stats({ windowFrames: 1 }).window as { damagePx: number }).damagePx
    let node = (VIEW * SHOWN_SCALE) ** 2
    let x = COLUMN * STEP + STEP / 2

    // The two boxes (old and new, NUDGE apart), through the node's scale.
    let before = facts(app, label + "-view")
    setSprite(sprite, { x: x + NUDGE, tint: WHITE })
    await app.frame()
    expect(facts(app, label + "-view").damageRenders).toBe(before.damageRenders + 1)
    let box = SPRITE * SHOWN_SCALE
    let reach = (SPRITE + NUDGE + 2 * EDGE_SLACK) * SHOWN_SCALE + 2 * WINDOW_SLACK
    expect(damagePx()).toBeGreaterThanOrEqual(box * box)
    expect(damagePx()).toBeLessThanOrEqual(reach * (box + 2 * EDGE_SLACK * SHOWN_SCALE + 2 * WINDOW_SLACK))
    expect(damagePx()).toBeLessThan(node * SPARSE_SHARE)

    // Under the switch the same write is a full render, and the whole node.
    expect(app.forceFullRenders(true)).toBe(true)
    setSprite(sprite, { x, tint: tint(COLUMN, ROW) })
    await app.frame()
    expect(damagePx()).toBeGreaterThanOrEqual(node)
    expect(app.forceFullRenders(false)).toBe(false)

    // A camera move renders in full: the whole node again.
    view.setCamera({ x: STEP * 2, y: STEP })
    await app.frame()
    expect(damagePx()).toBeGreaterThanOrEqual(node)
  })
}

// The window's pixels under partial repaint: a headless frame redraws the
// whole window unless the patch switch (`app.patchWindow`) has it redraw
// only its damage over the last frame and keep the rest, as a window
// patches its back buffer: through the rig as a single-sample window does
// (`true`), or through the patch layer as a multisampled one does
// (`"layer"`), every case on both. So a damage rectangle in the wrong
// place, or a patch drawn off its place, shows in the window. Each case
// runs one sequence of steps twice on identical
// scenes, first patched, then with every frame redrawn whole, reads the
// window after every step (`app.windowPixels`, the window as the frame
// left it; a node's pixels are painted afresh), and requires the same
// bytes at every step. In the patched run a write must change the window
// (else the compare proves nothing), every checked frame must be a patch
// (`partialPresents`), and its damage must stay inside its bound. One
// culled sprite layer alone in each scene: a `cull: false` twin renders in
// full and damages its whole node.

// How the node shows the view: well under half its size (minified, no mip
// chain: the one-texel filter reach in rendertree::Shown is what is
// checked), at its size, at SHOWN_SCALE.
const MINIFIED = 0.25
// How far the repaint boundary moves after its first frame, logical px.
const BOUNDARY_MOVE = 40
// A blur radius for the blurring view, logical px: any with a reach.
const BLUR = 3
// The share of the node's area a write's window damage stays under where
// the rectangles map to the window (the patch is a few sprite boxes).
const PATCH_SHARE = 0.05
// The share of the window's area a frame that damages its whole node
// stays under: the node is far smaller than the window, so the frame
// still patches.
const WINDOW_SHARE = 0.5

// The frosted panel over the node in the backdrop case, logical px: it
// holds the written sprite's old and new boxes, grid sprites end at its
// left and top edges, outside it, within the blur's reach, and its sides
// are whole patch-layer steps, so a patch is the panel itself on both
// routes.
const PANEL_X = 88
const PANEL_Y = 24
const PANEL_W = 128
const PANEL_H = 64
// The panel's backdrop blur radius, logical px: any with a reach, small
// enough that the blur is not drawn downsampled.
const BACKDROP_BLUR = 4
// The window the cases run in, logical px (the test host's default), and
// where the corner case places the node: the written sprite's nudged box
// (view px 108 to 124 across, 40 to 56 down) ends at the window's
// bottom-right corner.
const WINDOW_SIZE: [number, number] = [1280, 720]
const SPRITE_RIGHT = COLUMN * STEP + STEP / 2 + NUDGE + SPRITE / 2
const SPRITE_BOTTOM = ROW * STEP + STEP / 2 + SPRITE / 2

// What a checked frame's window damage must be: a few sprite boxes, the
// whole node (a blur spreads the change, full dirt wins), the frosted
// panel at least (a change under it re-filters it), or only some of the
// window (the node is cut by the window's edge).
type Bound = "rects" | "node" | "panel" | "some"
// A step of a sequence: what it does to the scene, and whether its frame
// is checked (it writes, so the window must change, and it must patch
// inside its bound).
type WindowStep = { act: (s: WindowScene) => void; check?: Bound }
// How the scene wraps the node: bare, in a repaint boundary (a
// recording) the steps can move, in a view with a blur filter, under a
// frosted panel (a backdrop blur), or bare at the window's bottom-right
// corner.
type Wrap = "bare" | "boundary" | "blur" | "backdrop" | "corner"
// `only` runs the case along one patch route alone.
type WindowCase = { name: string; scale: number; wrap: Wrap; steps: WindowStep[]; only?: Patch }
type WindowScene = { view: ViewHandle; sprite: Sprite; move: (by: number) => void }
// The routes a patch takes to the window, as the patch switch names them.
type Patch = true | "layer"
const PATCH_ROUTES: { name: string; patch: Patch }[] = [
  { name: "rig", patch: true },
  { name: "patch layer", patch: "layer" },
]

let nudged: WindowStep = { act: s => setSprite(s.sprite, { x: COLUMN * STEP + STEP / 2 + NUDGE, tint: WHITE }), check: "rects" }
let restored: WindowStep = { act: s => setSprite(s.sprite, { x: COLUMN * STEP + STEP / 2, tint: tint(COLUMN, ROW) }), check: "rects" }

const WINDOW_CASES: WindowCase[] = [
  { name: "magnified", scale: SHOWN_SCALE, wrap: "bare", steps: [nudged, restored] },
  { name: "minified", scale: MINIFIED, wrap: "bare", steps: [nudged, restored] },
  // The move replays the recording; the writes after it land where it moved.
  { name: "boundary", scale: 1, wrap: "boundary", steps: [{ act: s => s.move(BOUNDARY_MOVE) }, nudged, restored] },
  { name: "blur", scale: 1, wrap: "blur", steps: [{ ...nudged, check: "node" }, { ...restored, check: "node" }] },
  // A write and a camera move in one frame: the move's full dirt wins.
  {
    name: "camera",
    scale: 1,
    wrap: "bare",
    steps: [{
      act: s => {
        nudged.act(s)
        s.view.setCamera({ x: STEP * 2, y: STEP })
      },
      check: "node",
    }],
  },
  // A write under a frosted panel re-filters the panel, whose blur reads
  // the window past the panel's edges.
  { name: "backdrop", scale: 1, wrap: "backdrop", steps: [{ ...nudged, check: "panel" }, { ...restored, check: "panel" }] },
  // A large patch (the whole node, many layer steps wide) and then small
  // ones: the patch layer is larger than the patch, and the patch changes
  // size between frames.
  { name: "large then small", scale: 1, wrap: "bare", only: "layer", steps: [{ act: s => s.view.setCamera({ x: STEP * 2, y: STEP }), check: "node" }, nudged, restored] },
  // The same at the window's bottom-right corner, where the layer, larger
  // than the small patch there, lands past the window's edge.
  {
    name: "corner",
    scale: 1,
    wrap: "corner",
    steps: [
      { act: s => s.view.setCamera({ x: STEP * 2, y: STEP }), check: "some" },
      { act: s => s.view.setCamera({ x: 0, y: 0 }), check: "some" },
      nudged,
      restored,
    ],
  },
]

// One scene of a case: the culled layer over the grid, its view shown at
// the case's scale, wrapped as the case says. Hands its handles to `out`.
function windowScene(c: WindowCase, run: number, out: (s: WindowScene) => void) {
  let label = `window-${c.name}-${run}`
  let atlas = createAtlas(ATLAS, { label: label + "-atlas" })
  let layer = createSpriteLayer([atlas], { capacity: GRID * GRID, label, cull: { cellSize: CELL } })
  let view = layer.createView({ width: VIEW, height: VIEW, clearColor: [0, 0, 0, 0], label: label + "-view" })
  let sprite!: Sprite
  for (let i = 0; i < GRID; i++) {
    for (let j = 0; j < GRID; j++) {
      let added = addSprite(layer, { x: i * STEP + STEP / 2, y: j * STEP + STEP / 2, w: SPRITE, h: SPRITE, tint: tint(i, j) })
      if (i === COLUMN && j === ROW) sprite = added
    }
  }
  let [offset, setOffset] = createSignal(0)
  out({ view, sprite, move: setOffset })
  let side = VIEW * c.scale
  let shown = <texture src={view.texture} width={side} height={side} />
  if (c.wrap === "boundary") return <view repaintBoundary x={offset()} y={offset()}>{shown}</view>
  if (c.wrap === "blur") return <view filter={{ blur: BLUR }}>{shown}</view>
  if (c.wrap === "backdrop") {
    return (
      <view>
        {shown}
        <view position="absolute" left={PANEL_X} top={PANEL_Y} width={PANEL_W} height={PANEL_H} backdropFilter={{ blur: BACKDROP_BLUR }} />
      </view>
    )
  }
  if (c.wrap === "corner") {
    return (
      <view position="absolute" left={WINDOW_SIZE[0] - SPRITE_RIGHT} top={WINDOW_SIZE[1] - SPRITE_BOTTOM}>
        {shown}
      </view>
    )
  }
  return shown
}

// The window after every step of `c`, on a scene of its own, with the
// patch switch `patched`; in a patched run each checked frame is checked.
async function windowRun(app: TestApp, c: WindowCase, scene: () => WindowScene, patched: Patch | false): Promise<Pixels[]> {
  expect(app.patchWindow(patched)).toBe(patched)
  let damagePx = () => (app.stats({ windowFrames: 1 }).window as { damagePx: number }).damagePx
  // Read after the window, whose read follows the frame on the raster
  // thread's queue.
  let patches = () => app.stats().partialPresents as number
  let windows = [app.windowPixels()]
  for (let step of c.steps) {
    let before = patches()
    step.act(scene())
    await app.frame()
    let window = app.windowPixels()
    if (!patched) {
      expect(patches()).toBe(before)
    } else if (step.check !== undefined) {
      expect(changedRows(windows[windows.length - 1]!, window)).not.toBe(null)
      expect(patches()).toBe(before + 1)
      let node = (VIEW * c.scale) ** 2
      if (step.check === "rects") {
        expect(damagePx()).toBeGreaterThan(0)
        expect(damagePx()).toBeLessThan(node * PATCH_SHARE)
      } else if (step.check === "panel" || step.check === "some") {
        expect(damagePx()).toBeGreaterThanOrEqual(step.check === "panel" ? PANEL_W * PANEL_H : 1)
        expect(damagePx()).toBeLessThan(window.width * window.height * WINDOW_SHARE)
      } else {
        expect(damagePx()).toBeGreaterThanOrEqual(node)
        expect(damagePx()).toBeLessThan(window.width * window.height * WINDOW_SHARE)
      }
    }
    windows.push(window)
  }
  return windows
}

// The two runs of `c` on a mounted app: patched along `patch` on scene 0,
// then whole on scene 1 (`setRun(1)` swaps it in), the same window after
// every step.
async function patchedMatchesWhole(
  app: TestApp,
  c: WindowCase,
  patch: Patch,
  scenes: WindowScene[],
  setRun: (run: number) => void,
) {
  await app.frame(2)
  let patchedRun = await windowRun(app, c, () => scenes[0]!, patch)
  // The second scene, identical, every frame redrawn whole.
  expect(app.patchWindow(false)).toBe(false)
  setRun(1)
  await app.frame(2)
  let wholeRun = await windowRun(app, c, () => scenes[1]!, false)
  patchedRun.forEach((window, i) => sameBytes(window, wholeRun[i]!))
}

for (let route of PATCH_ROUTES) {
  for (let c of WINDOW_CASES) {
    if (c.only !== undefined && c.only !== route.patch) continue
    test(`a patched window matches a whole redraw: ${c.name} (${route.name})`, async app => {
      let [run, setRun] = createSignal(0)
      let scenes: WindowScene[] = []
      await app.mount(() => (
        <view>
          <Show when={run() === 0} fallback={windowScene(c, 1, s => (scenes[1] = s))}>
            {windowScene(c, 0, s => (scenes[0] = s))}
          </Show>
        </view>
      ))
      await patchedMatchesWhole(app, c, route.patch, scenes, setRun)
    }, { size: WINDOW_SIZE })
  }
}

// The window shader's covering triangle, its v flipped so the frame lands
// upright (packages/core/examples/window-shader.tsx).
let COVER = glsl`#version 300 es
  precision highp float;
  out vec2 vUV;
  void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    vUV = vec2(p.x, 1.0 - p.y);
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
  }
`
// A window shader that inverts the frame's colors: a shaded pixel a frame
// keeps after the shader is gone differs from an unshaded one.
let INVERT = glsl`
  uniform sampler2D uSource;
  in vec2 vUV;
  void main() {
    vec4 c = texture(uSource, vUV);
    fragColor = vec4(vec3(1.0) - c.rgb, 1.0);
  }
`

// A window shader on, then off, over one small write per frame: while it
// is on every frame redraws whole, and the frame that drops it must too,
// or the window keeps the shaded pixels outside the write. The writes
// after it patch again.
for (let route of PATCH_ROUTES) {
  test(`a patched window matches a whole redraw: a window shader on and off (${route.name})`, async app => {
    let [run, setRun] = createSignal(0)
    let [shaded, setShaded] = createSignal(false)
    let scenes: WindowScene[] = []
    let shade = (on: boolean, step: WindowStep): WindowStep => ({
      act: s => {
        setShaded(on)
        step.act(s)
      },
    })
    let c: WindowCase = {
      name: "shader",
      scale: 1,
      wrap: "bare",
      steps: [shade(true, nudged), shade(true, restored), shade(false, nudged), restored, nudged],
    }
    await app.mount(() => {
      let vs = compileShader("vertex", COVER)
      let fs = compileShader("fragment", INVERT, { header: true })
      let program = linkProgram(vs, fs, { label: "invert" })
      destroyShader(vs)
      destroyShader(fs)
      return (
        <window shader={shaded() ? { program } : null}>
          <Show when={run() === 0} fallback={windowScene(c, 1, s => (scenes[1] = s))}>
            {windowScene(c, 0, s => (scenes[0] = s))}
          </Show>
        </window>
      )
    })
    await patchedMatchesWhole(app, c, route.patch, scenes, setRun)
  })
}

// The switch is the alloy context's and outlives a test engine; the test
// host turns it off ahead of every test, so a test torn down with it on
// cannot leave the next one patching its window.
test("a test may leave the patch switch on", app => {
  expect(app.patchWindow(true)).toBe(true)
})

test("the next test starts with the patch switch off", app => {
  expect(app.patchWindow()).toBe(false)
})

// The switch is the alloy context's and outlives a test engine; the test
// host turns it off ahead of every test, so a test torn down with it on
// cannot leave the next one rendering in full.
test("a test may leave the force-full switch on", app => {
  expect(app.forceFullRenders(true)).toBe(true)
})

test("the next test starts with the force-full switch off", app => {
  expect(app.forceFullRenders()).toBe(false)
})
