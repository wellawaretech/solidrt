// Checks for the scene's pointer dispatch (scene-pointer.ts): the walk
// from the struck node - the instance of an instanced mesh, else the mesh
// - through its ancestors to the scene root, misses reaching the root
// alone, stopPropagation claiming (a stopped down keeps the root out of
// the whole press), capture per pointer to node or root with the hit
// point going null off the target, hover pairing per instance, wheel
// through the same walk, tap synthesis (slop, the alone rule, same-target
// release per instance, the repeat count and its window), layout scaling,
// and a node that left the scene mid-press. Pure-module input only
// (scene-pointer.ts imports types), so it runs headless on flux, bundled
// from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/dispatch-check.ts | target/release/flux -
//
// A failure prints FAIL lines and throws at the end, and the flux binary
// exits 1 on the uncaught throw, so a CI step can gate on the exit code.
// The live side (real element events off the leaf, the orbit control fed
// at the root) is exercised by examples/pick.tsx. @solidrt/2d's
// dispatch-check.ts is the same rig one dimension down.

import type { PointerEvent as ElementPointerEvent, WheelEvent as ElementWheelEvent } from "@solidrt/core"
import { makePointerInput } from "../src/scene-pointer.ts"
import type { SceneNode, ScenePointerListener } from "../src/node.ts"
import type { InstanceNode, Mesh } from "../src/mesh.ts"
import type { Hit, Scene } from "../src/scene.ts"

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

// A fake scene in scene pixels: named nodes whose handlers log
// "name:event(tag)" - the tag names the mesh and the instance the event
// carries, plus tap counts - into `log`, and whose pick is a centered
// rectangle test over the pickable nodes, the last added on top (nearest
// first).
// A node in the scene has a truthy `_scene`; `_scene` null is one that
// left it.
type Fake = { name: string; x: number; y: number; w: number; h: number }
type Pickable = { mesh: Mesh & Fake; instance: (InstanceNode & Fake) | null }
let log: string[] = []
let root = { name: "root" } as unknown as Scene
let listeners = new Set<ScenePointerListener>()
let pickables: Pickable[] = []
let clock = 0
let size = { width: 400, height: 200 }

let tag = (e: { mesh: Mesh | null; instance: InstanceNode | null; tapCount?: number }) =>
  (e.mesh ? (e.mesh as Mesh & Fake).name : "-") + (e.instance ? "/" + (e.instance as InstanceNode & Fake).name : "") + (e.tapCount ? "#" + e.tapCount : "")
