import { createRoot, onCleanup, NotReadyError } from "@solidjs/signals"
import { createRenderer } from "@solidjs/universal"
import { createErrorBoundary } from "solid-js"
import type { Element } from "solid-js"
import * as tree from "flux:rendertree"
import { attachWindow, setWindowRoot } from "./window"
import { setEventHandler, setFocusable, setTextInputHints, cleanupNode, focusedNode, setFocus } from "./core"

export { getEventHandler } from "./core"

export let nodes = new Map()

// export { registerPropHandler, registerNodeCleanup } from "./hooks"

type ElementType = string

// ProxyNode: Lightweight proxy for Rust-side nodes
// Caches parent/child references to avoid FFI calls for tree queries
interface ProxyNode {
  readonly id: number
  readonly elementType: ElementType
  parent?: ProxyNode
  children: ProxyNode[]
}

let id = 1
function createProxyNode(elementType: ElementType): ProxyNode {
  let node = { id, elementType, children: [] }
  nodes.set(id, node)
  id += 1
  return node
}

// Ancestor chain for event dispatch: node ids from `id` (inclusive) to the
// root, following the mount tree (a portaled node reports its mount point's
// chain, not its lexical one). Empty when the id is unknown.
export function getNodePath(id: number): number[] {
  let path: number[] = []
  let node: ProxyNode | undefined = nodes.get(id)
  for (; node; node = node.parent) path.push(node.id)
  return path
}

// Nodes detached this tick and awaiting the destroy sweep, keyed by id so a
// re-insert can cancel one. See removeNode / flushDestroy.
let pendingDestroy = new Map<number, ProxyNode>()
let destroyScheduled = false

// Frees a detached node and its subtree on both sides. Descendants that were
// moved out (their parent no longer points here) are left alone. Clear focus
// before dropping handlers so onBlur still fires for a focused descendant.
function destroyNode(node: ProxyNode): void {
  tree.destroyNode(node.id)
  let cleanup = (n: ProxyNode) => {
    for (let child of n.children) if (child.parent === n) cleanup(child)
    if (n.id === focusedNode()) setFocus(null)
    nodes.delete(n.id)
    cleanupNode(n.id)
  }
  cleanup(node)
}

// End-of-tick sweep: destroy every still-detached pending node. A node that was
// re-inserted this tick had its entry removed by insertNode (it moved, not
// died); one still parentless here is genuinely gone.
function flushDestroy(): void {
  destroyScheduled = false
  let batch = pendingDestroy
  pendingDestroy = new Map()
  for (let node of batch.values()) {
    if (node.parent === undefined) destroyNode(node)
  }
}

// Detaches `node` from `parent`, keeping the subtree alive so it can be
// re-inserted elsewhere (a move) - matching DOM removeChild. Destruction is
// deferred to an end-of-tick sweep; if nothing re-attaches the node by then it
// is freed. Hoisted out of the renderer config so createPortal can reuse it
// (createRenderer does not return its removeNode hook).
function removeNode(parent: ProxyNode, node: ProxyNode): void {
  if (!node || !parent) return

  // console.debug("[srt] removeNode", parent.id, node.id)

  // Update JS tree references
  let index = parent.children.indexOf(node)
  if (index !== -1) {
    parent.children.splice(index, 1)
  }
  node.parent = undefined

  tree.detachNode(parent.id, node.id)

  pendingDestroy.set(node.id, node)
  if (!destroyScheduled) {
    destroyScheduled = true
    Promise.resolve().then(flushDestroy)
  }
}

// ------ Leak sentinel (dev only) --------

// A node created but never inserted is unreachable by the remove -> destroy
// sweep, so it leaks permanently, natively and in the maps here. The usual
// cause is an element-valued prop read more than once: every read builds a
// fresh subtree and only the mounted one is ever freed. Rather than
// bookkeeping on the hot create/insert paths, orphans are derived from the
// proxy map itself: parentless, not the window root, and not awaiting the
// destroy sweep. window.ts runs the scan on a rendered frame every few
// seconds; dev bundles only (srt always defines import.meta.env.DEV, so a
// production bundle folds the check into a constant early return).
const SENTINEL_INTERVAL_MS = 5000
let sentinelDue = 0
let warnedLeakTypes = new Set<string>()

