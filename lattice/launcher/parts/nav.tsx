// Remote navigation: the launcher's pointer-free control path, so a TV
// remote, a keyboard, or a gamepad can drive it. A module-scope registry (the
// dev-connection pattern) holds the mounted screens' pressable targets; arrow
// keys and dpad presses move a focus between them and Enter / the remote's
// center key / the gamepad south button activates the focused one. Purely
// additive to pointer input: nothing is focused until the first navigation
// press, and pointer presses work unchanged throughout.
//
// A TV remote arrives as key events (dpad = arrows, center = the "Select"
// code), a game controller through the gamepads snapshot stream - both feed
// the same move/activate calls. Focus moves spatially: the nearest target in
// the pressed direction wins, judged by bounding boxes at press time, so
// registration order never matters. A modal target (the confirm dialogs)
// traps navigation: while any is mounted, only modal targets are reachable.
//
// Stage-1 gaps, deliberate: no scroll-into-view (a focused off-screen app row
// stays off-screen), no held-dpad auto-repeat on gamepads (keyboards repeat
// on their own), and text inputs are not navigation targets (typing needs a
// pointer or a hardware keyboard anyway).
import {
  createSignal,
  onSettled,
  untrack,
  getFocusedNodeId,
  getBoundingBoxViewport,
} from "@solidrt/core"
import { on } from "srt:events"
import { Button, theme, type ButtonProps, type StyleProps } from "@solidrt/components"

type Direction = "up" | "down" | "left" | "right"

type Target = {
  node: { id: number } | null
  action: () => void
  modal: () => boolean
  disabled: () => boolean
}

let targets: Target[] = []
// ownedWrite: cleared from unregister cleanups, which run inside disposal.
let [focusedTarget, setFocusedTarget] = createSignal<Target | null>(null, { ownedWrite: true })

/**
 * Register a pressable as a remote-navigation target. Call in a component
 * body; the target unregisters when the component unmounts. Attach `ref` to
 * the target's view (its bounds drive the spatial move) and read `focused()`
 * into its styling (see navRing). Options are getters so they stay live.
 */
export function navTarget(
  action: () => void,
  opts?: { modal?: () => boolean; disabled?: () => boolean },
) {
  let target: Target = {
    node: null,
    action,
    modal: opts?.modal ?? (() => false),
    disabled: opts?.disabled ?? (() => false),
  }
  targets.push(target)
  onSettled(() => () => {
    targets.splice(targets.indexOf(target), 1)
    if (untrack(focusedTarget) === target) setFocusedTarget(null)
  })
  return {
    ref: (n: { id: number }) => {
      target.node = n
    },
    focused: () => focusedTarget() === target,
  }
}

/**
 * The focus ring, spread into a target's style. Present while focused, empty
 * otherwise. Text-colored rather than primary so it stays visible on
 * primary-filled buttons.
 */
export function navRing(focused: boolean, radius?: number): StyleProps {
  if (!focused) return {}
  return {
    borderWidth: 2,
    borderColor: theme.color.text,
    borderRadius: radius ?? theme.radius.md,
  }
}

/**
 * A Button that remote navigation can reach: registers its press as a nav
 * target and wears the focus ring. Drop-in for Button at the launcher's call
 * sites; `modal` marks confirm-dialog buttons, which trap navigation while
 * mounted.
 */
export function NavButton(props: ButtonProps & { modal?: boolean }) {
  let nav = navTarget(() => props.onPress?.(), {
    modal: () => props.modal ?? false,
    disabled: () => props.disabled ?? false,
  })
  return (
    <Button
      ref={nav.ref}
      variant={props.variant}
      size={props.size}
      onPress={props.onPress}
      disabled={props.disabled}
      layout={props.layout}
      style={{ ...props.style, ...navRing(nav.focused()) }}
    >
      {props.children}
    </Button>
  )
}

type Placed = { target: Target; x: number; y: number }

// The currently reachable targets with their centers: mounted, enabled, and
// laid out - and only the modal ones while any modal target is up.
function reachable(): Placed[] {
  let usable = targets.filter((t) => !t.disabled())
  let modal = usable.filter((t) => t.modal())
  let placed: Placed[] = []
  for (let t of modal.length > 0 ? modal : usable) {
    let b = t.node && getBoundingBoxViewport(t.node)
    if (b) placed.push({ target: t, x: b.x + b.width / 2, y: b.y + b.height / 2 })
  }
  return placed
}

// Entry focus for a set the current focus is not part of (first press, screen
// change, a modal opening): the topmost target, leftmost among near-ties.
function focusFirst(placed: Placed[]) {
  let first = placed.reduce((a, b) =>
    b.y < a.y - 1 || (Math.abs(b.y - a.y) <= 1 && b.x < a.x) ? b : a,
  )
  setFocusedTarget(first.target)
}

function move(dir: Direction) {
  let placed = reachable()
  if (placed.length === 0) return
  let cur = untrack(focusedTarget)
  let from = cur && placed.find((p) => p.target === cur)
  if (!from) return focusFirst(placed)
  let best: Placed | null = null
  let bestScore = Infinity
  for (let p of placed) {
    if (p === from) continue
    let dx = p.x - from.x
    let dy = p.y - from.y
    // Progress along the pressed direction is required; among candidates the
    // nearest mostly-aligned one wins (cross-axis distance weighs double).
    let ahead = dir === "up" ? -dy : dir === "down" ? dy : dir === "left" ? -dx : dx
    if (ahead <= 1) continue
    let across = Math.abs(dir === "up" || dir === "down" ? dx : dy)
    let score = ahead + 2 * across
    if (score < bestScore) {
      bestScore = score
      best = p
    }
  }
  if (best) setFocusedTarget(best.target)
}

function activate() {
  let placed = reachable()
  if (placed.length === 0) return
  let cur = untrack(focusedTarget)
  let hit = cur && placed.find((p) => p.target === cur)
  // Nothing (reachable) focused yet: the press lands focus instead of acting.
  if (!hit) return focusFirst(placed)
  hit.target.action()
}

// Keyboard and TV remote. Arrows move (key repeat walks through targets);
// Enter or the remote's center key activates - the center key's `key` is
// unnamed ("Unidentified"), so it is matched by code. While a node holds real
// focus (a TextInput), the keyboard is entirely its.
on("keydown", (e: { key: string; code: string; repeat: boolean }) => {
  if (getFocusedNodeId() != null) return
  if (e.key === "ArrowUp") move("up")
  else if (e.key === "ArrowDown") move("down")
  else if (e.key === "ArrowLeft") move("left")
  else if (e.key === "ArrowRight") move("right")
  else if ((e.key === "Enter" || e.code === "Select") && !e.repeat) activate()
})

// Gamepads: edge-detect the dpad and south button on the union of all pads'
// pressed buttons. The sticky replay on subscribe seeds the baseline.
let prevButtons = new Set<string>()
on("gamepads", (e: { pads?: ({ buttons: string[] } | null)[] }) => {
  let now = new Set<string>()
  for (let pad of e.pads ?? []) for (let b of pad?.buttons ?? []) now.add(b)
  for (let b of now) {
    if (prevButtons.has(b)) continue
    if (b === "dpadUp") move("up")
    else if (b === "dpadDown") move("down")
    else if (b === "dpadLeft") move("left")
    else if (b === "dpadRight") move("right")
    else if (b === "south") activate()
  }
  prevButtons = now
})
