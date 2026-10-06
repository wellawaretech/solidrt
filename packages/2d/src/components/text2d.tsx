import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { setGroupTransition } from "../layer.ts"
import type { SpritePointerEvent, SpriteTapEvent, SpriteTransition, SpriteWheelEvent, TransitionEndEvent } from "../layer.ts"
import { addText, destroyText, setText } from "../text.ts"
import type { AddTextOptions, TextRun } from "../text.ts"
import { GroupContext, LayerContext } from "./context.ts"

export type Text2dProps = Omit<AddTextOptions, "parent"> & {
  /** Bubbled from a hit glyph sprite to the run's group (see
   * SpritePointerProps); a run never receives enter/leave. */
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  onPointerCancel?: (event: SpritePointerEvent) => void
  onWheel?: (event: SpriteWheelEvent) => void
  onTap?: (event: SpriteTapEvent) => void
  /** How pose-prop changes animate (the run's group; see
   * setGroupTransition). The mount pose snaps. */
  transition?: SpriteTransition | string | null
  onTransitionEnd?: (event: TransitionEndEvent) => void
  ref?: (run: TextRun) => void
}

/**
 * A text run in the enclosing `<SpriteLayer>` (under the enclosing
 * `<Group>` when there is one): `text` drawn from `font`'s atlas as glyph
 * sprites, at `x`/`y` in the group's frame, the style props re-laying it
 * out when they change. The layer must declare `font.atlas`. Renders
 * nothing itself; `Camera2d`, picking and `orderBy` see the glyphs as
 * sprites.
 */
export let Text2d: VoidComponent<Text2dProps> = props => {
  let layer = useContext(LayerContext)
  let parent = useContext(GroupContext)
  let run = untrack(() =>
    addText(layer, {
      font: props.font,
      text: props.text,
      x: props.x,
      y: props.y,
      rotation: props.rotation,
      visible: props.visible,
      parent,
    }),
  )
  createEffect(
    () => [props.x, props.y, props.rotation, props.visible] as const,
    ([x, y, rotation, visible]) => setText(run, { x, y, rotation, visible: visible !== false }),
  )
  createEffect(
    () =>
      [props.text, props.fontSize, props.lineHeight, props.letterSpacing, props.maxWidth, props.align, props.anchor, props.anchorY, props.tint, props.outline, props.renderOrder] as const,
    ([text, fontSize, lineHeight, letterSpacing, maxWidth, align, anchor, anchorY, tint, outline, renderOrder]) =>
      setText(run, { text, fontSize, lineHeight, letterSpacing, maxWidth, align, anchor, anchorY, tint, outline, renderOrder }),
  )
  // After the pose effect, so the mount pose snaps before writes animate.
  createEffect(
    () => props.transition,
    transition => setGroupTransition(run.group, transition ?? null),
  )
  createEffect(
    () => [props.onPointerDown, props.onPointerMove, props.onPointerUp, props.onPointerCancel, props.onWheel, props.onTap, props.onTransitionEnd] as const,
    ([down, move, up, cancel, wheel, tap, end]) => {
      run.group.onPointerDown = down
      run.group.onPointerMove = move
      run.group.onPointerUp = up
      run.group.onPointerCancel = cancel
      run.group.onWheel = wheel
      run.group.onTap = tap
      run.group.onTransitionEnd = end
    },
  )
  untrack(() => props.ref)?.(run)
  onCleanup(() => destroyText(run))
  return null
}
