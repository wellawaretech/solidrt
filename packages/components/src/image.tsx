import { createImage, createEffect, Loading, Errored } from "@solidrt/core"
import type { ImageSource, LayoutProps, PointerProps } from "@solidrt/core"
import type { StyleProps } from "./types"

export interface ImageProps extends PointerProps {
  src: string | Uint8Array
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
function FallbackTexture(props: { src: ImageSource; width?: number; height?: number }) {
  let tex = createImage(() => props.src)
  return (
    <Errored fallback={(_err: unknown) => null}>
      <texture src={tex()} width={props.width} height={props.height} />
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

  // A texture sizes from its own width/height (not the box around it), so the
  // numeric layout dimensions are forwarded to it. Omitting height lets the
  // texture follow the image's intrinsic aspect ratio.
  let texW = () => (typeof props.layout?.width === "number" ? props.layout.width : undefined)
  let texH = () => (typeof props.layout?.height === "number" ? props.layout.height : undefined)

  return (
    <view
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
        <d-rect color={props.style?.backgroundColor} radius={props.style?.borderRadius} />
      ) : null}
      <Loading fallback={null}>
        <Errored
          fallback={(_err: unknown) =>
            props.fallback != null ? <FallbackTexture src={props.fallback} width={texW()} height={texH()} /> : null
          }
        >
          <texture src={src()} width={texW()} height={texH()} />
        </Errored>
      </Loading>
      {hasBorder() ? (
        <d-rect
          drawStyle="stroke"
          color={props.style?.borderColor ?? "transparent"}
          strokeWidth={props.style?.borderWidth}
          radius={props.style?.borderRadius}
        />
      ) : null}
    </view>
  )
}