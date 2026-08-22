import { createImage, createEffect, Loading, Errored, pct } from "@solidrt/core"
import type { ImageSource, LayoutProps, Pct, PointerProps, TextureProps } from "@solidrt/core"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export interface ImageProps extends PointerProps, TransitionProps {
  src: string | Uint8Array
  /**
   * How the image maps into the Image's box (CSS object-fit): "fill"
   * stretches, "cover"/"none" crop, "contain"/"scale-down" letterbox,
   * everything centered. Requires the Image to have a box: give `layout` a
   * size in any form (numbers, `pct()`, flex). Without `fit` the image keeps
   * its legacy sizing: numeric layout sizes are honored, anything else draws
   * at intrinsic size.
   */
  fit?: TextureProps["fit"]
  /** Image source to show when `src` fails to load. If this also fails, only
   * the `backgroundColor` placeholder remains. */
  fallback?: ImageSource
  /** Called each time a source finishes loading (including reloads on a
   * reactive `src`). */
  onLoad?: () => void
  /** Called when `src` fails to load or decode. The fallback (if any) is shown
   * regardless; without one the error stays contained here instead of
   * propagating to an outer boundary. */
  onError?: (err: unknown) => void
  layout?: LayoutProps
  style?: StyleProps
}

// Renders the fallback source when the main one failed. Its own <Errored>
// keeps a broken fallback from escaping: the placeholder stays instead.
// Both fallbacks here take the error argument: an arity >= 1 fallback tells
// <Errored> the error is handled, so dev builds do not console.error it.
function FallbackTexture(props: {
  src: ImageSource
  fit?: TextureProps["fit"]
  width?: number | Pct
  height?: number | Pct
}) {
  let tex = createImage(() => props.src)

  return (
    <Errored fallback={(_err: unknown) => null}>
      <texture src={tex()} fit={props.fit} width={props.width} height={props.height} />
    </Errored>
  )
}

export function Image(props: ImageProps) {
  // createImage fetches/decodes/uploads, swaps the texture when src changes, and
  // frees it on cleanup, returning the texture id. Pass an accessor so a reactive
  // src reloads. Reading src() suspends until ready (a SolidJS 2.0 async value),
  // so the <texture> below sits in a <Loading> boundary.
  let src = createImage(() => props.src)
  let hasBorder = () => (props.style?.borderWidth ?? 0) > 0

  // Load/error callbacks ride a separate effect read of the same async value;
  // the error handler also keeps the failure out of the console when the
  // render side already contains it with a fallback.
  createEffect(() => src(), {
    effect: () => props.onLoad?.(),
    error: (err: unknown) => props.onError?.(err),
  })

  // A texture sizes from its own width/height (not the box around it). With
  // `fit` the texture simply fills the Image's box and the fit maps the pixels
  // into it. Without it, legacy sizing: numeric layout dimensions are
  // forwarded, and omitting height lets the texture follow the image's
  // intrinsic aspect ratio.
  let texW = () => (props.fit != null ? pct(100) : typeof props.layout?.width === "number" ? props.layout.width : undefined)
  let texH = () =>
    props.fit != null ? pct(100) : typeof props.layout?.height === "number" ? props.layout.height : undefined
  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      {...props.layout}
      overflow={props.style?.borderRadius != null ? "hidden" : props.layout?.overflow}
      clipRadius={props.style?.borderRadius}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      opacity={props.style?.opacity}
      onPointerEnter={props.onPointerEnter}
      onPointerLeave={props.onPointerLeave}
      onPointerDown={props.onPointerDown}
      onPointerUp={props.onPointerUp}
      onPointerMove={props.onPointerMove}
      onWheel={props.onWheel}
      onFocus={props.onFocus}
      onBlur={props.onBlur}
      onKeyDown={props.onKeyDown}
      onKeyUp={props.onKeyUp}
      onTextInput={props.onTextInput}
      pointerEvents={props.pointerEvents}
    >
      {props.style?.backgroundColor != null ? (
        <d-rect transition={split().background} onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)} color={props.style?.backgroundColor} radius={props.style?.borderRadius} />
      ) : null}
      <Loading fallback={null}>
        <Errored
          fallback={(_err: unknown) =>
            props.fallback != null ? (
              <FallbackTexture src={props.fallback} fit={props.fit} width={texW()} height={texH()} />
            ) : null
          }
        >
          <texture src={src()} fit={props.fit} width={texW()} height={texH()} />
        </Errored>
      </Loading>
      {hasBorder() ? (
        <d-rect
          drawStyle="stroke"
          transition={split().border}
          onTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
          color={props.style?.borderColor ?? "transparent"}
          strokeWidth={props.style?.borderWidth}
          radius={props.style?.borderRadius}
        />
      ) : null}
    </view>
  )
}