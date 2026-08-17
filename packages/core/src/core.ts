import * as tree from "flux:rendertree"
import { createSignal, onCleanup } from "@solidjs/signals"
import { on } from "srt:events"

let handlers = new Map<number, Map<string, Function>>()

// Pointer-handler presence mirrored into the render tree, so the runtime can
// skip building deliveries that would reach nobody (moves over static content
// are the flood case). Bits match alloy's EventInterest (rendertree/hit.rs);
// keep the two in sync. Down/up are recorded but the runtime never gates
// them: focus and gesture side effects hang off them regardless of handlers.
const MOVE_BIT = 1
const POINTER_INTEREST: Record<string, number> = {
  onPointerMove: MOVE_BIT,
  onPointerDown: 2,
  onPointerUp: 4,
  onPointerEnter: 8,
  onPointerLeave: 16,
  onWheel: 32,
}

let interests = new Map<number, number>()

function syncInterest(nodeId: number): void {
  let mask = 0
  let nodeHandlers = handlers.get(nodeId)
  if (nodeHandlers) for (let name of nodeHandlers.keys()) mask |= POINTER_INTEREST[name] ?? 0
  // Ambient onPointerMove subscribers park a move bit on the window root: it
  // sits on every hit path, so moves keep emitting wherever the pointer is.
  if (nodeId === interestRoot && globalMoveSubs.size > 0) mask |= MOVE_BIT
  if ((interests.get(nodeId) ?? 0) === mask) return
  if (mask === 0) interests.delete(nodeId)
  else interests.set(nodeId, mask)
  tree.setEventInterest(nodeId, mask)
}

/**
 * A pointer fact with no per-node fields: window coordinates plus pointer
 * identity and modifiers. `target` is the deepest node under the pointer
 * (0 when nothing is hit).
 */
export interface GlobalPointerEvent {
  clientX: number
  clientY: number
  target: number
  pointerId: number
  pointerType: "mouse" | "touch" | "pen" | (string & {})
  shiftKey: boolean
  ctrlKey: boolean
  altKey: boolean
  metaKey: boolean
}

let globalMoveSubs = new Set<(e: GlobalPointerEvent) => void>()
let globalMoveUnsub: (() => void) | null = null
// The window root's node id while a window is attached (see attachWindow):
// where the ambient move-interest bit lands.
let interestRoot: number | null = null

/**
 * Observe every pointer move, unattached to any node - for ambient tracking
 * (cursor followers, idle detection, overlays). Element interaction belongs
 * in per-node handlers: they carry exact local coordinates, and during a drag
 * moves already follow the frozen down-path off-element. Cleans up with the
 * owning scope; also returns its unsubscribe.
 */
export function onPointerMove(fn: (e: GlobalPointerEvent) => void): () => void {
  globalMoveSubs.add(fn)
  if (globalMoveSubs.size === 1) {
    globalMoveUnsub = on("pointerMove", (raw: any) => {
      let e: GlobalPointerEvent = {
        clientX: raw.clientX,
        clientY: raw.clientY,
        target: raw.target,
        pointerId: raw.pointerId,
        pointerType: raw.pointerType,
        shiftKey: raw.shiftKey,
        ctrlKey: raw.ctrlKey,
        altKey: raw.altKey,
        metaKey: raw.metaKey,
      }
      // Copy first: a subscriber may unsubscribe (itself or others) mid-dispatch.
      for (let sub of [...globalMoveSubs]) sub(e)
    })
    if (interestRoot != null) syncInterest(interestRoot)
  }
  let cleanup = () => {
    if (!globalMoveSubs.delete(fn)) return
    if (globalMoveSubs.size === 0) {
      globalMoveUnsub?.()
      globalMoveUnsub = null
      if (interestRoot != null) syncInterest(interestRoot)
    }
  }
  onCleanup(cleanup)
  return cleanup
}

