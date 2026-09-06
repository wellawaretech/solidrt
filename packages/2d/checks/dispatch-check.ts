// Checks for the layer's pointer dispatch (dispatch.ts): the walk from the
// hit sprite through its groups to the layer root, misses reaching the
// root alone, stopPropagation claiming (a stopped down keeps the root out
// of the whole press), capture per pointer to sprite or root, hover
// pairing, wheel through the same walk, tap synthesis (slop, the alone
// rule, same-target release, the repeat count and its window), layout
// scaling and the camera undo. Pure-module input only (dispatch.ts
// imports no GUI), so it runs headless on flux, bundled from the repo
// root:
//
//   bunx srt bundle -f --stdout packages/2d/checks/dispatch-check.ts | target/release/flux -
//
// A failure prints FAIL lines and throws at the end, and the flux binary
// exits 1 on the uncaught throw, so a CI step can gate on the exit code.
// The live side (real element events off the leaf, the camera attached at
// the root) is exercised by examples/camera.tsx and examples/pick.tsx.

import type { PointerEvent as ElementPointerEvent, WheelEvent as ElementWheelEvent } from "@solidrt/core"
import { spriteDispatch } from "../src/dispatch.ts"
import type { LayerPointerListener, Sprite, SpriteGroup, SpriteLayer } from "../src/layer.ts"
import { pointInSprite } from "../src/pick.ts"
import type { CameraUpdate } from "../src/camera.ts"

let failures = 0
function fail(msg: string) {
  failures++
  console.log(`FAIL: ${msg}`)
}
function expect(name: string, got: string[], want: string[]) {
  let g = got.join(" ")
  let w = want.join(" ")
  if (g !== w) fail(`${name}: got [${g}] want [${w}]`)
}

// A fake layer with named sprites and groups whose handlers log
// "name:event" (plus the sprite the event names, and tap counts) into
// `log`, and whose pick is the real rotated-rect narrowphase over the
// sprite rects, topmost (last added) first.
type Fake = { name: string; x: number; y: number; w: number; h: number }
let log: string[] = []
let root = { name: "root" } as unknown as SpriteLayer
let listeners = new Set<LayerPointerListener>()
let sprites: (Sprite & Fake)[] = []
let camera: CameraUpdate = { x: 0, y: 0, zoom: 1, rotation: 0, pivotX: 0, pivotY: 0 }
let clock = 0
let size: [number, number] = [400, 200]

let tag = (e: { sprite: Sprite | null; tapCount?: number }) => (e.sprite ? (e.sprite as Sprite & Fake).name : "-") + (e.tapCount ? "#" + e.tapCount : "")
let handlers = (name: string, stop: Set<string> = new Set()) => ({
  onPointerDown(e: { sprite: Sprite | null; stopPropagation(): void }) {
    log.push(`${name}:down(${tag(e)})`)
    if (stop.has("down")) e.stopPropagation()
  },
  onPointerMove(e: { sprite: Sprite | null; stopPropagation(): void }) {
    log.push(`${name}:move(${tag(e)})`)
    if (stop.has("move")) e.stopPropagation()
  },
  onPointerUp(e: { sprite: Sprite | null }) {
    log.push(`${name}:up(${tag(e)})`)
  },
  onWheel(e: { sprite: Sprite | null; deltaY: number }) {
    log.push(`${name}:wheel(${tag(e)},${e.deltaY})`)
  },
  onTap(e: { sprite: Sprite | null; tapCount: number; x: number; y: number }) {
    log.push(`${name}:tap(${tag(e)})@${e.x},${e.y}`)
  },
})
function group(name: string, parent: SpriteGroup | null = null, stop?: Set<string>): SpriteGroup {
  return { layer: root, _parent: parent, _children: new Set(), ...handlers(name, stop) } as unknown as SpriteGroup
}
function sprite(name: string, x: number, y: number, w: number, h: number, parent: SpriteGroup | null = null, stop?: Set<string>): Sprite & Fake {
  let s = {
    layer: root,
    _parent: parent,
    name,
    x,
    y,
    w,
    h,
    ...handlers(name, stop),
    onPointerEnter: () => log.push(`${name}:enter`),
    onPointerLeave: () => log.push(`${name}:leave`),
  } as unknown as Sprite & Fake
  sprites.push(s)
  return s
}
let pick = (x: number, y: number): Sprite[] => {
  let out: Sprite[] = []
  for (let i = sprites.length - 1; i >= 0; i--) {
    let s = sprites[i]!
    if (s.layer !== null && pointInSprite(x, y, s.x, s.y, s.w, s.h, 0)) out.push(s)
  }
  return out
}
let dispatch = spriteDispatch({
  size: () => size,
  camera: () => camera,
  pick,
  root,
  listeners,
  now: () => clock,
})
let leaf = dispatch(null)
let rootHandlers = handlers("root")
listeners.add(rootHandlers)

