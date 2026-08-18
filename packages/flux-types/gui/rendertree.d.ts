// The render-tree bridge (gui-enabled runtime only): the low-level surface the
// renderer drives to build and mutate the native tree - create/insert/delete
// nodes, write properties, query layout, measure text. Displaying the built
// tree is the runner's concern ("srt:render" in lattice), not part of this
// module; requestFrame here only schedules a future frame.

declare module "flux:rendertree" {
  /** Font options for {@link measureText} and {@link prepareText}. */
  export interface MeasureTextOptions {
    fontFamily?: "sans" | "serif" | "mono" | (string & {})
    fontSize?: number
    fontStyle?: "normal" | "italic"
    fontWeight?: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
    lineHeight?: number
    /** measureText only. */
    maxLines?: number
    /** prepareText only: also report each unit's {@link TextUnit.carets}. */
    carets?: boolean
    /**
     * prepareText only: styled ranges over the text, in JS string offsets,
     * sorted and disjoint (text between them is in the base font). Each
     * overrides the font options it names. A wrap unit crossing a range
     * boundary comes back as one {@link TextUnit} per range, the pieces
     * after the first `glue`d to it. Throws on an invalid range.
     */
    runs?: TextRunRange[]
  }

  /** One styled range for {@link MeasureTextOptions.runs}. */
  export interface TextRunRange {
    start: number
    end: number
    fontFamily?: "sans" | "serif" | "mono" | (string & {})
    fontSize?: number
    fontStyle?: "normal" | "italic"
    fontWeight?: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
    lineHeight?: number
  }

  /**
   * One wrap unit (a word plus its trailing whitespace, or an empty unit at
   * a blank line) of a {@link prepareText} result: everything the engine
   * knows about it, for app-side line breaking.
   */
  export interface TextUnit {
    /** The unit's text without its break characters. */
    text: string
    /** Offsets into the prepared text (JS string indexing), break characters included: the ranges tile the text. */
    start: number
    end: number
    /** Horizontal extent including trailing whitespace: what the next unit's pen position advances by. */
    advance: number
    /** Ink extent without trailing whitespace: what the unit needs at the end of a line. */
    width: number
    ascent: number
    descent: number
    /** The unit ends at a hard line break (newline). */
    hardBreak: boolean
    /** A continuation piece of the previous unit (it crossed a `runs` boundary): a line never breaks before it. */
    glue: boolean
    /** Index into `runs` of the range this piece was shaped in; absent for the base font. */
    run?: number
    /**
     * With `carets`: the caret positions inside the unit, one per grapheme
     * cluster boundary from its start (`offset` = start, x 0) to the end of
     * its shaped text (before any break characters), in order. `offset` is
     * into the prepared text, `x` from the unit's pen position.
     */
    carets?: { offset: number, x: number }[]
  }

  /** The wrap units of a text in one font, shaped once. Plain data; layout is arithmetic over `units`. */
  export interface PreparedText {
    text: string
    units: TextUnit[]
  }

  /** Create the window root node with the given id. */
  export function createRoot(id: number): void
  /** Create a node of `kind` (the primitive element name) with the given id. Throws an `Error` for a name that is not an element. */
  export function createNode(id: number, kind: string): void
  /** Insert `nodeId` under `parentId`, before `anchorId` if given (else appended). */
  export function insertNode(parentId: number, nodeId: number, anchorId?: number): void
  /**
   * Unlink `nodeId` from `parentId` but keep its subtree alive, so it can be
   * re-inserted elsewhere (a move). Mirrors DOM removeChild. Pair with
   * {@link destroyNode} once the node is confirmed dead.
   */
  export function detachNode(parentId: number, nodeId: number): void
  /** Free `nodeId` and its whole subtree. Call after {@link detachNode}. */
  export function destroyNode(nodeId: number): void
  /**
   * Write a single property on a node; `value` is marshalled per property.
   * Throws an `Error` for an unknown property name (message starts with
   * "Unknown property") or a value that does not decode; it never aborts the
   * runtime. Core's renderer warns-and-continues on the name-level rejections
   * and rethrows value errors.
   */
  export function setProperty(nodeId: number, name: string, value: unknown): void
  /**
   * Declare which pointer deliveries the node's handlers want, as a bitmask
   * (move 1, down 2, up 4, enter 8, leave 16, wheel 32): the runtime skips
   * building events that would reach no listener. Maintained by core's
   * handler registry; apps do not call this directly.
   */
  export function setEventInterest(nodeId: number, bits: number): void
  /**
   * IME behavior for a text session, mirroring SDL's text-input properties.
   * An unset knob keeps the OS default - notably capitalization defaults to
   * "sentences" for plain text, which identifier fields and terminals want
   * off. Read when the session starts.
   */
  export interface TextInputHints {
    /** Semantic input type, steering the keyboard layout and masking. */
    type?: "text" | "name" | "email" | "username" | "password" | "number" | "pin"
    capitalize?: "none" | "sentences" | "words" | "letters"
    autocorrect?: boolean
    multiline?: boolean
  }
  /** Enable or disable text-input capture / the on-screen keyboard. */
  export function setTextInputActive(active: boolean, hints?: TextInputHints): void
  /** Request that a frame be rendered soon (coalesced by the demand-driven loop). */
  export function requestFrame(): void
  /**
   * Put the current tree on screen now (one frame). The direct draw path for
   * a flux + alloy app; requestFrame only schedules a future frame and leaves
   * the actual draw to the runner. When nothing changed since the last call
   * the retained frame is re-presented instead of laid out and painted again;
   * changed texture contents (uploads, camera frames) still show either way.
   */
  export function render(): void
  /**
   * Measure the rendered size of `text` under the given font options, without
   * adding it to the tree.
   */
  export function measureText(text: string, options?: MeasureTextOptions): { width: number, height: number }
  /**
   * Segment `text` into wrap units and shape each in the given font (through
   * the shared word cache), for laying lines out in app code; see
   * layoutNextLine in @solidrt/core. Single style; `maxLines` is ignored.
   */
  export function prepareText(text: string, options?: MeasureTextOptions): PreparedText
  /**
   * The node's bounding box from the most recent layout, relative to its
   * nearest positioning context (an ancestor with an explicit
   * `position="relative"`, falling back to the window), or `null` if it has no
   * layout or has not been laid out yet. Transforms anywhere in the chain
   * compose fully; the box is the axis-aligned bounds of the transformed quad.
   */
  export function getBoundingBox(id: number): { x: number, y: number, width: number, height: number } | null
  /**
   * Like getBoundingBox, but always window-relative (getBoundingClientRect
   * semantics), for comparing against pointer event coordinates.
   */
  export function getBoundingBoxViewport(id: number): { x: number, y: number, width: number, height: number } | null
}