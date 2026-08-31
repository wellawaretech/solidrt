// Camera-mapping probe: the live guard that the three spellings of the
// camera mapping agree - projectCamera (camera.ts), the vertex stages'
// uCameraRot (shaders.ts), and pointer dispatch's unprojectCamera
// (layer.ts). The headless check (checks/camera-check.ts) ties
// projectCamera to the <view>-prop oracle but cannot see the shader; this
// probe closes that gap on a real client. Self-asserting: watch the logs
// for CAMERA lines ending in CAMERA-OK. Run it after touching ANY
// rotation in the package (the touch-one-rotation-touch-all rule in
// AGENTS.md).
//
// What it checks, all under one rotated + pivoted camera:
// 1. The shader implements projectCamera: a probe sprite samples opaque
//    at its PROJECTED pixel and background at the unrotated position.
// 2. VERTEX (records) and VERTEX_SPLIT (nodes) agree: pixel parity
//    between the two layer kinds.
// 3. Pointer dispatch round-trips: a pointer at the projected screen
//    point hits the sprite and reports its world coordinates.
// 4. Tile chunks bake with the pinned identity camera rotation.
import { onFrame, render } from "@solidrt/core"
import { readTexture } from "@solidrt/core/gpu"
import {
  addSprite,
  createAtlas,
  createRecordLayer,
  createSpriteLayer,
  createTileLayer,
  FULL_FRAME,
  grid,
  projectCamera,
} from "@solidrt/2d"
import type { CameraUpdate, SpriteHandle, SpritePointerEvent } from "@solidrt/2d"
import logoBytes from "./logo.png" with { type: "binary" }

const W = 360
const H = 360
const N = 40
const SEED = 0xcafe

