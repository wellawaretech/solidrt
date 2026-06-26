import { createRoot, onCleanup } from "@solidjs/signals"
import { createRenderer } from "@solidjs/universal"
import * as tree from "flux:rendertree"
import { attachWindow } from "./window"
import { setEventHandler, cleanupNodeHandlers, getFocusedNodeId, setFocus } from "./core"
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

// Detaches `node` from `parent` and destroys it (and all descendants) on both
// the JS and native sides. Hoisted out of the renderer config so createPortal
// can reuse it (createRenderer does not return its removeNode hook).
function removeNode(parent: ProxyNode, node: ProxyNode): void {
  if (!node || !parent) return

  // console.debug("[srt] removeNode", parent.id, node.id)

  // Update JS tree references
  let index = parent.children.indexOf(node)
  if (index !== -1) {
    parent.children.splice(index, 1)
  }
  node.parent = undefined

  tree.deleteNode(parent.id, node.id)

  // Recursively clean up node and all descendants. Clear focus before
  // dropping handlers so onBlur still fires for a focused descendant.
  let cleanup = (n: ProxyNode) => {
    for (let child of n.children) cleanup(child)
    if (n.id === getFocusedNodeId()) setFocus(null)
    nodes.delete(n.id)
    cleanupNodeHandlers(n.id)
  }
  cleanup(node)
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
  createElement: (elementType: string): ProxyNode => {
    let proxy = createProxyNode(elementType)

    // console.debug("[srt] createElement", proxy.id, elementType)

    if (elementType === "window") tree.createRoot(proxy.id)
    else tree.createNode(proxy.id, elementType)

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
    if (!node) return

    // console.debug("[srt] setProperty", node.id, name, value)

    if (/^on[A-Z]/.test(name) && (value == null || typeof value === "function")) {
      setEventHandler(node.id, name, value as Function | null | undefined)
      return
    }

    if (name === "color" && isGradient(value)) {
      tree.setProperty(node.id, name, value)
      return
    }

    if (name === "color" && typeof value === "string") {
      tree.setProperty(node.id, name, parseColor(value))
      return
    }

    tree.setProperty(node.id, name, value)
  },

  insertNode: (parent: ProxyNode, node: ProxyNode, anchor?: ProxyNode): void => {
    if (!node) return

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
 */
export function createPortal(node: ProxyNode, mount?: ProxyNode): void {
  let target = mount ?? windowRoot
  if (!target) {
    throw new Error("createPortal: no mount target (called before render()?)")
  }
  insertNode(target, node)
  onCleanup(() => removeNode(target, node))
}