// Called by attachWindow with the window root's id (null on teardown). No
// interest write on clear: the root node is being destroyed with its window,
// and cleanupNode drops the cached mask.
export function setInterestRoot(nodeId: number | null): void {
  interestRoot = nodeId
  if (nodeId != null) syncInterest(nodeId)
}

export function setEventHandler(nodeId: number, name: string, fn: Function | null | undefined): void {
  if (fn == null) {
    handlers.get(nodeId)?.delete(name)
    if (name in POINTER_INTEREST) syncInterest(nodeId)
    return
  }
  let nodeHandlers = handlers.get(nodeId)
  if (!nodeHandlers) {
    nodeHandlers = new Map()
    handlers.set(nodeId, nodeHandlers)
  }
  nodeHandlers.set(name, fn)
  if (name in POINTER_INTEREST) syncInterest(nodeId)
}

export function getEventHandler(nodeId: number, name: string): Function | undefined {
  return handlers.get(nodeId)?.get(name)
}

// Cleans up every per-node registry entry (handlers, focus candidacy, text
// hints) when a node is destroyed.
export function cleanupNode(nodeId: number): void {
  handlers.delete(nodeId)
  // No setEventInterest call: the tree node is being destroyed with us.
  interests.delete(nodeId)
  focusables.delete(nodeId)
  textHints.delete(nodeId)
}

// Per-node IME hints, declared by the `textInputHints` prop (renderer.ts)
// and read when the node's text session starts.
let textHints = new Map<number, tree.TextInputHints>()

export function setTextInputHints(nodeId: number, hints: tree.TextInputHints | null | undefined): void {
  if (hints == null) textHints.delete(nodeId)
  else textHints.set(nodeId, hints)
}

// Currently-focused node id and text-session state. Each is a plain field
// paired with a signal: signal writes flush on the microtask, so a read
// through the signal alone is stale within the very dispatch that wrote it -
// and focus logic reads what it just moved (a tap handler focusing a field,
// then the dispatcher deciding on that focus). The plain field is the
// always-current truth; the signal exists only to make tracked scopes re-run.
// Reset across engine reloads for free: the JS environment is rebuilt.
let focusedNodeId: number | null = null
let [trackFocusedNode, setFocusedNodeSignal] = createSignal<number | null>(null)
let textInputActiveNow = false
let [trackTextInputActive, setTextInputActiveSignal] = createSignal(false)

/**
 * The focused node id, or null - as a reactive accessor: read it inside a
 * tracked scope (JSX, memo, effect) to re-run when focus moves; any read
 * (including in the same dispatch as a setFocus) sees the current value.
 * setFocus is the only writer.
 */
export function focusedNode(): number | null {
  trackFocusedNode()
  return focusedNodeId
}

/**
 * Whether a text-entry session is active on the focused node (text events
 * flowing; the on-screen keyboard up, where one is used), as a reactive
 * accessor. Distinct from focus: a field focused by navigation is not
 * editing until a tap or startTextInput() begins the session. Lets a text
 * field tell its focused and editing states apart (Enter starts editing in
 * the former, submits in the latter).
 */
export function textInputActive(): boolean {
  trackTextInputActive()
  return textInputActiveNow
}

// The native window outlives engine reloads, so a previous session may have
// left its text input active; assert the known boot state.
tree.setTextInputActive(false)

// Facts for the text-session policy, from the sticky inputDevices event
// (init + hotplug). Conservative defaults until it arrives: assume a screen
// keyboard could appear, so an eager session start is never visibly wrong.
let screenKeyboard = true
let physicalKeyboard = false
on("inputDevices", (d: { keyboard?: boolean; screenKeyboard?: boolean }) => {
  physicalKeyboard = !!d.keyboard
  screenKeyboard = !!d.screenKeyboard
  // Facts can arrive after a node was focused (boot ordering) or change under
  // it (keyboard hotplug): re-evaluate the eager session so e.g. a
  // mount-focused terminal starts receiving text without a tap.
  syncTextInput(textInputEligible() && (textInputActive() || textInputInvisible()))
})