export function scanForOrphans(now: number): void {
  if (!import.meta.env.DEV) return
  if (now < sentinelDue) return
  sentinelDue = now + SENTINEL_INTERVAL_MS
  let counts = new Map<string, number>()
  let total = 0
  for (let node of nodes.values()) {
    if (node.parent !== undefined || node.elementType === "window" || pendingDestroy.has(node.id)) continue
    total += 1
    counts.set(node.elementType, (counts.get(node.elementType) ?? 0) + 1)
  }
  if (total === 0) return
  // Warn only when a new element type joins the orphans, but list every type
  // with its count so the breakdown always adds up to the total.
  let fresh = [...counts].filter(([type]) => !warnedLeakTypes.has(type))
  if (fresh.length === 0) return
  for (let [type] of fresh) warnedLeakTypes.add(type)
  let list = [...counts].map(([type, n]) => `<${type}> x${n}`).join(", ")
  console.warn(
    `Leak sentinel: ${total} nodes are unreachable and will never be freed: ${list}. ` +
      `The usual cause is reading an element-valued prop more than once (every read ` +
      `builds a new subtree); read it once where it mounts, or resolve it with ` +
      `children(). If these nodes are intentionally kept for later mounting, ignore ` +
      `this. The next warning comes when a new element type joins the list.`,
  )
}

// A property the native tree rejected must not take down the reactive system:
// a typo'd, not-yet-implemented, or detached-only prop poisons only itself.
// Warn once per element kind + property with a stack (the dev server remaps
// its frames to the .tsx source), then ignore further writes of the same pair.
let warnedRejectedProps = new Set<string>()

function setTreeProperty(node: ProxyNode, name: string, value: unknown): void {
  try {
    tree.setProperty(node.id, name, value)
  } catch (e) {
    // Name-level rejections (the exact prefixes apply_jsx in flux emits) are
    // warn-and-continue so a stale prop does not kill the app; a bad VALUE for
    // a known property rethrows, per the throw-in-dev validation policy.
    let message = String(e)
    if (!message.includes("Unknown property") && !message.includes("Detached-only")) throw e
    let key = node.elementType + "." + name
    if (warnedRejectedProps.has(key)) return
    warnedRejectedProps.add(key)
    let stack = new Error().stack ?? ""
    console.warn(`Ignoring property '${name}' on <${node.elementType}>: ${message}\n${stack}`)
  }
}

// Applies a single prop to a node: routes events to the handler registry,
// parses color strings/gradients, and forwards everything else to the tree.
// Shared by the renderer's setProperty hook and by createElement, which since
// the dom-expressions "universal" template passes static props inline as a
// second argument rather than as separate setProp calls.
// A property's route is a function of its name alone, so the classifying
// regex and compares run once per unique name; per write it is one Map get.
const ROUTE_TREE = 0
const ROUTE_EVENT = 1
const ROUTE_FOCUSABLE = 2
const ROUTE_HINTS = 3
let propRoutes = new Map<string, number>()

function routeFor(name: string): number {
  let route = propRoutes.get(name)
  if (route === undefined) {
    route = /^on[A-Z]/.test(name)
      ? ROUTE_EVENT
      : name === "focusable"
        ? ROUTE_FOCUSABLE
        : name === "textInputHints"
          ? ROUTE_HINTS
          : ROUTE_TREE
    propRoutes.set(name, route)
  }
  return route
}

function applyProp<T>(node: ProxyNode, name: string, value: T): void {
  if (!node) return

  // console.debug("[srt] applyProp", node.id, name, value)

  switch (routeFor(name)) {
    case ROUTE_EVENT:
      // A non-function, non-null value on an on* name is not a handler;
      // fall through to the tree so the native side rejects it.
      if (value == null || typeof value === "function") {
        setEventHandler(node.id, name, value as Function | null | undefined)
        return
      }
      break
    case ROUTE_FOCUSABLE:
      setFocusable(node.id, value === true)
      return
    case ROUTE_HINTS:
      setTextInputHints(node.id, value as any)
      return
  }

  setTreeProperty(node, name, value)
}

