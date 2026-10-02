// Tests for the scene's pointer dispatch (scene-pointer.ts): the walk from
// the struck node - the instance of an instanced mesh, else the mesh -
// through its ancestors to the scene root, misses reaching the root alone,
// stopPropagation claiming (a stopped down keeps the root out of the whole
// press), capture per pointer to node or root with the hit point going null
// off the target, hover pairing per instance, wheel through the same walk,
// tap synthesis (slop, the alone rule, same-target release per instance,
// the repeat count and its window), layout scaling, and a node that left
// the scene mid-press. Pure-module input only (scene-pointer.ts imports
// types), so it runs headless on flux: `sol test packages/3d`. The live
// side (real element events off the leaf, the orbit control fed at the
// root) is exercised by examples/pick.tsx. tests/dispatch.test.ts in
// @solidrt/2d is the same test one dimension down.
//
// Each test builds a fake scene of its own (`world()`), so none starts from
// the state another left (what is hovered, the tap history).

import { test } from "flux:test"
import type { PointerEvent as ElementPointerEvent, WheelEvent as ElementWheelEvent } from "@solidrt/core"
import { makePointerInput } from "../src/scene-pointer.ts"
import type { SceneNode, ScenePointerListener } from "../src/node.ts"
import type { InstanceNode, Mesh } from "../src/mesh.ts"
import type { Hit, Scene } from "../src/scene.ts"

function fail(msg: string): void {
  throw new Error(msg)
}
function expect(name: string, got: string[], want: string[]) {
  let g = got.join(" ")
  let w = want.join(" ")
  if (g !== w) fail(`${name}: got [${g}] want [${w}]`)
}

type Fake = { name: string; x: number; y: number; w: number; h: number }
type Pickable = { mesh: Mesh & Fake; instance: (InstanceNode & Fake) | null }

// A fake scene in scene pixels: named nodes whose handlers log
// "name:event(tag)" - the tag names the mesh and the instance the event
// carries, plus tap counts - and whose pick is a centered rectangle test
// over the pickable nodes, the last added on top (nearest first).
// A node in the scene has a truthy `_scene`; `_scene` null is one that
// left it.
// In it: group G holds mesh S (a 40x40 square centered at 100,100); T
// stands alone at 300,100; instanced mesh IM (no rectangle of its own)
// has instances I1 at 300,20 and I2 right below at 300,60. Empty space
// everywhere else.
function world() {
  let log: string[] = []
  let root = { name: "root" } as unknown as Scene
  let listeners = new Set<ScenePointerListener>()
  let pickables: Pickable[] = []
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
  let group = (name: string, parent: SceneNode | null = null, stop?: Set<string>): SceneNode =>
    ({ kind: "group", parent, children: [], _scene: inScene, ...handlers(name, stop) }) as unknown as SceneNode
  let mesh = (name: string, x: number, y: number, w: number, h: number, parent: SceneNode | null = null, stop?: Set<string>): Mesh & Fake => {
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
  let instance = (name: string, owner: Mesh & Fake, x: number, y: number, w: number, h: number, stop?: Set<string>): InstanceNode & Fake => {
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
  })
  listeners.add(handlers("root"))
  let G = group("G")
  mesh("S", 100, 100, 40, 40, G)
  mesh("T", 300, 100, 40, 40)
  let IM = mesh("IM", -1, -1, 0, 0)
  instance("I1", IM, 300, 20, 40, 40)
  instance("I2", IM, 300, 60, 40, 40)

  return {
    root,
    listeners,
    dispatch,
    leaf: dispatch.handlers,
    group,
    mesh,
    G,
    // What the handlers logged since the last call.
    logged: (): string[] => log.splice(0),
  }
}

