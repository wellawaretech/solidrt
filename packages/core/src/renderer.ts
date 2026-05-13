import { createRoot, createEffect } from "@solidjs/signals"
import { createRenderer } from "@solidjs/universal"
import { attachWindow } from "./window"

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

    console.log("createElement", proxy.id, elementType)

    if (elementType === "window") ffi.createRoot(proxy.id)
    else ffi.createNode(proxy.id, elementType)
  
    return proxy
  },

  createTextNode: (value: string): ProxyNode => {
    let proxy = createProxyNode("span")
    console.log("createTextNode", proxy.id, value)
    ffi.createNode(proxy.id, "span")
    ffi.setProperty(proxy.id, "text", ""+value)
    return proxy
  },

  replaceText: (node: ProxyNode, value: string): void => {
    console.log("replaceText", node.id, value)
    ffi.setProperty(node.id, "text", ""+value)
  },

  isTextNode: (node: ProxyNode): boolean => node?.elementType === "span",
  setProperty: <T>(node: ProxyNode, name: string, value: T, prev?: T): void => {
    if (!node) return

    console.log("setProperty", node.id, name, value)
    // if (runPropHandlers(node.id, name, value, prev)) return

    ffi.setProperty(node.id, name, value)
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

      if (anchor) ffi.insertNode(parent.id, node.id, anchor.id)
      else ffi.insertNode(parent.id, node.id)
    }
  },

  removeNode: (parent: ProxyNode, node: ProxyNode): void => {
    if (!node || !parent) return

    // console.log(`remove node ${parent.id}::${node.id}`)

    // Update JS tree references
    let index = parent.children.indexOf(node)
    if (index !== -1) {
      parent.children.splice(index, 1)
    }
    node.parent = undefined

    ffi.deleteNode(parent.id, node.id)

    // Recursively clean up node and all descendants
    // let cleanup = (n: ProxyNode) => {
    //   for (let child of n.children) cleanup(child)
    //   nodes.delete(n.id)
    //   runNodeCleanup(n.id)
    // }
    // cleanup(node)
  },

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

export function render(code: () => any) {
  createRoot(() => {
    let root = code()
    if (!root || root.elementType !== "window") {
      throw new Error("render() root must be a <window> element")
    }
    attachWindow(root.id)
    insert(null, root)
  })
}
