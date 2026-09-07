import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { addSprite, removeSprite, setSprite, setSpriteTransition } from "../layer.ts"
import type { Sprite as SpriteHandle, SpriteOptions, SpritePointerEvent, SpriteTapEvent, SpriteWheelEvent, TransitionEndEvent } from "../layer.ts"
import type { NodeTransition } from "flux:spatial"
import { GroupContext, LayerContext } from "./context.ts"

/**
 * Sprite pointer events, the element vocabulary one tree deeper: the
 * topmost hit sprite receives the event, down/move/up/wheel/tap bubble to
 * enclosing Groups and end at the `<SpriteLayer>` (stopPropagation stops
 * the walk; stopping a down claims the whole press), enter/leave pair on
 * the sprite alone. Events flow while the element showing the layer
 * carries layer.handlers - the built-in `<SpriteLayer>` leaf does (opt
 * out with events={false}); an `output` leaf spreads them itself.
 */
export type SpritePointerProps = {
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  /** Sprites only: a Group never receives enter/leave. */
  onPointerEnter?: (event: SpritePointerEvent) => void
  onPointerLeave?: (event: SpritePointerEvent) => void
  onWheel?: (event: SpriteWheelEvent) => void
  /** A press released on the sprite without dragging; `tapCount` counts
   * repeats (2 = double tap). */
  onTap?: (event: SpriteTapEvent) => void
}

export type SpriteProps = SpriteOptions &
  SpritePointerProps & {
    /** How pose-prop changes animate (see setSpriteTransition); the mount
     * pose always snaps. */
    transition?: NodeTransition | string | null
    /** A declared transition settled on one component. */
    onTransitionEnd?: (event: TransitionEndEvent) => void
    ref?: (sprite: SpriteHandle) => void
  }

/** One sprite: a frame drawn at a position (in the enclosing `<Group>`'s
 * frame when there is one). */
export let Sprite: VoidComponent<SpriteProps> = props => {
  let layer = useContext(LayerContext)
  let parent = useContext(GroupContext)
  let sprite = untrack(() => addSprite(layer, { parent }))
  createEffect(
    () => [props.x, props.y, props.w, props.h, props.frame, props.flipX, props.flipY, props.rotation, props.tint, props.renderOrder, props.visible] as const,
    ([x, y, w, h, frame, flipX, flipY, rotation, tint, renderOrder, visible]) =>
      setSprite(sprite, { x, y, w, h, frame, flipX, flipY, rotation, tint, renderOrder, visible: visible !== false }),
  )
  // After the pose effect, so the mount pose snaps before writes animate.
  createEffect(
    () => props.transition,
    transition => setSpriteTransition(sprite, transition ?? null),
  )
  createEffect(
    () => [props.onPointerDown, props.onPointerMove, props.onPointerUp, props.onPointerEnter, props.onPointerLeave, props.onWheel, props.onTap, props.onTransitionEnd] as const,
    ([down, move, up, enter, leave, wheel, tap, end]) => {
      sprite.onPointerDown = down
      sprite.onPointerMove = move
      sprite.onPointerUp = up
      sprite.onPointerEnter = enter
      sprite.onPointerLeave = leave
      sprite.onWheel = wheel
      sprite.onTap = tap
      sprite.onTransitionEnd = end
    },
  )
  untrack(() => props.ref)?.(sprite)
  onCleanup(() => removeSprite(sprite))
  return null
}
