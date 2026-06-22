// The render-tree bridge (gui-enabled runtime only): the low-level surface the
// renderer drives to build and mutate the native tree - create/insert/delete
// nodes, write properties, query layout, measure text. Displaying the built
// tree is the runner's concern ("srt:render" in lattice), not part of this
// module; requestFrame here only schedules a future frame.

declare module "flux:rendertree" {
  /** Font options for {@link measureText}. */
  export interface MeasureTextOptions {
    fontFamily?: "sans" | "mono" | (string & {})
    fontSize?: number
    fontStyle?: "normal" | "italic"
    fontWeight?: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
    maxLines?: number
  }

  /** Create the window root node with the given id. */
  export function createRoot(id: number): void
  /** Create a node of `kind` (the primitive element name) with the given id. */
  export function createNode(id: number, kind: string): void
  /** Insert `nodeId` under `parentId`, before `anchorId` if given (else appended). */
  export function insertNode(parentId: number, nodeId: number, anchorId?: number): void
  /** Detach and destroy `nodeId` from under `parentId`. */
  export function deleteNode(parentId: number, nodeId: number): void
  /** Write a single property on a node; `value` is marshalled per property. */
  export function setProperty(nodeId: number, name: string, value: unknown): void
  /** Enable or disable text-input capture / the on-screen keyboard. */
  export function setTextInputActive(active: boolean): void
  /** Request that a frame be rendered soon (coalesced by the demand-driven loop). */
  export function requestFrame(): void
  /**
   * Lay out, paint and submit the whole tree to the screen now (one frame). The
   * direct draw path for a flux + alloy app; requestFrame only schedules a
   * future frame and leaves the actual draw to the runner.
   */
  export function render(): void
  /**
   * Measure the rendered size of `text` under the given font options, without
   * adding it to the tree.
   */
  export function measureText(text: string, options?: MeasureTextOptions): { width: number, height: number }
  /**
   * The node's window-relative bounding box from the most recent layout, or
   * `null` if it has no layout or has not been laid out yet.
   */
  export function getBoundingBox(id: number): { x: number, y: number, width: number, height: number } | null
}