let renderer = createRenderer<ProxyNode>({
  createElement: (elementType: string, props?: Record<string, any>): ProxyNode => {
    let proxy = createProxyNode(elementType)

    // console.debug("[srt] createElement", proxy.id, elementType)

    if (elementType === "window") tree.createRoot(proxy.id)
    else tree.createNode(proxy.id, elementType)

    // The universal JSX template hands static props here as an object. The
    // compiler routes children/ref expressions through their own hooks, so
    // those names only appear here in degenerate literal forms (children="hi",
    // a bare ref) - let them flow to the tree so the unknown-property warning
    // reports them instead of dropping them silently.
    if (props) {
      for (let name in props) {
        applyProp(proxy, name, props[name])
      }
    }

    return proxy
  },

  // A string child of <text> or <span>: a run of its parent's content, with
  // no element form of its own (the DOM's "#text" node name).
  createTextNode: (value: string): ProxyNode => {
    let proxy = createProxyNode("#text")
    // console.debug("[srt] createTextNode", proxy.id, value)
    tree.createNode(proxy.id, "#text")
    tree.setProperty(proxy.id, "text", "" + value)
    return proxy
  },

  replaceText: (node: ProxyNode, value: string): void => {
    // console.debug("[srt] replaceText", node.id, value)
    tree.setProperty(node.id, "text", "" + value)
  },

  isTextNode: (node: ProxyNode): boolean => node?.elementType === "#text",
  setProperty: <T>(node: ProxyNode, name: string, value: T): void => {
    // console.debug("[srt] setProperty", node.id, name, value)
    applyProp(node, name, value)
  },

  insertNode: (parent: ProxyNode, node: ProxyNode, anchor?: ProxyNode): void => {
    if (!node) return

    // A value without an id is not a node: a signal accessor (<For>, <Repeat>,
    // a memo) that reached the renderer unresolved. Without this check it
    // surfaces as an FFI type error on node.id, which names nothing. Known
    // cause: okf/upstream/signals-flatten-array-clobbers-needs-unwrap.md.
    if (typeof node !== "object" || node.id === undefined) {
      let what = typeof node === "function" ? "a signal accessor" : `a ${typeof node}`
      throw new Error(
        `insertNode received ${what} instead of an element under <${parent?.elementType ?? "?"}>; ` +
          `resolve the children with children() or return one root element from the component.`,
      )
    }

    // A re-inserted node is being moved, not destroyed: cancel its pending
    // destroy so the end-of-tick sweep leaves it (and its subtree) alone.
    pendingDestroy.delete(node.id)

    if (parent) {
      // console.debug("[srt] insertNode", parent.id, node.id, anchor?.id ?? "")

      // Native first: the tree refuses a laid-out element under a d-* parent
      // (it throws, naming both tags), and the mirror must not record a child
      // the tree does not have.
      if (anchor) tree.insertNode(parent.id, node.id, anchor.id)
      else tree.insertNode(parent.id, node.id)

      node.parent = parent

      if (!anchor) {
        parent.children.push(node)
      } else {
        let index = parent.children.indexOf(anchor)
        if (index === -1) {
          parent.children.push(node)
        } else {
          parent.children.splice(index, 0, node)
        }
      }
    }
  },

  removeNode,

  getParentNode: (node: ProxyNode) => node?.parent,
  getFirstChild: (node: ProxyNode) => node?.children[0],
  getNextSibling: (node: ProxyNode) => {
    let parent = node?.parent
    if (!parent) return undefined
    let index = parent.children.indexOf(node)
    if (index === -1) return undefined
    return parent.children[index + 1]
  },
})

export let { memo, createComponent, createElement, createTextNode, insertNode, spread, setProp, mergeProps, applyRef, ref } =
  renderer
let { effect: rawEffect, insert: rawInsert } = renderer

// ------ Per-node error containment --------
//
// Every reactive write into the tree goes through two exports the compiled
// JSX calls: `effect` (an element's dynamic props) and `insert` (a child
// expression). An error thrown while computing either is contained right
// there: the props or children keep their last good value, the effect stays
// subscribed (the throwing read is tracked, so fixing it recomputes and the
// node recovers on its own), and the rest of the app keeps running. Without
// this one unclaimed error halts the whole reactive system. A NotReadyError
// is not an error but a pending async read on its way to the nearest
// <Loading>, so it passes through. Reported once per site until it recovers,
// not once per run. What escapes these two (a user createEffect that throws)
// still reaches render()'s root boundary.
const SKIP = Symbol("skip")