let handlers = (name: string, stop: Set<string> = new Set()) => ({
  onPointerDown(e: { mesh: Mesh | null; instance: InstanceNode | null; stopPropagation(): void }) {
    log.push(`${name}:down(${tag(e)})`)
    if (stop.has("down")) e.stopPropagation()
  },
  onPointerMove(e: { mesh: Mesh | null; instance: InstanceNode | null; stopPropagation(): void }) {
    log.push(`${name}:move(${tag(e)})`)
    if (stop.has("move")) e.stopPropagation()
  },
  onPointerUp(e: { mesh: Mesh | null; instance: InstanceNode | null }) {
    log.push(`${name}:up(${tag(e)})`)
  },
  onWheel(e: { mesh: Mesh | null; instance: InstanceNode | null; deltaY: number; stopPropagation(): void }) {
    log.push(`${name}:wheel(${tag(e)},${e.deltaY})`)
    if (stop.has("wheel")) e.stopPropagation()
  },
  onTap(e: { mesh: Mesh | null; instance: InstanceNode | null; tapCount: number; x: number; y: number }) {
    log.push(`${name}:tap(${tag(e)})@${e.x},${e.y}`)
  },
})
let inScene = {} as SceneNode["_scene"]
function group(name: string, parent: SceneNode | null = null, stop?: Set<string>): SceneNode {
  return { kind: "group", parent, children: [], _scene: inScene, ...handlers(name, stop) } as unknown as SceneNode
}
function mesh(name: string, x: number, y: number, w: number, h: number, parent: SceneNode | null = null, stop?: Set<string>): Mesh & Fake {
  let m = {
    kind: "mesh",
    parent,
    children: [],
    _scene: inScene,
    name,
    x,
    y,
    w,
    h,
    ...handlers(name, stop),
    onPointerEnter: () => log.push(`${name}:enter`),
    onPointerLeave: () => log.push(`${name}:leave`),
  } as unknown as Mesh & Fake
  pickables.push({ mesh: m, instance: null })
  return m
}
// An instance of `owner` at its own rectangle: picked as the owner mesh
// with the instance named, the walk starting at the instance.
function instance(name: string, owner: Mesh & Fake, x: number, y: number, w: number, h: number, stop?: Set<string>): InstanceNode & Fake {
  let i = {
    kind: "instance",
    parent: owner,
    children: [],
    _scene: inScene,
    mesh: owner,
    name,
    x,
    y,
    w,
    h,
    ...handlers(name, stop),
    onPointerEnter: () => log.push(`${name}:enter`),
    onPointerLeave: () => log.push(`${name}:leave`),
  } as unknown as InstanceNode & Fake
  pickables.push({ mesh: owner, instance: i })
  return i
}
let inside = (p: Fake, x: number, y: number) => Math.abs(x - p.x) < p.w / 2 && Math.abs(y - p.y) < p.h / 2
let pick = (x: number, y: number): Hit[] => {
  let out: Hit[] = []
  for (let i = pickables.length - 1; i >= 0; i--) {
    let p = pickables[i]!
    let shape = p.instance ?? p.mesh
    if (shape._scene !== null && inside(shape, x, y)) {
      let hit: Hit = { mesh: p.mesh, distance: 1, point: [x, y, 0], normal: [0, 0, 1] }
      if (p.instance) hit.instance = p.instance
      out.push(hit)
    }
  }
  return out
}
let dispatch = makePointerInput({
  pick,
  targetSize: () => size,
  root,
  listeners,
  now: () => clock,
})
let leaf = dispatch.handlers
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

// World: group G holds mesh S (a 40x40 square centered at 100,100); T
// stands alone at 300,100; instanced mesh IM (no rectangle of its own)
// has instances I1 at 300,20 and I2 right below at 300,60. Empty space
// everywhere else.
let G = group("G")
let S = mesh("S", 100, 100, 40, 40, G)
let T = mesh("T", 300, 100, 40, 40)
let IM = mesh("IM", -1, -1, 0, 0)
let I1 = instance("I1", IM, 300, 20, 40, 40)
let I2 = instance("I2", IM, 300, 60, 40, 40)

// A. A miss walks the root alone, mesh null.
log = []
leaf.onPointerDown(ev(10, 10))
leaf.onPointerMove(ev(12, 12))
leaf.onPointerUp(ev(12, 12))
expect("miss press", log, ["root:down(-)", "root:move(-)", "root:up(-)", "root:tap(-#1)@12,12"])

// B. A hit bubbles mesh -> group -> root, the mesh constant; an instance
// hit starts at the instance, then its mesh, then the root.
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerUp(ev(100, 100))
expect("hit walk", log, ["S:down(S)", "G:down(S)", "root:down(S)", "S:up(S)", "G:up(S)", "root:up(S)", "S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100"])
log = []
leaf.onPointerDown(ev(300, 20))
leaf.onPointerUp(ev(300, 20))
expect("instance walk", log, ["I1:down(IM/I1)", "IM:down(IM/I1)", "root:down(IM/I1)", "I1:up(IM/I1)", "IM:up(IM/I1)", "root:up(IM/I1)", "I1:tap(IM/I1#1)@300,20", "IM:tap(IM/I1#1)@300,20", "root:tap(IM/I1#1)@300,20"])

