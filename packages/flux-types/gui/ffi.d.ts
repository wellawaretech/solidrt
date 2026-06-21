// The low-level render-tree bridge (gui-enabled runtime only). INTERNAL: this is
// what `@solidrt/core`'s SolidJS renderer drives. App code should use JSX
// components, not call `ffi` directly. Typed here only for completeness of the
// runtime surface. Helper types stay module-scoped via the trailing `export {}`.

/** Options for {@link ffi.measureText}. */
type MeasureTextOptions = {
  /** "sans", "mono", or a font family name. */
  fontFamily?: string
  /** Font size in pixels. */
  fontSize?: number
  fontStyle?: "normal" | "italic"
  /** Weight 100..900. */
  fontWeight?: number
  /** Cap the measured height at this many lines. */
  maxLines?: number
}

/** Measured text dimensions in pixels. */
type TextSize = {
  width: number
  height: number
}

/** A node's laid-out bounding box in pixels. */
type NodeBoundingBox = {
  x: number
  y: number
  width: number
  height: number
}

declare global {
  /**
   * The low-level render-tree bridge. INTERNAL: `@solidrt/core`'s renderer drives
   * this; app code should use JSX components rather than call `ffi` directly.
   * Available only on a gui-enabled runtime.
   */
  const ffi: {
    /** Create the root window node with the given id. */
    createRoot(id: number): void
    /** Create a node of the given element kind (e.g. "view", "text"). */
    createNode(id: number, kind: string): void
    /** Remove `nodeId` from `parentId`. */
    deleteNode(parentId: number, nodeId: number): void
    /** Insert `nodeId` under `parentId`, before `anchorId` (or at the end if omitted). */
    insertNode(parentId: number, nodeId: number, anchorId?: number): void
    /** Set a single property on a node. Throws on an invalid value for the property. */
    setProperty(nodeId: number, property: string, value: any): void
    /** Request a redraw without mutating the tree. */
    requestFrame(): void
    /** Toggle the platform text-input (on-screen keyboard) active state. */
    setTextInputActive(active: boolean): void
    /** Measure a string under the given text options. */
    measureText(text: string, options?: MeasureTextOptions): TextSize
    /** The laid-out bounding box of a node, or undefined if it has none yet. */
    getBoundingBox(id: number): NodeBoundingBox | undefined
  }
}

export {}