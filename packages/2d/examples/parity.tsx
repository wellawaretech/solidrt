// Node-vs-records parity probe: the same deterministic sprite population
// rendered through the node-backed live layer (spatial arena nodes, pose
// buffer core-written) and the records layer (13 JS-owned floats), side by
// side. Self-asserting: after the first frames it readTexture-compares the
// two outputs pixel by pixel (the pose round-trips through the core's
// Pose2D decomposition, so only alpha-edge pixels may flip), cross-checks
// picking at random points, exercises pickRect, groups and slot recycling,
// and benches addSprite plus a move-everything frame on both layers. Watch
// the logs for PARITY/BENCH lines ending in PARITY-OK.
import { onFrame, render } from "@solidrt/core"
import { readTexture } from "@solidrt/core/gpu"
import {
  addGroup,
  addSprite,
  createAtlas,
  createRecordLayer,
  createSpriteLayer,
  getSprite,
  grid,
  removeSprite,
  setGroup,
  setSprite,
} from "@solidrt/2d"
import type { SpriteHandle } from "@solidrt/2d"
import logoBytes from "./logo.png" with { type: "binary" }

const N = 200
const W = 360
const H = 360
const SEED = 0x2d2d

// Deterministic PRNG (mulberry32) so both layers see identical sprites.
function rng(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

function App() {
  let atlas = createAtlas(logoBytes, { label: "logo-atlas" })
  let frames = grid(2, 2, { width: atlas.width, height: atlas.height })
  let nodes = createSpriteLayer(W, H, atlas.texture, { capacity: 64, clearColor: [0.05, 0.05, 0.09, 1], label: "nodes" })
  let records = createRecordLayer(W, H, atlas.texture, { capacity: 64, clearColor: [0.05, 0.05, 0.09, 1], label: "records" })

  let fields = (r: () => number) => ({
    x: 20 + r() * (W - 40),
    y: 20 + r() * (H - 40),
    w: 16 + r() * 48,
    h: 16 + r() * 48,
    rotation: r() * Math.PI * 2,
    frame: frames[Math.floor(r() * 4) % 4]!,
    tint: [0.5 + r() * 0.5, 0.5 + r() * 0.5, 0.5 + r() * 0.5, 1] as [number, number, number, number],
  })

  // Population (the capacity of 64 forces both growth paths, including the
  // pose-buffer retarget).
  let nodeSprites: SpriteHandle[] = []
  let recSprites: SpriteHandle[] = []
  let r1 = rng(SEED)
  let t0 = performance.now()
  for (let i = 0; i < N; i++) nodeSprites.push(addSprite(nodes, fields(r1)))
  let tNodesAdd = performance.now() - t0
  let r2 = rng(SEED)
  t0 = performance.now()
  for (let i = 0; i < N; i++) recSprites.push(addSprite(records, fields(r2)))
  let tRecsAdd = performance.now() - t0
  console.log(`BENCH addSprite x${N}: nodes ${tNodesAdd.toFixed(2)}ms, records ${tRecsAdd.toFixed(2)}ms`)

  let failures: string[] = []
  let check = (ok: boolean, what: string) => {
    if (!ok) failures.push(what)
    console.log(`PARITY ${ok ? "ok" : "FAIL"}: ${what}`)
  }

  // Slot recycling: remove one, add another, the layer stays coherent.
  {
    let extra = addSprite(nodes, { x: 10, y: 10, w: 8, h: 8 })
    let slotBefore = extra._slot
    removeSprite(extra)
    let reused = addSprite(nodes, { x: -100, y: -100, w: 0, h: 0 })
    check(reused._slot === slotBefore, "removed slot recycles to the next add")
    check(getSprite(extra) === null, "removed handle is inert")
    removeSprite(reused)
  }

  let pixelParity = () => {
    let a = readTexture(nodes.texture)
    let b = readTexture(records.texture)
    if (a.width !== b.width || a.height !== b.height) {
      check(false, `sizes differ: ${a.width}x${a.height} vs ${b.width}x${b.height}`)
      return
    }
    let off = 0
    for (let i = 0; i < a.data.length; i += 4) {
      if (
        Math.abs(a.data[i]! - b.data[i]!) > 8 ||
        Math.abs(a.data[i + 1]! - b.data[i + 1]!) > 8 ||
        Math.abs(a.data[i + 2]! - b.data[i + 2]!) > 8 ||
        Math.abs(a.data[i + 3]! - b.data[i + 3]!) > 8
      ) {
        off++
      }
    }
    let total = a.data.length / 4
    let pct = (off / total) * 100
    check(pct < 0.5, `pixels within tolerance (${off}/${total} off, ${pct.toFixed(3)}%)`)
  }

  let pickParity = () => {
    let r = rng(SEED ^ 0xffff)
    let byNode = new Map(nodeSprites.map((s, i) => [s, i]))
    let byRec = new Map(recSprites.map((s, i) => [s, i]))
    let mismatches = 0
    for (let i = 0; i < 200; i++) {
      let x = r() * W
      let y = r() * H
      let a = nodes.pick(x, y)
      let b = records.pick(x, y)
      let ai = a ? byNode.get(a) : null
      let bi = b ? byRec.get(b) : null
      if (ai !== bi) mismatches++
    }
    check(mismatches <= 1, `pick agrees at random points (${mismatches} mismatches)`)
  }

  let rectQuery = () => {
    // Left half of the layer: every hit must at least touch it by AABB,
    // every sprite fully inside it must be reported.
    let hits = new Set(nodes.pickRect(0, 0, W / 2, H))
    let missed = 0
    let phantom = 0
    for (let s of nodeSprites) {
      let f = getSprite(s)!
      let radius = (Math.hypot(f.w, f.h) / 2) * 1.001
      let inside = f.x + radius <= W / 2 && f.x - radius >= 0 && f.y - radius >= 0 && f.y + radius <= H
      let touches = f.x - radius <= W / 2
      if (inside && !hits.has(s)) missed++
      if (!touches && hits.has(s)) phantom++
    }
    check(missed === 0 && phantom === 0, `pickRect covers the left half (${hits.size} hits, ${missed} missed, ${phantom} phantom)`)
  }

  let groups = () => {
    // A group at (100, 100) rotated 90deg clockwise: a child sprite at
    // local (50, 0) lands at world (100, 150) in y-down space.
    let g = addGroup(nodes, { x: 100, y: 100 })
    let child = addSprite(nodes, { parent: g, x: 50, y: 0, w: 10, h: 10 })
    setGroup(g, { rotation: Math.PI / 2 })
    queueMicrotask(() => {
      check(nodes.pick(100, 150) === child, "group rotation carries the child sprite")
      check(nodes.pick(150, 100) !== child, "the ungrouped position no longer hits")
      let f = getSprite(child)!
      check(f.x === 50 && f.y === 0, "getSprite reads the local pose")
      removeSprite(child)
      // The group node stays for the layer to dispose.
    })
  }

  let bench = () => {
    let r = rng(SEED ^ 0xabcd)
    let t = performance.now()
    for (let i = 0; i < N; i++) setSprite(nodeSprites[i]!, { x: 20 + r() * (W - 40), y: 20 + r() * (H - 40) })
    let tNodes = performance.now() - t
    r = rng(SEED ^ 0xabcd)
    t = performance.now()
    for (let i = 0; i < N; i++) setSprite(recSprites[i]!, { x: 20 + r() * (W - 40), y: 20 + r() * (H - 40) })
    let tRecs = performance.now() - t
    console.log(`BENCH move x${N}: nodes ${tNodes.toFixed(2)}ms, records ${tRecs.toFixed(2)}ms`)
    // Put both populations back so the parity compare still holds.
    let ra = rng(SEED)
    for (let i = 0; i < N; i++) setSprite(nodeSprites[i]!, fields(ra))
    let rb = rng(SEED)
    for (let i = 0; i < N; i++) setSprite(recSprites[i]!, fields(rb))
  }

  let frame = 0
  onFrame(() => {
    frame++
    if (frame === 3) {
      bench()
      groups()
    }
    if (frame === 6) {
      pixelParity()
      pickParity()
      rectQuery()
      console.log(failures.length === 0 ? "PARITY-OK" : `PARITY-FAIL: ${failures.join("; ")}`)
    }
  })

  return (
    <window flexDirection="row" alignItems="center" justifyContent="center" gap={8}>
      <texture src={nodes.texture} width={W} height={H} />
      <texture src={records.texture} width={W} height={H} />
    </window>
  )
}

render(() => <App />)