function ev(localX: number, localY: number, pointerId = 1, extra: Partial<ElementWheelEvent> = {}): ElementWheelEvent {
  return {
    localX,
    localY,
    clientX: localX,
    clientY: localY,
    parentX: localX,
    parentY: localY,
    movementX: 0,
    movementY: 0,
    currentTarget: 0,
    target: 0,
    pointerId,
    pointerType: "mouse",
    button: 0,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
    metaKey: false,
    deltaX: 0,
    deltaY: 0,
    stopPropagation() {},
    ...extra,
  }
}
function reset() {
  log = []
  sprites = []
}

// World: group G holds sprite S (a 40x40 square at 100,100); T stands
// alone at 300,100. Empty space everywhere else.
let G = group("G")
let S = sprite("S", 100, 100, 40, 40, G)
let T = sprite("T", 300, 100, 40, 40)

// A. A miss walks the root alone, sprite null.
log = []
leaf.onPointerDown(ev(10, 10))
leaf.onPointerMove(ev(12, 12))
leaf.onPointerUp(ev(12, 12))
expect("miss press", log, ["root:down(-)", "root:move(-)", "root:up(-)", "root:tap(-#1)@12,12"])

// B. A hit bubbles sprite -> group -> root, the sprite constant.
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerUp(ev(100, 100))
expect("hit walk", log, ["S:down(S)", "G:down(S)", "root:down(S)", "S:up(S)", "G:up(S)", "root:up(S)", "S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100"])

// C. A stopped down claims the press: the chain keeps bubbling, the root
// never sees that pointer again (move, up, tap included).
let C = sprite("C", 100, 100, 40, 40, G, new Set(["down"]))
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerMove(ev(150, 150))
leaf.onPointerUp(ev(150, 150))
expect("claimed press", log, ["C:down(C)", "C:move(C)", "G:move(C)", "C:up(C)", "G:up(C)"])
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerUp(ev(101, 101))
expect("claimed tap", log, ["C:down(C)", "C:up(C)", "G:up(C)", "C:tap(C#1)@101,101", "G:tap(C#1)@101,101"])
sprites.pop()

// D. A group stopping a move keeps the root out of that event only.
let H = group("H", null, new Set(["move"]))
let D = sprite("D", 100, 100, 40, 40, H)
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerMove(ev(102, 102))
leaf.onPointerUp(ev(102, 102))
expect("group stop per event", log, ["D:down(D)", "H:down(D)", "root:down(D)", "D:move(D)", "H:move(D)", "D:up(D)", "H:up(D)", "root:up(D)", "D:tap(D#1)@102,102", "H:tap(D#1)@102,102", "root:tap(D#1)@102,102"])
sprites.pop()

// E. Root capture: a press from empty space stays with the root as it
// crosses sprites - no enter, no sprite handlers, sprite stays null.
log = []
leaf.onPointerDown(ev(10, 10))
leaf.onPointerMove(ev(100, 100))
leaf.onPointerUp(ev(300, 100))
expect("root capture", log, ["root:down(-)", "root:move(-)", "root:up(-)"])

// F. Sprite capture: a press on S keeps naming S off the sprite.
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerMove(ev(10, 10))
leaf.onPointerUp(ev(10, 10))
expect("sprite capture", log, ["S:down(S)", "G:down(S)", "root:down(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:up(S)", "G:up(S)", "root:up(S)"])

// G. Hover: enter/leave on the sprite alone, moves walk with the hit; an
// empty-space move reaches the root with sprite null.
log = []
leaf.onPointerMove(ev(100, 100))
leaf.onPointerMove(ev(101, 100))
leaf.onPointerMove(ev(10, 10))
leaf.onPointerMove(ev(300, 100))
leaf.onPointerLeave(ev(300, 100))
expect("hover", log, ["S:enter", "S:move(S)", "G:move(S)", "root:move(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:leave", "root:move(-)", "T:enter", "T:move(T)", "root:move(T)", "T:leave"])

// H. Wheel walks like a move, with the deltas.
log = []
leaf.onWheel(ev(100, 100, 1, { deltaY: 3 }))
leaf.onWheel(ev(10, 10, 1, { deltaY: -2 }))
expect("wheel", log, ["S:wheel(S,3)", "G:wheel(S,3)", "root:wheel(S,3)", "root:wheel(-,-2)"])

