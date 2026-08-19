// The Solid face: PascalCase components over context, syncing the retained
// layer (layer.ts) - no new intrinsic elements, no renderer changes. Props
// follow the Solid 2.0 model (reactive values, no destructuring); effects
// write into the retained records and the runtime's dirty flush renders.
// Anything moving at frame rate can bypass the declarative layer: grab the
// sprite with `ref` and call setSprite from onFrame - signals carry
// structure and slow state, per-frame motion goes straight to the layer.
// The same split, with the same reasoning, as @solidrt/3d's components.
import { createContext, createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent, TextureId, VoidComponent } from "@solidrt/core"
import { addSprite, createSpriteLayer, removeSprite, setSprite } from "./layer.ts"
import type { CameraUpdate, Sprite as SpriteHandle, SpriteLayer as LayerHandle, SpriteOptions, SpritePointerEvent } from "./layer.ts"

let LayerContext = createContext<LayerHandle>()

/**
 * The enclosing layer - the imperative escape hatch inside a component
 * subtree (throws outside a `<SpriteLayer>`).
 */
export function useSpriteLayer(): LayerHandle {
  return useContext(LayerContext)
}

export type SpritePointerProps = {
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  onPointerEnter?: (event: SpritePointerEvent) => void
  onPointerLeave?: (event: SpritePointerEvent) => void
}

export type SpriteLayerProps = {
  /** Layer pixels. With `output`, the leaf's own width/height are layout, so
   * render size and display size separate. */
  width: number
  height: number
  /** The atlas texture every sprite samples (create with createAtlas). */
  atlas: TextureId
  /** Record capacity, fixed for the layer's life; default 1024. */
  capacity?: number
  clearColor?: [number, number, number, number]
  /** Pan/zoom over the world; a shared-params write, never per-sprite. */
  camera?: CameraUpdate
  label?: string
  ref?: (layer: LayerHandle) => void
  /**
   * Compose the output yourself: called once (untracked) with the layer's
   * texture id, and its return renders in place of the built-in `<texture>`
   * leaf. Sprite pointer events then need the layer's handlers on your
   * leaf: `<texture src={texture} {...useSpriteLayer().handlers} />`.
   */
  output?: (texture: TextureId) => Element
  /**
   * Sprite pointer events (default on): the built-in leaf carries
   * layer.handlers. `false` detaches them - the leaf then costs no pointer
   * routing at all.
   */
  events?: boolean
}

/**
 * Owns a sprite layer and composites it as an ordinary `<texture>` leaf, so
 * the output takes layout, transforms, blendMode, and pointer events like
 * any element - or hand `output` the texture id and compose it yourself.
 * Children (`<Sprite>`) render nothing themselves - they populate the
 * retained layer through context.
 */
export let SpriteLayer: ParentComponent<SpriteLayerProps> = props => {
  let layer = untrack(() =>
    createSpriteLayer(props.width, props.height, props.atlas, {
      capacity: props.capacity,
      clearColor: props.clearColor,
      label: props.label,
    }),
  )
  createEffect(
    () => [props.width, props.height] as const,
    ([w, h]) => layer.setSize(w, h),
  )
  createEffect(
    () => props.camera,
    camera => {
      if (camera) layer.setCamera(camera)
    },
  )
  untrack(() => props.ref)?.(layer)
  let output = untrack(() => props.output)
  let events = untrack(() => props.events) !== false
  return (
    <LayerContext value={layer}>
      {output ? (
        untrack(() => output(layer.texture))
      ) : (
        <texture
          src={layer.texture}
          width={props.width}
          height={props.height}
          onPointerDown={events ? layer.handlers.onPointerDown : undefined}
          onPointerMove={events ? layer.handlers.onPointerMove : undefined}
          onPointerUp={events ? layer.handlers.onPointerUp : undefined}
          onPointerLeave={events ? layer.handlers.onPointerLeave : undefined}
        />
      )}
      {props.children}
    </LayerContext>
  )
}

export type SpriteProps = SpriteOptions &
  SpritePointerProps & {
    ref?: (sprite: SpriteHandle) => void
  }

/** One sprite: a frame drawn at a position, top of the draw order at mount. */
export let Sprite: VoidComponent<SpriteProps> = props => {
  let layer = useContext(LayerContext)
  let sprite = untrack(() => addSprite(layer))
  createEffect(
    () => [props.x, props.y, props.w, props.h, props.frame, props.rotation, props.tint] as const,
    ([x, y, w, h, frame, rotation, tint]) => setSprite(sprite, { x, y, w, h, frame, rotation, tint }),
  )
  createEffect(
    () => [props.onPointerDown, props.onPointerMove, props.onPointerUp, props.onPointerEnter, props.onPointerLeave] as const,
    ([down, move, up, enter, leave]) => {
      sprite.onPointerDown = down
      sprite.onPointerMove = move
      sprite.onPointerUp = up
      sprite.onPointerEnter = enter
      sprite.onPointerLeave = leave
    },
  )
  untrack(() => props.ref)?.(sprite)
  onCleanup(() => removeSprite(sprite))
  return null
}
