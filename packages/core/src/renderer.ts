import { createRoot, onCleanup } from "@solidjs/signals"
import { createRenderer } from "@solidjs/universal"
import type { Element } from "solid-js"
import * as tree from "flux:rendertree"
import { attachWindow } from "./window"
import { setEventHandler, setFocusable, setTextInputHints, cleanupNode, focusedNode, setFocus } from "./core"
import { parseColor, isGradient } from "./color"

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
  let fresh = [...counts].filter(([type]) => !warnedLeakTypes.has(type))
  if (fresh.length === 0) return
  for (let [type] of fresh) warnedLeakTypes.add(type)
  let list = fresh.map(([type, n]) => `<${type}> x${n}`).join(", ")
  console.warn(
    `Leak sentinel: ${total} nodes are unreachable and will never be freed: ${list}. ` +
      `The usual cause is reading an element-valued prop more than once (every read ` +
      `builds a new subtree); read it once where it mounts, or resolve it with ` +
      `children(). If these nodes are intentionally kept for later mounting, ignore ` +
      `this. Element types already reported are not reported again.`,
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
    let message = String(e)
    if (!message.includes("unknown property") && !message.includes("detached-only")) throw e
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
function applyProp<T>(node: ProxyNode, name: string, value: T): void {
  if (!node) return

  // console.debug("[srt] applyProp", node.id, name, value)

  if (/^on[A-Z]/.test(name) && (value == null || typeof value === "function")) {
    setEventHandler(node.id, name, value as Function | null | undefined)
    return
  }

  if (name === "focusable") {
    setFocusable(node.id, value === true)
    return
  }

  if (name === "textInputHints") {
    setTextInputHints(node.id, value as any)
    return
  }

  if (name === "color" && isGradient(value)) {
    setTreeProperty(node, name, value)
    return
  }

  if (name === "color" && typeof value === "string") {
    setTreeProperty(node, name, parseColor(value))
    return
  }

  setTreeProperty(node, name, value)
}

export let {
  effect,
  memo,
  createComponent,
  createElement,
  createTextNode,
  insertNode,
  insert,
  spread,
  setProp,
  mergeProps,
  applyRef,
  ref,
} = createRenderer<ProxyNode>({
  createElement: (elementType: string, props?: Record<string, any>): ProxyNode => {
    let proxy = createProxyNode(elementType)

    // console.debug("[srt] createElement", proxy.id, elementType)

    if (elementType === "window") tree.createRoot(proxy.id)
    else tree.createNode(proxy.id, elementType)

    // The universal JSX template hands static props here as an object; children
    // and ref arrive through their own hooks, so skip them.
    if (props) {
      for (let name in props) {
        if (name === "children" || name === "ref") continue
        applyProp(proxy, name, props[name])
      }
    }

    return proxy
  },

  createTextNode: (value: string): ProxyNode => {
    let proxy = createProxyNode("d-span")
    // console.debug("[srt] createTextNode", proxy.id, value)
    tree.createNode(proxy.id, "d-span")
    tree.setProperty(proxy.id, "text", "" + value)
    return proxy
  },

  replaceText: (node: ProxyNode, value: string): void => {
    // console.debug("[srt] replaceText", node.id, value)
    tree.setProperty(node.id, "text", "" + value)
  },

  isTextNode: (node: ProxyNode): boolean => node?.elementType === "d-span",
  setProperty: <T>(node: ProxyNode, name: string, value: T): void => {
    // console.debug("[srt] setProperty", node.id, name, value)
    applyProp(node, name, value)
  },

  insertNode: (parent: ProxyNode, node: ProxyNode, anchor?: ProxyNode): void => {
    if (!node) return

    // A re-inserted node is being moved, not destroyed: cancel its pending
    // destroy so the end-of-tick sweep leaves it (and its subtree) alone.
    pendingDestroy.delete(node.id)

    if (parent) {
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

      // console.debug("[srt] insertNode", parent.id, node.id, anchor?.id ?? "")

      if (anchor) tree.insertNode(parent.id, node.id, anchor.id)
      else tree.insertNode(parent.id, node.id)
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

// The app's single <window> node, set by render(). Serves as the default mount
// target for createPortal (single window by design, so one ambient ref).
let windowRoot: ProxyNode | undefined

/**
 * Mounts a SolidRT app. Call once at the top level: `render(() => <App />)`.
 * The element returned by `code` MUST be a `<window>` (it becomes the native
 * window and root of the render tree); anything else throws. Runs inside a
 * reactive root, so the whole tree is disposed together on engine reload.
 */
export function render(code: () => any) {
  createRoot(() => {
    let root = code()
    if (!root || root.elementType !== "window") {
      throw new Error("render() root must be a <window> element")
    }
    windowRoot = root
    attachWindow(root.id)
    insert(null, root)
  })
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
  onCleanup(() => removeNode(target, node as ProxyNode))
  return null
}
