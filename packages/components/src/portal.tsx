import { createPortal } from "@solidrt/core"

export interface PortalProps {
  children?: any
  // Where to mount the content. A node, typically captured from another
  // element's `ref`. Defaults to the window root.
  mount?: { id: number }
}

/**
 * Renders its child somewhere other than its lexical position: by default at the
 * window root, so overlays (modals, menus, tooltips) escape the clipping and
 * stacking of their surrounding layout. Thin JSX wrapper over core's
 * createPortal; the child should be a single element positioned absolutely
 * (`position="absolute"`), since it is inserted into the window's flex root.
 */
export function Portal(props: PortalProps) {
  // A JSX element's runtime value is the built node; createPortal relocates it.
  createPortal(props.children, props.mount as any)
  return undefined
}