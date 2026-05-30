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