import { createImage, Loading } from "@solidrt/core"
import type { LayoutProps, PointerProps } from "@solidrt/core"
import type { StyleProps } from "./types"

export interface ImageProps extends PointerProps {
  src: string | Uint8Array
  layout?: LayoutProps
  style?: StyleProps
}

//TODO onLoad, onError
export function Image(props: ImageProps) {
  // createImage fetches/decodes/uploads, swaps the texture when src changes, and
  // frees it on cleanup, returning the texture id. Pass an accessor so a reactive
  // src reloads. Reading src() suspends until ready (a SolidJS 2.0 async value),
  // so the <texture> below sits in a <Loading> boundary.
  let src = createImage(() => props.src)
  let hasBorder = () => (props.style?.borderWidth ?? 0) > 0

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
        <texture src={src()} width={texW()} height={texH()} />
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