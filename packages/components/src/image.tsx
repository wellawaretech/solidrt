import { createSignal, createEffect, onCleanup } from "@solidjs/signals"
import { decodeImage, createTexture } from "@solidrt/core/gpu"
import type { LayoutProps, PointerProps } from "@solidrt/core"
import type { StyleProps } from "./types"

export interface ImageProps extends PointerProps {
  src: string | Uint8Array
  layout?: LayoutProps
  style?: StyleProps
}

//TODO onLoad, onError
//TODO release texture in onCleanup
export function Image(props: ImageProps) {
  let [res, setRes] = createSignal<{ id: number; width: number; height: number }>()

  // Load (and decode) whenever src changes. A url is fetched; bytes are used
  // directly. The async result is pushed into the signal so the texture shows
  // once ready; a stale flag drops results from a superseded src.
  createEffect(
    () => props.src,
    (source) => {
      let stale = false

      ;(async () => {
        let bytes: Uint8Array
        if (typeof source === "string") {
          let response = await fetch(source)
          bytes = await response.bytes()
        } else {
          bytes = source
        }
        if (stale) return
        let { data, width, height } = decodeImage(bytes)
        let id = createTexture(data, width, height)
        setRes({ id, width, height })
      })()

      return () => {
        stale = true
      }
    },
  )

  let src = () => res()?.id
  let hasBorder = () => (props.style?.borderWidth ?? 0) > 0

  // A texture sizes from its own width/height (not the box around it), so the
  // numeric layout dimensions are forwarded to it. Omitting height lets the
  // texture follow the image's intrinsic aspect ratio.
  let texW = () => (typeof props.layout?.width === "number" ? props.layout.width : undefined)
  let texH = () => (typeof props.layout?.height === "number" ? props.layout.height : undefined)

  onCleanup(() => {
    //TODO release texture
  })

  return (
    <view
      {...props.layout}
      overflow={props.style?.borderRadius != null ? "hidden" : props.layout?.overflow}
      clipRadius={props.style?.borderRadius}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
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
      <texture src={src()} width={texW()} height={texH()} />
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