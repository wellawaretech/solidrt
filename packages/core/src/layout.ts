export interface BoundingBox {
  x: number
  y: number
  width: number
  height: number
}

// Returns the node's window-relative bounding box from the last computed layout,
// or null if the node has no layout or hasn't been laid out yet.
export function getBoundingBox(node: { id: number }): BoundingBox | null {
  return ffi.getBoundingBox(node.id)
}