// Chosen so the probe sprite at world (320, 320) projects on-screen
// (~185, ~208) while its unrotated position (20, 20) maps back to a world
// point far outside the population's reach - both sample points are clean.
const CAM: CameraUpdate = { x: 300, y: 300, zoom: 1, rotation: 0.6, pivotX: 180, pivotY: 180 }

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
  let nodes = createSpriteLayer(W, H, atlas.texture, { label: "cam-nodes" })
  let records = createRecordLayer(W, H, atlas.texture, { label: "cam-records" })
  nodes.setCamera(CAM)
  records.setCamera(CAM)

  // The same deterministic population on both layers, kept inside world
  // 20..100 so its reach (~145 with rotation) stays clear of both sample
  // points.
  let fields = (r: () => number) => ({
    x: 20 + r() * 80,
    y: 20 + r() * 80,
    w: 16 + r() * 48,
    h: 16 + r() * 48,
    rotation: r() * Math.PI * 2,
    frame: frames[Math.floor(r() * 4) % 4]!,
    tint: [0.5 + r() * 0.5, 0.5 + r() * 0.5, 0.5 + r() * 0.5, 1] as [number, number, number, number],
  })
  let r1 = rng(SEED)
  for (let i = 0; i < N; i++) addSprite(nodes, fields(r1))
  let r2 = rng(SEED)
  for (let i = 0; i < N; i++) addSprite(records, fields(r2))

  // The probe sprite: the FULL logo frame (its central 10% is fully
  // opaque, so the projected center samples opaque), isolated from the
  // population.
  let probe: SpriteHandle = addSprite(nodes, { x: 320, y: 320, w: 40, h: 40, frame: FULL_FRAME })
  addSprite(records, { x: 320, y: 320, w: 40, h: 40, frame: FULL_FRAME })

  // A small tile layer: the bake path with the pinned identity rotation.
  // The checked tile (2, 2) gets the full frame (opaque center texel).
  let tiles = createTileLayer(8, 8, 16, 16, atlas.texture, { label: "cam-tiles" })
  for (let i = 0; i < 8; i++) tiles.setTile(i, i, i === 2 ? FULL_FRAME : frames[i % 4]!)

  let failures: string[] = []
  let check = (ok: boolean, what: string) => {
    if (!ok) failures.push(what)
    console.log(`CAMERA ${ok ? "ok" : "FAIL"}: ${what}`)
  }

  let alphaAt = (img: { width: number; height: number; data: Uint8Array | Uint8ClampedArray }, sx: number, sy: number, n: number) => {
    // The target is oversampled n times the layer size; sample the texel
    // at the scaled screen point.
    let px = Math.min(img.width - 1, Math.max(0, Math.round(sx * n)))
    let py = Math.min(img.height - 1, Math.max(0, Math.round(sy * n)))
    return img.data[(py * img.width + px) * 4 + 3]!
  }

  let shaderCheck = () => {
    let img = readTexture(nodes.texture)
    let n = nodes.oversample
    let [sx, sy] = projectCamera(CAM, 320, 320)
    check(sx > 8 && sx < W - 8 && sy > 8 && sy < H - 8, `projected probe on screen (${sx.toFixed(1)}, ${sy.toFixed(1)})`)
    check(alphaAt(img, sx, sy, n) > 0, "probe center opaque at its PROJECTED pixel")
    // Where a pre-rotation shader would draw it: (world - cam) * zoom.
    let ux = (320 - CAM.x!) * CAM.zoom!
    let uy = (320 - CAM.y!) * CAM.zoom!
    check(Math.hypot(ux - sx, uy - sy) > 40, "rotated and unrotated positions are far apart")
    check(alphaAt(img, ux, uy, n) === 0, "background at the UNROTATED position")
    let [cx, cy] = projectCamera(CAM, CAM.x!, CAM.y!)
    check(Math.abs(cx - CAM.pivotX!) < 1e-9 && Math.abs(cy - CAM.pivotY!) < 1e-9, "camera world point projects onto the pivot")
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
    check(pct < 0.5, `node/record pixel parity under rotated camera (${off}/${total} off, ${pct.toFixed(3)}%)`)
  }

  let pointerRoundTrip = () => {
    let got: SpritePointerEvent | null = null
    probe.onPointerDown = e => (got = e)
    let [sx, sy] = projectCamera(CAM, 320, 320)
    // Synthesize the element event the built-in leaf would deliver (layout
    // null: localX/localY are layer pixels already).
    nodes.handlers.onPointerDown({
      localX: sx,
      localY: sy,
      pointerId: 1,
      pointerType: "mouse",
      button: 0,
      shiftKey: false,
      ctrlKey: false,
      altKey: false,
      metaKey: false,
    } as never)
    // TS narrows `got` to null (the write is inside the handler call above).
    let hit = got as SpritePointerEvent | null
    check(hit !== null && hit.sprite === probe, "pointer at the projected pixel hits the probe")
    check(hit !== null && Math.abs(hit.x - 320) < 1e-6 && Math.abs(hit.y - 320) < 1e-6, `pointer event carries world coords (${hit ? `${hit.x.toFixed(3)}, ${hit.y.toFixed(3)}` : "none"})`)
    check(nodes.pick(320, 320) === probe, "pick in world space still hits the probe")
  }

  let tileBake = () => {
    let chunk = tiles.chunks[0]
    if (!chunk) {
      check(false, "tile chunk allocated")
      return
    }
    let img = readTexture(chunk.texture)
    let n = tiles.oversample
    // Tile (2, 2) center at world (40, 40), chunk origin (0, 0).
    check(alphaAt(img, 40, 40, n) > 0, "tile bakes opaque with the pinned identity camera rotation")
    check(alphaAt(img, 40, 8, n) === 0, "empty cell stays clear")
  }

  let frame = 0
  onFrame(() => {
    frame++
    if (frame === 6) {
      shaderCheck()
      pixelParity()
      pointerRoundTrip()
      tileBake()
      console.log(failures.length === 0 ? "CAMERA-OK" : `CAMERA-FAIL: ${failures.join("; ")}`)
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