// C. A stopped down claims the press: the chain keeps bubbling, the root
// never sees that pointer again (move, up, tap included).
let C = mesh("C", 100, 100, 40, 40, G, new Set(["down"]))
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerMove(ev(150, 150))
leaf.onPointerUp(ev(150, 150))
expect("claimed press", log, ["C:down(C)", "C:move(C)", "G:move(C)", "C:up(C)", "G:up(C)"])
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerUp(ev(101, 101))
expect("claimed tap", log, ["C:down(C)", "C:up(C)", "G:up(C)", "C:tap(C#1)@101,101", "G:tap(C#1)@101,101"])
pickables.pop()

// D. A group stopping a move keeps the root out of that event only.
let H = group("H", null, new Set(["move"]))
let D = mesh("D", 100, 100, 40, 40, H)
log = []
leaf.onPointerDown(ev(100, 100))
leaf.onPointerMove(ev(102, 102))
leaf.onPointerUp(ev(102, 102))
expect("group stop per event", log, ["D:down(D)", "H:down(D)", "root:down(D)", "D:move(D)", "H:move(D)", "D:up(D)", "H:up(D)", "root:up(D)", "D:tap(D#1)@102,102", "H:tap(D#1)@102,102", "root:tap(D#1)@102,102"])
pickables.pop()

// E. Root capture: a press from empty space stays with the root as it
// crosses meshes - no enter, no mesh handlers, mesh stays null.
log = []
leaf.onPointerDown(ev(10, 10))
leaf.onPointerMove(ev(100, 100))
leaf.onPointerUp(ev(300, 100))
expect("root capture", log, ["root:down(-)", "root:move(-)", "root:up(-)"])

