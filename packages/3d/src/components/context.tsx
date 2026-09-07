import { createContext, createSignal, setFocus, untrack, useContext } from "@solidrt/core"
import type { Element, KeyEvent, PointerEvent, WheelEvent } from "@solidrt/core"
import type { SceneNode } from "../node.ts"
import type { Scene as SceneHandle } from "../scene.ts"
import type { CameraState, CameraUpdate } from "../camera.ts"

/** A camera control's input feed - the handlers shape the camera controls
 * expose, every field optional. A listener with key handlers makes the
 * leaf focusable and focused on pointer down, since keys route through
 * the focused node; onBlur tells it the held keys will get no key-up. */
export type SceneInputListener = {
  onPointerDown?(event: PointerEvent): void
  onPointerMove?(event: PointerEvent): void
  onPointerUp?(event: PointerEvent): void
  onWheel?(event: WheelEvent): void
  onKeyDown?(event: KeyEvent): void
  onKeyUp?(event: KeyEvent): void
  onBlur?(): void
}

/**
 * The channel between the element showing the scene and camera-control
 * components (`<OrbitCamera>`, `<FirstPersonCamera>`): the leaf feeds
 * pointer, wheel and key events in, controls listen. The built-in
 * `<Scene>` leaf is wired automatically; a custom `output` leaf spreads
 * `{...useScene().input.handlersFor(layout)}` beside its scene.handlersFor
 * spread, with the same `layout`, and declares itself `focusable` when a
 * key-driven control is in the scene (the spread's onPointerDown focuses
 * it; keys reach only the focused node).
 */
export type SceneInput = {
  /** Spreadable pointer + wheel + key handlers for the leaf. `layout`
   * reports the leaf's laid-out size - what viewport-relative controls
   * scale to - and is read per event, so a reactive layout just works.
   * `node` is the leaf itself, focused on pointer down while a key
   * listener is registered. */
  handlersFor(layout: () => { width: number; height: number }, node?: () => { id: number } | undefined): {
    onPointerDown(event: PointerEvent): void
    onPointerMove(event: PointerEvent): void
    onPointerUp(event: PointerEvent): void
    onWheel(event: WheelEvent): void
    onKeyDown(event: KeyEvent): void
    onKeyUp(event: KeyEvent): void
    onBlur(): void
  }
  /** Subscribe a control; returns the unsubscribe. */
  add(listener: SceneInputListener): () => void
  /** The showing leaf's laid-out size, null before one registered a
   * layout (handlersFor supplies it). */
  layout(): { width: number; height: number } | null
}

/** The camera state a camera control drives: the scene, or a view (both
 * expose setCamera and camera()). */
export type CameraTarget = { setCamera(update: CameraUpdate): void; camera(): CameraState }

export type SceneCtx = { scene: SceneHandle; parent: SceneNode; camera: CameraTarget; input: SceneInput }
export let SceneContext = createContext<SceneCtx>()

/**
 * The enclosing scene, parent node, camera target and input channel - the
 * imperative escape hatch inside a component subtree (throws outside a
 * `<Scene>`). `camera` and `input` are the nearest OWNER's: the scene's,
 * or inside a `<View3d>` that view's - what the camera-control components
 * drive and listen on, and what a custom `output` leaf spreads from.
 */
export function useScene(): SceneCtx {
  return useContext(SceneContext)
}

// The scene context for a node component's children. Solid's provider is
// a root and two memos over the children, paid even when there are none,
// so it is built only when the JSX has children: the compiler emits the
// `children` getter only then, so the test is structural, not a read
// (probes/3d-instance-mount-bench.tsx has the per-row cost).
export function provide(ctx: SceneCtx, parent: SceneNode, props: { children?: Element }): Element | undefined {
  if (!("children" in props)) return undefined
  return <SceneContext value={{ scene: ctx.scene, parent, camera: ctx.camera, input: ctx.input }}>{props.children}</SceneContext>
}

// The input channel between a leaf and the camera controls under it (the
// SceneInput contract above), one per owner: the scene's and each view's.
// `hasInput` gates the owner's built-in leaf handlers, so a control-less
// leaf costs no pointer routing; `hasKeys` turns it focusable only while a
// key-driven control is registered, so a pointer-only scene never steals
// focus from a text field on a click. ownedWrite because controls add()
// from their component bodies.
export function createSceneInput(): { input: SceneInput; hasInput: () => boolean; hasKeys: () => boolean } {
  let listeners = new Set<SceneInputListener>()
  let [hasInput, setHasInput] = createSignal(false, { ownedWrite: true })
  let [hasKeys, setHasKeys] = createSignal(false, { ownedWrite: true })
  let anyKeys = () => [...listeners].some(l => l.onKeyDown || l.onKeyUp)
  let leafLayout: (() => { width: number; height: number }) | null = null
  let input: SceneInput = {
    handlersFor(layout, node) {
      leafLayout = layout
      return {
        onPointerDown: e => {
          let n = node?.()
          if (n && untrack(hasKeys)) setFocus(n.id)
          listeners.forEach(l => l.onPointerDown?.(e))
        },
        onPointerMove: e => listeners.forEach(l => l.onPointerMove?.(e)),
        onPointerUp: e => listeners.forEach(l => l.onPointerUp?.(e)),
        onWheel: e => listeners.forEach(l => l.onWheel?.(e)),
        onKeyDown: e => listeners.forEach(l => l.onKeyDown?.(e)),
        onKeyUp: e => listeners.forEach(l => l.onKeyUp?.(e)),
        onBlur: () => listeners.forEach(l => l.onBlur?.()),
      }
    },
    add(listener) {
      listeners.add(listener)
      setHasInput(true)
      setHasKeys(anyKeys())
      return () => {
        listeners.delete(listener)
        setHasInput(listeners.size > 0)
        setHasKeys(anyKeys())
      }
    },
    layout: () => leafLayout?.() ?? null,
  }
  return { input, hasInput, hasKeys }
}
