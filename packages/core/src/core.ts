import * as tree from "flux:rendertree"

let handlers = new Map<number, Map<string, Function>>()

export function setEventHandler(nodeId: number, name: string, fn: Function | null | undefined): void {
  if (fn == null) {
    handlers.get(nodeId)?.delete(name)
    return
  }
  let nodeHandlers = handlers.get(nodeId)
  if (!nodeHandlers) {
    nodeHandlers = new Map()
    handlers.set(nodeId, nodeHandlers)
  }
  nodeHandlers.set(name, fn)
}

export function getEventHandler(nodeId: number, name: string): Function | undefined {
  return handlers.get(nodeId)?.get(name)
}

export function cleanupNodeHandlers(nodeId: number): void {
  handlers.delete(nodeId)
}

// Currently-focused node id. Reset to null automatically across engine
// reloads because the JS environment is rebuilt from scratch.
let focusedNodeId: number | null = null
let textInputActive = false

/**
 * Moves keyboard focus to `nodeId`, or clears it with `null`. Fires `onBlur` on
 * the previously focused node and `onFocus` on the new one. As a side effect,
 * the on-screen keyboard is activated when the newly focused node has an
 * `onTextInput` handler and deactivated otherwise. No-op if the node is already
 * focused.
 */
export function setFocus(nodeId: number | null): void {
  if (nodeId === focusedNodeId) return
  let oldId = focusedNodeId
  focusedNodeId = nodeId
  if (oldId != null) {
    getEventHandler(oldId, "onBlur")?.()
  }
  if (nodeId != null) {
    getEventHandler(nodeId, "onFocus")?.()
  }
  let wantActive = nodeId != null && getEventHandler(nodeId, "onTextInput") != null
  if (wantActive !== textInputActive) {
    textInputActive = wantActive
    tree.setTextInputActive(wantActive)
  }
}

export function getFocusedNodeId(): number | null {
  return focusedNodeId
}

export interface BoundingBox {
  x: number
  y: number
  width: number
  height: number
}

/**
 * Returns the node's window-relative bounding box from the most recently
 * computed layout, or `null` if the node has no layout or has not been laid out
 * yet. This is a snapshot read, not reactive: call it inside `onLayout` (or an
 * event handler) to get values for the current frame. Phase 1 composes only
 * translations; x/y are wrong when a rotate/scale sits anywhere above the node.
 */
export function getBoundingBox(node: { id: number }): BoundingBox | null {
  return tree.getBoundingBox(node.id)
}

/**
 * Measures the rendered size of `text` in layout pixels under the given font
 * options (family, size, weight, style, maxLines), without adding it to the
 * tree. Useful for sizing or laying out around text before it is drawn.
 */
export function measureText(text: string, options?: tree.MeasureTextOptions): { width: number, height: number } {
  return tree.measureText(text, options)
}