function guard<T>(fn: (prev?: T) => T, describe: () => string, nested: boolean, empty: T): (prev?: T) => T {
  let last = empty
  let failing = false
  return (prev?: T) => {
    try {
      let value = fn(prev === SKIP ? undefined : prev)
      if (failing) {
        failing = false
        console.warn(`Recovered: ${describe()} computes again`)
      }
      // A child expression resolving to a function is read by an inner
      // effect (universal's insert); that read gets the same containment.
      // Solid's flatten only unwraps zero-arity functions and inserts any
      // other function as a node, so only accessors are wrapped, and the
      // wrapper keeps arity 0.
      if (nested && typeof value === "function" && value.length === 0) {
        let inner = guard(value as any, describe, true, empty)
        value = (() => inner()) as any
      }
      last = value
      return value
    } catch (e) {
      if (e instanceof NotReadyError) throw e
      if (!failing) {
        failing = true
        console.error(`Contained error: ${describe()} threw and keeps its last value until it computes again.`, e)
      }
      return last
    }
  }
}

// Universal's declarations trail its runtime (effect takes options, insert
// takes initial and options), so the wrappers carry the runtime signatures.
type EffectFn = <T>(fn: (prev?: T) => T, effectFn?: (value: T, prev?: T) => void, options?: unknown) => void
type InsertFn = (parent: ProxyNode, accessor: unknown, marker?: unknown, initial?: unknown, options?: unknown) => ProxyNode
let effectRaw = rawEffect as unknown as EffectFn
let insertRaw = rawInsert as unknown as InsertFn

export let effect: EffectFn = (fn, effectFn, options) =>
  effectRaw<any>(
    guard<any>(fn, () => "an element's prop expression", false, SKIP),
    effectFn && ((value, prev) => (value === SKIP ? undefined : effectFn(value, prev === SKIP ? undefined : prev))),
    options,
  )

export let insert: InsertFn = (parent, accessor, marker, initial, options) =>
  insertRaw(
    parent,
    typeof accessor === "function"
      ? guard(accessor as any, () => `a child expression of <${parent.elementType}> ${getNodePath(parent.id).join("/")}`, true, undefined)
      : accessor,
    marker,
    initial,
    options,
  )

// The current <window> node: the app's, or the error window standing in for
// it. Serves as the default mount target for createPortal (single window by
// design, so one ambient ref).
let windowRoot: ProxyNode | undefined
let rendered = false
// Ids of error windows built by the root boundary, alive only while shown.
let errorWindows = new Set<number>()

/**
 * Mounts a SolidRT app. Call once at the top level: `render(() => <App />)`.
 * The element returned by `code` MUST be a `<window>` (it becomes the native
 * window and root of the render tree); anything else throws. Runs inside a
 * reactive root, so the whole tree is disposed together on engine reload.
 *
 * The whole app, window included, sits inside an error boundary: an error no
 * <Errored> claims replaces the app's window with an error window (message,
 * stack, a reset button) instead of halting the reactive system for good.
 * The app's subtree stays alive behind it - the boundary keeps it and marks
 * only the failed computations - so reset recomputes those in place and the
 * same window node comes back.
 */
export function render(code: () => any) {
  // Once per app: there is no unmount; teardown is engine teardown.
  if (rendered) {
    throw new Error("render() already called; an app has exactly one render()")
  }
  rendered = true
  createRoot(() => {
    let root = createErrorBoundary(
      () => {
        let win = code()
        if (!win || win.elementType !== "window") {
          throw new Error("render() root must be a <window> element")
        }
        return win
      },
      (error, reset) => {
        // The boundary hands the error as an accessor.
        let err = error()
        console.error("Uncaught error: the app is replaced by the error window until reset or reload.", err)
        let win = errorWindow(err, reset)
        errorWindows.add(win.id)
        return win
      },
    )
    rawEffect(
      () => root() as ProxyNode,
      (win, prev) => swapRoot(win, prev),
    )
  })
}

