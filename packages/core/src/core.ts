import { colord, extend } from "colord"
import namesPlugin from "colord/plugins/names"
import type { MeasureTextOptions } from "./types"
extend([namesPlugin])

export function parseColorToU32(color: string): number {
  let { r, g, b, a } = colord(color).toRgb()
  return (((r & 0xFF) << 24) | ((g & 0xFF) << 16) | ((b & 0xFF) << 8) | ((a * 255) & 0xFF)) >>> 0
}

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
    ffi.setTextInputActive(wantActive)
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

// Returns the node's window-relative bounding box from the most recently
// computed layout, or null if the node has no layout or has not been laid out
// yet. This is a snapshot read, not reactive: call it inside onLayout (or an
// event handler) to get values for the current frame. Phase 1 composes only
// translations; x/y are wrong when a rotate/scale sits anywhere above the node.
export function getBoundingBox(node: { id: number }): BoundingBox | null {
  return ffi.getBoundingBox(node.id)
}

export function measureText(text: string, options?: MeasureTextOptions): { width: number, height: number } {
  return ffi.measureText(text, options)
}