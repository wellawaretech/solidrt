import * as tree from "flux:rendertree"
import { createSignal } from "@solidjs/signals"
import { on } from "srt:events"

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

// Cleans up every per-node registry entry (handlers, focus candidacy) when a
// node is destroyed.
export function cleanupNode(nodeId: number): void {
  handlers.delete(nodeId)
  focusables.delete(nodeId)
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

function syncTextInput(active: boolean): void {
  if (active === textInputActiveNow) return
  textInputActiveNow = active
  setTextInputActiveSignal(active)
  tree.setTextInputActive(active)
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