// F. Node capture: a press on S keeps naming S off the mesh, with the hit
// point null there and back when the ray strikes it again.
log = []
let points: (string | null)[] = []
let pointListener: ScenePointerListener = { onPointerMove: e => points.push(e.point ? "hit" : null) }
listeners.add(pointListener)
leaf.onPointerDown(ev(100, 100))
leaf.onPointerMove(ev(10, 10))
leaf.onPointerMove(ev(110, 110))
leaf.onPointerUp(ev(10, 10))
listeners.delete(pointListener)
expect("node capture", log, ["S:down(S)", "G:down(S)", "root:down(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
expect("captured point", points.map(p => String(p)), ["null", "hit"])

// G. Hover: enter/leave on the struck node alone (per instance on an
// instanced mesh), moves walk with the hit; an empty-space move reaches
// the root with mesh null.
log = []
leaf.onPointerMove(ev(100, 100))
leaf.onPointerMove(ev(101, 100))
leaf.onPointerMove(ev(10, 10))
leaf.onPointerMove(ev(300, 20))
leaf.onPointerMove(ev(300, 60))
leaf.onPointerLeave(ev(300, 60))
expect("hover", log, [
  "S:enter", "S:move(S)", "G:move(S)", "root:move(S)",
  "S:move(S)", "G:move(S)", "root:move(S)",
  "S:leave", "root:move(-)",
  "I1:enter", "I1:move(IM/I1)", "IM:move(IM/I1)", "root:move(IM/I1)",
  "I1:leave", "I2:enter", "I2:move(IM/I2)", "IM:move(IM/I2)", "root:move(IM/I2)",
  "I2:leave",
])

// H. Wheel walks like a move, with the deltas; a stopped wheel never
// reaches the root.
log = []
leaf.onWheel(ev(100, 100, 1, { deltaY: 3 }))
leaf.onWheel(ev(10, 10, 1, { deltaY: -2 }))
let W = mesh("W", 100, 100, 40, 40, G, new Set(["wheel"]))
leaf.onWheel(ev(100, 100, 1, { deltaY: 5 }))
pickables.pop()
expect("wheel", log, ["S:wheel(S,3)", "G:wheel(S,3)", "root:wheel(S,3)", "root:wheel(-,-2)", "W:wheel(W,5)"])

// I. Tap rules: travel past the slop is a drag; a release off the target
// is nothing (another instance of the same mesh included); repeats count
// within the window, on the same target, near the same spot; a new target
// or a lapse restarts at 1.
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
leaf.onPointerDown(ev(300, 38))
leaf.onPointerUp(ev(300, 42))
expect("release on another instance", log, ["I1:down(IM/I1)", "IM:down(IM/I1)", "root:down(IM/I1)", "I1:up(IM/I1)", "IM:up(IM/I1)", "root:up(IM/I1)"])
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
clock = 2300
leaf.onPointerDown(ev(300, 20))
leaf.onPointerUp(ev(300, 20))
clock = 2400
leaf.onPointerDown(ev(300, 25))
leaf.onPointerUp(ev(300, 25))
expect(
  "tap count",
  log.filter(l => l.includes("tap")),
  [
    "S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100",
    "S:tap(S#2)@105,100", "G:tap(S#2)@105,100", "root:tap(S#2)@105,100",
    "S:tap(S#3)@100,100", "G:tap(S#3)@100,100", "root:tap(S#3)@100,100",
    "S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100",
    "T:tap(T#1)@300,100", "root:tap(T#1)@300,100",
    "root:tap(-#1)@10,10",
    "root:tap(-#1)@40,10",
    "root:tap(-#2)@45,10",
    "I1:tap(IM/I1#1)@300,20", "IM:tap(IM/I1#1)@300,20", "root:tap(IM/I1#1)@300,20",
    "I1:tap(IM/I1#2)@300,25", "IM:tap(IM/I1#2)@300,25", "root:tap(IM/I1#2)@300,25",
  ],
)

// J. A second pointer down during a press ends "alone" for both: neither
// release taps, even without travel.
log = []
leaf.onPointerDown(ev(100, 100, 1))
leaf.onPointerDown(ev(10, 10, 2))
leaf.onPointerUp(ev(100, 100, 1))
leaf.onPointerUp(ev(10, 10, 2))
expect("two pointers never tap", log, ["S:down(S)", "G:down(S)", "root:down(S)", "root:down(-)", "S:up(S)", "G:up(S)", "root:up(S)", "root:up(-)"])

// K. Layout scaling: a 200x100 leaf over a 400x200 target doubles.
let scaled = dispatch.handlersFor(() => ({ width: 200, height: 100 }))
let seen: [number, number] | null = null
listeners.add({ onPointerDown: e => (seen = [e.x, e.y]) })
scaled.onPointerDown(ev(50, 50))
scaled.onPointerUp(ev(50, 50))
if (!seen || seen[0] !== 100 || seen[1] !== 100) fail(`layout scaling: got ${seen}`)

// L. Listeners run in registration order; a remover removes.
listeners.clear()
let order: string[] = []
let removeA = (() => {
  let l: ScenePointerListener = { onPointerDown: () => order.push("a") }
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

// M. A node that left the scene mid-press drops out of the chain; the
// press still reaches the root, nothing throws.
let M = mesh("M", 100, 100, 40, 40, G)
log = []
leaf.onPointerDown(ev(100, 100))
;(M as { _scene: SceneNode["_scene"] })._scene = null
;(M as { parent: SceneNode | null }).parent = null
leaf.onPointerMove(ev(102, 102))
leaf.onPointerUp(ev(102, 102))
expect("left mid-press", log, ["M:down(M)", "G:down(M)", "root:down(M)", "root:move(M)", "root:up(M)"])
pickables.pop()

// N. An up this root never saw go down delivers to what is under it.
log = []
leaf.onPointerUp(ev(300, 100, 7))
expect("orphan up", log, ["T:up(T)", "root:up(T)"])

// O. The root is the event's currentTarget at the last stop, and every
// event carries the leaf's element event.
let last: { currentTarget: unknown; native: ElementPointerEvent } | null = null
listeners.add({ onPointerDown: e => (last = e) })
let down = ev(10, 10)
leaf.onPointerDown(down)
leaf.onPointerUp(ev(10, 10))
if (!last || (last as { currentTarget: unknown }).currentTarget !== root) fail("root currentTarget")
if (!last || (last as { native: ElementPointerEvent }).native !== down) fail("native element event")

if (failures > 0) throw new Error(`${failures} dispatch check(s) failed`)
console.log("DISPATCH-OK")
