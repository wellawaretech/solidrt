// Tests for the layer's pointer dispatch (dispatch.ts): the walk from the
// hit sprite through its groups to the layer root, misses reaching the root
// alone, stopPropagation claiming (a stopped down keeps the root out of the
// whole press), capture per pointer to sprite or root, hover pairing, wheel
// through the same walk, tap synthesis (slop, the alone rule, same-target
// release, the repeat count and its window), layout scaling and the camera
// undo. Pure-module input only (dispatch.ts imports no GUI), so it runs
// headless on flux: `sol test packages/2d`. The live side (real element
// events off the leaf, the camera attached at the root) is exercised by
// examples/camera.tsx and examples/pick.tsx.
//
// Each test builds a fake layer of its own (`world()`), so none starts from
// the state another left (what is hovered, the tap history).

import { test } from "flux:test"
import type { PointerEvent as ElementPointerEvent, WheelEvent as ElementWheelEvent } from "@solidrt/core"
import { spriteDispatch } from "../src/dispatch.ts"
import type { LayerPointerListener, Sprite, SpriteGroup, SpriteLayer } from "../src/layer.ts"
import type { ViewHandle } from "../src/views.ts"
import { pointInSprite } from "../src/pick.ts"
import type { CameraUpdate } from "../src/camera.ts"

function fail(msg: string): void {
  throw new Error(msg)
}
function expect(name: string, got: string[], want: string[]) {
  let g = got.join(" ")
  let w = want.join(" ")
  if (g !== w) fail(`${name}: got [${g}] want [${w}]`)
}

type Fake = { name: string; x: number; y: number; w: number; h: number }

// A fake layer with named sprites and groups whose handlers log
// "name:event" (plus the sprite the event names, and tap counts), and whose
// pick is the real rotated-rect narrowphase over the sprite rects, topmost
// (last added) first. In it: group G holds sprite S (a 40x40 square at
// 100,100); T stands alone at 300,100. Empty space everywhere else.
function world() {
  let log: string[] = []
  let root = { name: "root" } as unknown as ViewHandle
  let listeners = new Set<LayerPointerListener>()
  let sprites: (Sprite & Fake)[] = []
  let size: [number, number] = [400, 200]
  // What a test moves: the time the dispatch reads, and the camera.
  let state: { camera: CameraUpdate } = { camera: { x: 0, y: 0, zoom: 1, rotation: 0, pivotX: 0, pivotY: 0 } }

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
    onPointerCancel(e: { sprite: Sprite | null }) {
      log.push(`${name}:cancel(${tag(e)})`)
    },
    onWheel(e: { sprite: Sprite | null; deltaY: number }) {
      log.push(`${name}:wheel(${tag(e)},${e.deltaY})`)
    },
    onTap(e: { sprite: Sprite | null; tapCount: number; x: number; y: number }) {
      log.push(`${name}:tap(${tag(e)})@${e.x},${e.y}`)
    },
  })
  let group = (name: string, parent: SpriteGroup | null = null, stop?: Set<string>): SpriteGroup =>
    ({ layer: root, _parent: parent, _children: new Set(), ...handlers(name, stop) }) as unknown as SpriteGroup
  let sprite = (name: string, x: number, y: number, w: number, h: number, parent: SpriteGroup | null = null, stop?: Set<string>): Sprite & Fake => {
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
    camera: () => state.camera,
    pick,
    root,
    listeners,
  })
  listeners.add(handlers("root"))
  let G = group("G")
  sprite("S", 100, 100, 40, 40, G)
  sprite("T", 300, 100, 40, 40)

  return {
    state,
    listeners,
    dispatch,
    leaf: dispatch(null),
    group,
    sprite,
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

test("a miss walks the root alone, sprite null", () => {
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(10, 10))
  leaf.onPointerMove(ev(12, 12))
  leaf.onPointerUp(ev(12, 12))
  expect("miss press", logged(), ["root:down(-)", "root:move(-)", "root:up(-)", "root:tap(-#1)@12,12"])
})