// Focus candidacy, declared by the `focusable` prop (routed in renderer.ts).
// Candidacy only: navigation schemes enumerate candidates and move focus
// themselves via setFocus.
let focusables = new Set<number>()

export function setFocusable(nodeId: number, focusable: boolean): void {
  if (focusable) focusables.add(nodeId)
  else focusables.delete(nodeId)
}

/**
 * Node ids currently declaring `focusable`, for building focus navigation
 * (spatial/D-pad movement, tab order). A snapshot, not reactive; pair with
 * getBoundingBoxViewport for their geometry.
 */
export function getFocusables(): number[] {
  return [...focusables]
}

function textInputEligible(): boolean {
  return focusedNodeId != null && getEventHandler(focusedNodeId, "onTextInput") != null
}

// Whether starting a text session shows nothing on screen: the platform has
// no screen keyboard, or a physical keyboard is attached (the runtime keeps
// the screen keyboard down natively then too). Invisible sessions start
// eagerly at focus, so desktops and keyboard-equipped devices deliver text
// from the moment a node is focused; visible ones wait for an interaction so
// a keyboard never appears without one (the pointerDown dispatch in
// window.ts, or an explicit startTextInput).
function textInputInvisible(): boolean {
  return !screenKeyboard || physicalKeyboard
}

// The node whose session is running: a session that stays active while focus
// hops between text fields must restart on the new node so its IME hints
// (keyboard type, capitalization) take effect.
let sessionNodeId: number | null = null

function syncTextInput(active: boolean): void {
  let target = active ? focusedNodeId : null
  if (active === textInputActiveNow && target === sessionNodeId) return
  textInputActiveNow = active
  sessionNodeId = target
  setTextInputActiveSignal(active)
  if (active) tree.setTextInputActive(true, textHints.get(target!))
  else tree.setTextInputActive(false)
}

/**
 * Moves keyboard focus to `nodeId`, or clears it with `null`. Fires `onBlur` on
 * the previously focused node and `onFocus` on the new one. No-op if the node
 * is already focused.
 *
 * Focus also scopes the text-entry session of a node with an `onTextInput`
 * handler: where starting one is invisible (no screen keyboard, or a physical
 * keyboard attached) it begins at focus; where it would raise an on-screen
 * keyboard it waits for an interaction - a tap on the focused node, or
 * startTextInput() - so focus alone never summons a keyboard. Focus moving
 * between text nodes carries an active session along; focus leaving them ends
 * it (and hides the keyboard).
 */
export function setFocus(nodeId: number | null): void {
  if (nodeId === focusedNodeId) return
  let oldId = focusedNodeId
  focusedNodeId = nodeId
  setFocusedNodeSignal(nodeId)
  if (oldId != null) {
    getEventHandler(oldId, "onBlur")?.()
  }
  if (nodeId != null) {
    getEventHandler(nodeId, "onFocus")?.()
  }
  syncTextInput(textInputEligible() && (textInputActiveNow || textInputInvisible()))
}

// Interactive trigger: pointer dispatch calls this when a tap lands on the
// focused node (window.ts), the moment a pending session may raise the
// on-screen keyboard.
export function activateTextInput(): void {
  if (textInputEligible()) syncTextInput(true)
}

/**
 * Begins text entry on the focused node: enables text-event delivery and, on
 * platforms that use one (and with no physical keyboard attached), raises the
 * on-screen keyboard. Focus alone never raises it; a tap on the focused node
 * triggers this automatically, so call it only for other interactions that
 * should (a remote's select on a focused field, a search button). Throws when
 * the focused node has no onTextInput handler.
 */
export function startTextInput(): void {
  if (!textInputEligible()) {
    throw new Error("startTextInput: no focused node with an onTextInput handler")
  }
  syncTextInput(true)
}