function ev(localX: number, localY: number, pointerId = 1, extra: Partial<ElementWheelEvent> = {}): ElementWheelEvent {
  return {
    timeStamp: 0,
    predicted: false,
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

test("a miss walks the root alone, mesh null", () => {
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(10, 10))
  leaf.onPointerMove(ev(12, 12))
  leaf.onPointerUp(ev(12, 12))
  expect("miss press", logged(), ["root:down(-)", "root:move(-)", "root:up(-)", "root:tap(-#1)@12,12"])
})

test("a hit bubbles mesh -> group -> root; an instance hit starts at the instance", () => {
  // B. A hit bubbles mesh -> group -> root, the mesh constant; an instance
  // hit starts at the instance, then its mesh, then the root.
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerUp(ev(100, 100))
  expect("hit walk", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:up(S)", "G:up(S)", "root:up(S)", "S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100"])
  leaf.onPointerDown(ev(300, 20))
  leaf.onPointerUp(ev(300, 20))
  expect("instance walk", logged(), ["I1:down(IM/I1)", "IM:down(IM/I1)", "root:down(IM/I1)", "I1:up(IM/I1)", "IM:up(IM/I1)", "root:up(IM/I1)", "I1:tap(IM/I1#1)@300,20", "IM:tap(IM/I1#1)@300,20", "root:tap(IM/I1#1)@300,20"])
})

test("a stopped down claims the press", () => {
  // C. A stopped down claims the press: the chain keeps bubbling, the root
  // never sees that pointer again (move, up, tap included).
  let { leaf, logged, mesh, G } = world()
  mesh("C", 100, 100, 40, 40, G, new Set(["down"]))
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerMove(ev(150, 150))
  leaf.onPointerUp(ev(150, 150))
  expect("claimed press", logged(), ["C:down(C)", "C:move(C)", "G:move(C)", "C:up(C)", "G:up(C)"])
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerUp(ev(101, 101))
  expect("claimed tap", logged(), ["C:down(C)", "C:up(C)", "G:up(C)", "C:tap(C#1)@101,101", "G:tap(C#1)@101,101"])
})

test("a group stopping a move keeps the root out of that event only", () => {
  let { leaf, logged, group, mesh } = world()
  let H = group("H", null, new Set(["move"]))
  mesh("D", 100, 100, 40, 40, H)
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerMove(ev(102, 102))
  leaf.onPointerUp(ev(102, 102))
  expect("group stop per event", logged(), ["D:down(D)", "H:down(D)", "root:down(D)", "D:move(D)", "H:move(D)", "D:up(D)", "H:up(D)", "root:up(D)", "D:tap(D#1)@102,102", "H:tap(D#1)@102,102", "root:tap(D#1)@102,102"])
})

test("root capture: a press from empty space stays with the root as it crosses meshes", () => {
  // E. Root capture: a press from empty space stays with the root as it
  // crosses meshes - no enter, no mesh handlers, mesh stays null.
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(10, 10))
  leaf.onPointerMove(ev(100, 100))
  leaf.onPointerUp(ev(300, 100))
  expect("root capture", logged(), ["root:down(-)", "root:move(-)", "root:up(-)"])
})

test("node capture: a press on S keeps naming S off the mesh, the hit point null there", () => {
  // F. Node capture: a press on S keeps naming S off the mesh, with the hit
  // point null there and back when the ray strikes it again.
  let { leaf, logged, listeners } = world()
  let points: (string | null)[] = []
  listeners.add({ onPointerMove: e => points.push(e.point ? "hit" : null) })
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerMove(ev(10, 10))
  leaf.onPointerMove(ev(110, 110))
  leaf.onPointerUp(ev(10, 10))
  expect("node capture", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
  expect("captured point", points.map(p => String(p)), ["null", "hit"])
})

test("hover: enter and leave on the struck node alone, moves walk with the hit", () => {
  // G. Hover: enter/leave on the struck node alone (per instance on an
  // instanced mesh), moves walk with the hit; an empty-space move reaches
  // the root with mesh null.
  let { leaf, logged } = world()
  leaf.onPointerMove(ev(100, 100))
  leaf.onPointerMove(ev(101, 100))
  leaf.onPointerMove(ev(10, 10))
  leaf.onPointerMove(ev(300, 20))
  leaf.onPointerMove(ev(300, 60))
  leaf.onPointerLeave(ev(300, 60))
  expect("hover", logged(), [
    "S:enter", "S:move(S)", "G:move(S)", "root:move(S)",
    "S:move(S)", "G:move(S)", "root:move(S)",
    "S:leave", "root:move(-)",
    "I1:enter", "I1:move(IM/I1)", "IM:move(IM/I1)", "root:move(IM/I1)",
    "I1:leave", "I2:enter", "I2:move(IM/I2)", "IM:move(IM/I2)", "root:move(IM/I2)",
    "I2:leave",
  ])
})

test("wheel walks like a move, with the deltas; a stopped wheel never reaches the root", () => {
  // H. Wheel walks like a move, with the deltas; a stopped wheel never
  // reaches the root.
  let { leaf, logged, mesh, G } = world()
  leaf.onWheel(ev(100, 100, 1, { deltaY: 3 }))
  leaf.onWheel(ev(10, 10, 1, { deltaY: -2 }))
  mesh("W", 100, 100, 40, 40, G, new Set(["wheel"]))
  leaf.onWheel(ev(100, 100, 1, { deltaY: 5 }))
  expect("wheel", logged(), ["S:wheel(S,3)", "G:wheel(S,3)", "root:wheel(S,3)", "root:wheel(-,-2)", "W:wheel(W,5)"])
})

test("tap rules: slop, release off the target, repeats, restarts", () => {
  // I. Tap rules: travel past the slop is a drag; a release off the target
  // is nothing (another instance of the same mesh included); repeats count
  // within the window, on the same target, near the same spot; a new target
  // or a lapse restarts at 1.
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerMove(ev(110, 100))
  leaf.onPointerUp(ev(100, 100))
  expect("drag is no tap", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
  leaf.onPointerDown(ev(119, 100))
  leaf.onPointerUp(ev(124, 100))
  expect("release off target", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
  leaf.onPointerDown(ev(300, 38))
  leaf.onPointerUp(ev(300, 42))
  expect("release on another instance", logged(), ["I1:down(IM/I1)", "IM:down(IM/I1)", "root:down(IM/I1)", "I1:up(IM/I1)", "IM:up(IM/I1)", "root:up(IM/I1)"])
  // A tap at (x, y) whose down and up carry the time `at`, ms.
  let tapAt = (at: number, x: number, y: number) => {
    leaf.onPointerDown(ev(x, y, 1, { timeStamp: at }))
    leaf.onPointerUp(ev(x, y, 1, { timeStamp: at }))
  }
  tapAt(1000, 100, 100)
  tapAt(1200, 105, 100)
  tapAt(1400, 100, 100)
  tapAt(1800, 100, 100)
  tapAt(1900, 300, 100)
  tapAt(2000, 10, 10)
  tapAt(2100, 40, 10)
  tapAt(2200, 45, 10)
  tapAt(2300, 300, 20)
  tapAt(2400, 300, 25)
  expect(
    "tap count",
    logged().filter(l => l.includes("tap")),
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
})

test("a second pointer down during a press ends alone for both", () => {
  // J. A second pointer down during a press ends "alone" for both: neither
  // release taps, even without travel.
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(100, 100, 1))
  leaf.onPointerDown(ev(10, 10, 2))
  leaf.onPointerUp(ev(100, 100, 1))
  leaf.onPointerUp(ev(10, 10, 2))
  expect("two pointers never tap", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "root:down(-)", "S:up(S)", "G:up(S)", "root:up(S)", "root:up(-)"])
})

test("layout scaling: a 200x100 leaf over a 400x200 target doubles", () => {
  let { dispatch, listeners } = world()
  let scaled = dispatch.handlersFor(() => ({ width: 200, height: 100 }))
  let seen: [number, number] | null = null
  listeners.add({ onPointerDown: e => (seen = [e.x, e.y]) })
  scaled.onPointerDown(ev(50, 50))
  scaled.onPointerUp(ev(50, 50))
  if (!seen || seen[0] !== 100 || seen[1] !== 100) fail(`layout scaling: got ${seen}`)
})

test("listeners run in registration order; a remover removes", () => {
  let { leaf, listeners } = world()
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
})

test("a node that left the scene mid-press drops out of the chain", () => {
  // M. A node that left the scene mid-press drops out of the chain; the
  // press still reaches the root, nothing throws.
  let { leaf, logged, mesh, G } = world()
  let M = mesh("M", 100, 100, 40, 40, G)
  leaf.onPointerDown(ev(100, 100))
  ;(M as { _scene: SceneNode["_scene"] })._scene = null
  ;(M as { parent: SceneNode | null }).parent = null
  leaf.onPointerMove(ev(102, 102))
  leaf.onPointerUp(ev(102, 102))
  expect("left mid-press", logged(), ["M:down(M)", "G:down(M)", "root:down(M)", "root:move(M)", "root:up(M)"])
})

test("an up this root never saw go down delivers to what is under it", () => {
  let { leaf, logged } = world()
  leaf.onPointerUp(ev(300, 100, 7))
  expect("orphan up", logged(), ["T:up(T)", "root:up(T)"])
})

test("the root is the currentTarget at the last stop, and every event carries the leaf's element event", () => {
  // O. The root is the event's currentTarget at the last stop, and every
  // event carries the leaf's element event.
  let { leaf, listeners, root } = world()
  let last: { currentTarget: unknown; native: ElementPointerEvent } | null = null
  listeners.add({ onPointerDown: e => (last = e) })
  let down = ev(10, 10)
  leaf.onPointerDown(down)
  leaf.onPointerUp(ev(10, 10))
  if (!last || (last as { currentTarget: unknown }).currentTarget !== root) fail("root currentTarget")
  if (!last || (last as { native: ElementPointerEvent }).native !== down) fail("native element event")
})