test("a hit bubbles sprite -> group -> root, the sprite constant", () => {
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerUp(ev(100, 100))
  expect("hit walk", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:up(S)", "G:up(S)", "root:up(S)", "S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100"])
})

test("a stopped down claims the press", () => {
  // C. A stopped down claims the press: the chain keeps bubbling, the root
  // never sees that pointer again (move, up, tap included).
  let { leaf, logged, sprite, G } = world()
  sprite("C", 100, 100, 40, 40, G, new Set(["down"]))
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerMove(ev(150, 150))
  leaf.onPointerUp(ev(150, 150))
  expect("claimed press", logged(), ["C:down(C)", "C:move(C)", "G:move(C)", "C:up(C)", "G:up(C)"])
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerUp(ev(101, 101))
  expect("claimed tap", logged(), ["C:down(C)", "C:up(C)", "G:up(C)", "C:tap(C#1)@101,101", "G:tap(C#1)@101,101"])
})

test("a group stopping a move keeps the root out of that event only", () => {
  let { leaf, logged, group, sprite } = world()
  let H = group("H", null, new Set(["move"]))
  sprite("D", 100, 100, 40, 40, H)
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerMove(ev(102, 102))
  leaf.onPointerUp(ev(102, 102))
  expect("group stop per event", logged(), ["D:down(D)", "H:down(D)", "root:down(D)", "D:move(D)", "H:move(D)", "D:up(D)", "H:up(D)", "root:up(D)", "D:tap(D#1)@102,102", "H:tap(D#1)@102,102", "root:tap(D#1)@102,102"])
})

test("root capture: a press from empty space stays with the root as it crosses sprites", () => {
  // E. Root capture: a press from empty space stays with the root as it
  // crosses sprites - no enter, no sprite handlers, sprite stays null.
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(10, 10))
  leaf.onPointerMove(ev(100, 100))
  leaf.onPointerUp(ev(300, 100))
  expect("root capture", logged(), ["root:down(-)", "root:move(-)", "root:up(-)"])
})