// I. Tap rules: travel past the slop is a drag; a release off the target
// is nothing; repeats count within the window, on the same target, near
// the same spot; a new target or a lapse restarts at 1.
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerMove(ev(110, 100))
leaf.onPointerUp(ev(100, 100))
expect("drag is no tap", log, ["S:down(S)", "G:down(S)", "root:down(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
log = []
leaf.onPointerDown(ev(119, 100))
leaf.onPointerUp(ev(124, 100))
expect("release off target", log, ["S:down(S)", "G:down(S)", "root:down(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
log = []
clock = 1000
leaf.onPointerDown(ev(100, 100))
leaf.onPointerUp(ev(100, 100))
clock = 1200
leaf.onPointerDown(ev(105, 100))
leaf.onPointerUp(ev(105, 100))
clock = 1400
leaf.onPointerDown(ev(100, 100))
leaf.onPointerUp(ev(100, 100))
clock = 1800
leaf.onPointerDown(ev(100, 100))
leaf.onPointerUp(ev(100, 100))
clock = 1900
leaf.onPointerDown(ev(300, 100))
leaf.onPointerUp(ev(300, 100))
clock = 2000
leaf.onPointerDown(ev(10, 10))
leaf.onPointerUp(ev(10, 10))
clock = 2100
leaf.onPointerDown(ev(40, 10))
leaf.onPointerUp(ev(40, 10))
clock = 2200
leaf.onPointerDown(ev(45, 10))
leaf.onPointerUp(ev(45, 10))
expect(
  "tap count",
  log.filter(l => l.includes("tap")),
  ["S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100", "S:tap(S#2)@105,100", "G:tap(S#2)@105,100", "root:tap(S#2)@105,100", "S:tap(S#3)@100,100", "G:tap(S#3)@100,100", "root:tap(S#3)@100,100", "S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100", "T:tap(T#1)@300,100", "root:tap(T#1)@300,100", "root:tap(-#1)@10,10", "root:tap(-#1)@40,10", "root:tap(-#2)@45,10"],
)

// J. A second pointer down during a press ends "alone" for both: neither
// release taps, even without travel.
log = []
leaf.onPointerDown(ev(100, 100, 1))
leaf.onPointerDown(ev(10, 10, 2))
leaf.onPointerUp(ev(100, 100, 1))
leaf.onPointerUp(ev(10, 10, 2))
expect("two pointers never tap", log, ["S:down(S)", "G:down(S)", "root:down(S)", "root:down(-)", "S:up(S)", "G:up(S)", "root:up(S)", "root:up(-)"])

// K. Layout scaling and the camera undo: a 200x100 leaf over a 400x200
// layer doubles, and zoom 2 halves.
let scaled = dispatch(() => ({ width: 200, height: 100 }))
log = []
let seen: [number, number] | null = null
listeners.add({ onPointerDown: e => (seen = [e.x, e.y]) })
scaled.onPointerDown(ev(50, 50))
scaled.onPointerUp(ev(50, 50))
if (!seen || seen[0] !== 100 || seen[1] !== 100) fail(`layout scaling: got ${seen}`)
camera = { x: 0, y: 0, zoom: 2, rotation: 0, pivotX: 0, pivotY: 0 }
seen = null
leaf.onPointerDown(ev(100, 100))
leaf.onPointerUp(ev(100, 100))
if (!seen || seen[0] !== 50 || seen[1] !== 50) fail(`camera undo: got ${seen}`)
camera = { x: 0, y: 0, zoom: 1, rotation: 0, pivotX: 0, pivotY: 0 }

// L. Listeners run in registration order; a remover removes.
listeners.clear()
let order: string[] = []
let removeA = (() => {
  let l: LayerPointerListener = { onPointerDown: () => order.push("a") }
  listeners.add(l)
  return () => listeners.delete(l)
})()
listeners.add({ onPointerDown: () => order.push("b") })
leaf.onPointerDown(ev(10, 10))
leaf.onPointerUp(ev(10, 10))
removeA()
leaf.onPointerDown(ev(10, 10))
leaf.onPointerUp(ev(10, 10))
expect("listener order and removal", order, ["a", "b", "b"])
listeners.clear()
listeners.add(rootHandlers)

// M. A sprite removed mid-press (inert handle) drops out of the chain;
// the press still reaches the root, nothing throws.
let M = sprite("M", 100, 100, 40, 40, G)
log = []
leaf.onPointerDown(ev(100, 100))
;(M as { layer: SpriteLayer | null }).layer = null
;(M as { _parent: SpriteGroup | null })._parent = null
leaf.onPointerMove(ev(102, 102))
leaf.onPointerUp(ev(102, 102))
expect("inert mid-press", log, ["M:down(M)", "G:down(M)", "root:down(M)", "root:move(M)", "root:up(M)"])
sprites.pop()

// N. An up this layer never saw go down delivers to what is under it.
log = []
leaf.onPointerUp(ev(300, 100, 7))
expect("orphan up", log, ["T:up(T)", "root:up(T)"])

if (failures > 0) throw new Error(`${failures} dispatch check(s) failed`)
console.log("DISPATCH-OK")