export interface BoundingBox {
  x: number
  y: number
  width: number
  height: number
}

/**
 * Returns the node's bounding box from the most recently computed layout,
 * relative to its nearest positioning context (an ancestor with an explicit
 * `position="relative"`, falling back to the window), or `null` if the node
 * has no layout or has not been laid out yet. This is a snapshot read, not
 * reactive: call it inside `onLayout` (or an event handler) to get values for
 * the current frame. Transforms anywhere in the chain (including the node's
 * own) compose fully; the box is the axis-aligned bounds of the transformed
 * quad.
 */
export function getBoundingBox(node: { id: number }): BoundingBox | null {
  return tree.getBoundingBox(node.id)
}

/**
 * Like getBoundingBox, but always window-relative (getBoundingClientRect
 * semantics), the frame pointer event clientX/clientY are reported in.
 */
export function getBoundingBoxViewport(node: { id: number }): BoundingBox | null {
  return tree.getBoundingBoxViewport(node.id)
}

/**
 * Measures the rendered size of `text` in layout pixels under the given font
 * options (family, size, weight, style, maxLines), without adding it to the
 * tree. Useful for sizing or laying out around text before it is drawn.
 */
export function measureText(text: string, options?: tree.MeasureTextOptions): { width: number, height: number } {
  return tree.measureText(text, options)
}

/**
 * Segments `text` into wrap units (words with their trailing whitespace) and
 * shapes each in the given font, once, for laying lines out in app code with
 * layoutNextLine or arithmetic of your own over `units`. For the non-standard
 * case (text into a shape, around a moving obstacle, handed between columns,
 * fitted by size); regular text of any length is a <text>.
 */
export function prepareText(text: string, options?: tree.MeasureTextOptions): tree.PreparedText {
  return tree.prepareText(text, options)
}

/** One laid-out line from layoutNextLine. */
export type TextLine = {
  /** Unit range [from, to) into prepared.units. */
  from: number
  to: number
  /** Character range into prepared.text: `text.slice(start, end)` is the line's text (break characters included). */
  start: number
  end: number
  /** Ink width: the units' advances plus the last unit's width, without its trailing whitespace. */
  width: number
  /** Tallest ascent plus tallest descent on the line. */
  height: number
  ascent: number
  /** The line ended at a hard break rather than by running out of width. */
  hardBreak: boolean
  /** Where the next line starts; equal to `to`. */
  cursor: number
}

/**
 * The next line of `prepared` from unit `cursor` that fits `width`, or null
 * when the text is used up. Greedy: units go on the line while the pen plus
 * the unit's ink stays within `width`; a hard break ends the line; a unit
 * wider than `width` on its own goes on the line whole and overflows. Draw a
 * line as `<d-text x y w={line.width + 1}>{prepared.text.slice(line.start, line.end)}</d-text>`
 * with the same font options; its words are already shaped, so that is
 * cheap. Floats, balancing and ellipsis are <text> features, not this.
 */
export function layoutNextLine(prepared: tree.PreparedText, cursor: number, width: number): TextLine | null {
  let units = prepared.units
  if (cursor >= units.length) return null
  let pen = 0
  let ascent = 0
  let descent = 0
  let i = cursor
  while (i < units.length) {
    let unit = units[i]!
    if (i > cursor && pen + unit.width > width) break
    pen += unit.advance
    if (unit.ascent > ascent) ascent = unit.ascent
    if (unit.descent > descent) descent = unit.descent
    i++
    if (unit.hardBreak) break
  }
  let last = units[i - 1]!
  return {
    from: cursor,
    to: i,
    start: units[cursor]!.start,
    end: last.end,
    width: pen - last.advance + last.width,
    height: ascent + descent,
    ascent,
    hardBreak: last.hardBreak,
    cursor: i,
  }
}