test("sprite capture: a press on S keeps naming S off the sprite", () => {
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerMove(ev(10, 10))
  leaf.onPointerUp(ev(10, 10))
  expect("sprite capture", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
})

test("hover: enter and leave on the sprite alone, moves walk with the hit", () => {
  // G. Hover: enter/leave on the sprite alone, moves walk with the hit; an
  // empty-space move reaches the root with sprite null.
  let { leaf, logged } = world()
  leaf.onPointerMove(ev(100, 100))
  leaf.onPointerMove(ev(101, 100))
  leaf.onPointerMove(ev(10, 10))
  leaf.onPointerMove(ev(300, 100))
  leaf.onPointerLeave(ev(300, 100))
  expect("hover", logged(), ["S:enter", "S:move(S)", "G:move(S)", "root:move(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:leave", "root:move(-)", "T:enter", "T:move(T)", "root:move(T)", "T:leave"])
})

test("wheel walks like a move, with the deltas", () => {
  let { leaf, logged } = world()
  leaf.onWheel(ev(100, 100, 1, { deltaY: 3 }))
  leaf.onWheel(ev(10, 10, 1, { deltaY: -2 }))
  expect("wheel", logged(), ["S:wheel(S,3)", "G:wheel(S,3)", "root:wheel(S,3)", "root:wheel(-,-2)"])
})

test("tap rules: slop, release off the target, repeats, restarts", () => {
  // I. Tap rules: travel past the slop is a drag; a release off the target
  // is nothing; repeats count within the window, on the same target, near
  // the same spot; a new target or a lapse restarts at 1.
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerMove(ev(110, 100))
  leaf.onPointerUp(ev(100, 100))
  expect("drag is no tap", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:move(S)", "G:move(S)", "root:move(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
  leaf.onPointerDown(ev(119, 100))
  leaf.onPointerUp(ev(124, 100))
  expect("release off target", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
  // A secondary button walks its down and up but never taps (DOM click
  // is primary-only), and leaves no tap history for the next left tap.
  leaf.onPointerDown(ev(100, 100, 1, { button: 2 }))
  leaf.onPointerUp(ev(100, 100, 1, { button: 2 }))
  expect("right click is no tap", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:up(S)", "G:up(S)", "root:up(S)"])
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
  expect(
    "tap count",
    logged().filter(l => l.includes("tap")),
    ["S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100", "S:tap(S#2)@105,100", "G:tap(S#2)@105,100", "root:tap(S#2)@105,100", "S:tap(S#3)@100,100", "G:tap(S#3)@100,100", "root:tap(S#3)@100,100", "S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100", "T:tap(T#1)@300,100", "root:tap(T#1)@300,100", "root:tap(-#1)@10,10", "root:tap(-#1)@40,10", "root:tap(-#2)@45,10"],
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

test("layout scaling and the camera undo", () => {
  // K. Layout scaling and the camera undo: a 200x100 leaf over a 400x200
  // layer doubles, and zoom 2 halves.
  let { leaf, dispatch, listeners, state } = world()
  let scaled = dispatch(() => ({ width: 200, height: 100 }))
  let seen: [number, number] | null = null
  listeners.add({ onPointerDown: e => (seen = [e.x, e.y]) })
  scaled.onPointerDown(ev(50, 50))
  scaled.onPointerUp(ev(50, 50))
  if (!seen || seen[0] !== 100 || seen[1] !== 100) fail(`layout scaling: got ${seen}`)
  state.camera = { x: 0, y: 0, zoom: 2, rotation: 0, pivotX: 0, pivotY: 0 }
  seen = null
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerUp(ev(100, 100))
  if (!seen || seen[0] !== 50 || seen[1] !== 50) fail(`camera undo: got ${seen}`)
})

test("listeners run in registration order; a remover removes", () => {
  let { leaf, listeners } = world()
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
})

test("a sprite removed mid-press drops out of the chain", () => {
  // M. A sprite removed mid-press (inert handle) drops out of the chain;
  // the press still reaches the root, nothing throws.
  let { leaf, logged, sprite, G } = world()
  let M = sprite("M", 100, 100, 40, 40, G)
  leaf.onPointerDown(ev(100, 100))
  ;(M as { layer: SpriteLayer | null }).layer = null
  ;(M as { _parent: SpriteGroup | null })._parent = null
  leaf.onPointerMove(ev(102, 102))
  leaf.onPointerUp(ev(102, 102))
  expect("inert mid-press", logged(), ["M:down(M)", "G:down(M)", "root:down(M)", "root:move(M)", "root:up(M)"])
})

test("a cancel ends the press on its target, never taps, and frees the pointer", () => {
  let { leaf, logged } = world()
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerCancel(ev(100, 100))
  expect("cancel on S", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:cancel(S)", "G:cancel(S)", "root:cancel(S)"])
  // The press is gone: an up on the same pointer is one this layer never
  // saw go down, and the next press taps as a first tap.
  leaf.onPointerUp(ev(300, 100))
  expect("orphan up after cancel", logged(), ["T:up(T)", "root:up(T)"])
  leaf.onPointerDown(ev(100, 100))
  leaf.onPointerUp(ev(100, 100))
  expect("tap after cancel", logged(), ["S:down(S)", "G:down(S)", "root:down(S)", "S:up(S)", "G:up(S)", "root:up(S)", "S:tap(S#1)@100,100", "G:tap(S#1)@100,100", "root:tap(S#1)@100,100"])
})

test("a stopped down keeps the root out of the cancel too", () => {
  let { leaf, logged, sprite } = world()
  sprite("C", 200, 100, 40, 40, null, new Set(["down"]))
  leaf.onPointerDown(ev(200, 100))
  leaf.onPointerCancel(ev(250, 100))
  expect("claimed cancel", logged(), ["C:down(C)", "C:cancel(C)"])
})

test("an up this layer never saw go down delivers to what is under it", () => {
  let { leaf, logged } = world()
  leaf.onPointerUp(ev(300, 100, 7))
  expect("orphan up", logged(), ["T:up(T)", "root:up(T)"])
})
