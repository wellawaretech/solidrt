import { getEventHandler } from "./events"

// Currently-focused node id. Reset to null automatically across engine
// reloads because the JS environment is rebuilt from scratch.
let focusedNodeId: number | null = null

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
}

export function getFocusedNodeId(): number | null {
  return focusedNodeId
}