// The boundary's value changed: the app's window on mount and after a
// successful reset, an error window after an error. Creating a window already
// made it the native root; setRoot is the way back to an existing one.
function swapRoot(win: ProxyNode, prev?: ProxyNode) {
  windowRoot = win
  if (prev === undefined) {
    attachWindow(win.id)
    return
  }
  // Creating the error window made it the native root; the app's window
  // coming back is the case that needs the explicit way back.
  if (!errorWindows.has(win.id)) tree.setRoot(win.id)
  setWindowRoot(win.id)
  // Keys route to the focused node; one inside the hidden window must not
  // keep hearing them.
  setFocus(null)
  // The app's window survives behind an error window (the boundary keeps its
  // subtree for reset); an error window replaced by anything is dead.
  if (errorWindows.has(prev.id) || !errorWindows.has(win.id)) {
    errorWindows.delete(prev.id)
    destroyNode(prev)
  }
}

// The error window: the in-app sibling of the runtime's startup BSOD, built
// from the primitives directly (no JSX in core). Static content; the reset
// button recomputes the failed sources, and a reload replaces everything.
function errorWindow(err: unknown, reset: () => void): ProxyNode {
  let message = err instanceof Error ? err.message : String(err)
  let stack = err instanceof Error && err.stack ? err.stack : ""
  let text = (content: string, props: Record<string, any>) => {
    let node = createElement("text", props)
    insertNode(node, createTextNode(content))
    return node
  }
  let win = createElement("window", { title: "Application error" })
  insertNode(win, createElement("d-rect", { color: "#1144bb" }))
  let column = createElement("view", { flexGrow: 1, flexDirection: "column", padding: 40, gap: 12 })
  insertNode(column, text(":(", { color: "white", fontSize: 64, fontWeight: 700 }))
  insertNode(column, text("Something went wrong", { color: "white", fontSize: 22 }))
  insertNode(column, text(message, { color: "white", fontSize: 16 }))
  if (stack) insertNode(column, text(stack, { color: "#aac2ff", fontSize: 12, fontFamily: "mono" }))
  insertNode(column, text("Fix the error and save to reload, or reset to retry the failed computations.", { color: "#aac2ff", fontSize: 14 }))
  let button = createElement("view", { alignSelf: "flex-start", padding: 12, onPointerDown: () => reset() })
  insertNode(button, createElement("d-rect", { color: "white", radius: 6 }))
  insertNode(button, text("Reset", { color: "#1144bb", fontSize: 16, fontWeight: 600 }))
  insertNode(column, button)
  insertNode(win, column)
  return win
}

/**
 * Relocates an already-built node out of its lexical position to `mount` (the
 * window root by default), then removes it again when the surrounding reactive
 * scope disposes. The low-level portal primitive: it moves a single node and
 * nothing more. Conveniences (an overlay layer, centering, a backdrop) belong
 * in higher packages built on top of it.
 *
 * `node` is a concrete node, not an accessor: its own children (including any
 * reactive content) are already wired by the JSX that built it and keep working
 * wherever it is mounted. We only move the root.
 *
 * The default mount is the window's flex root, so a portaled node that is not
 * `position: "absolute"` will take flow space and displace app content. Position
 * the portal root absolutely, or pass a `mount` target that does it for you.
 *
 * Returns null (nothing in place), so a component may return a portal directly.
 *
 * Portals cannot mount during the initial render: the default target is the
 * window root, which exists only after the app's first build returns, so a
 * portal created during that build throws. This is the contract, not a bug:
 * portal content is overlay content, opened by a signal that starts false.
 */
export function createPortal(node: Element, mount?: ProxyNode): null {
  let target = mount ?? windowRoot
  if (!target) {
    throw new Error("createPortal: no mount target (portals cannot mount during the initial render; open them after mount)")
  }
  if (node === null || typeof node !== "object" || Array.isArray(node)) {
    throw new Error("createPortal: node must be a single built element")
  }
  insertNode(target, node as ProxyNode)
  // A destroyed mount target has already swept the portaled node with it
  // (destroy walks the mount tree), so detaching then would hand freed ids to
  // the native side, which panics. Gone from the proxy map = already freed.
  onCleanup(() => {
    if (nodes.has((node as ProxyNode).id)) removeNode(target, node as ProxyNode)
  })
  return null
}
