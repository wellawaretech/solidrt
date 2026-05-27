import { createSignal, onCleanup } from "@solidjs/signals"
import { decodeImage, createTexture } from "@solidrt/core/gpu"
import type { LayoutProps, TransformProps, PointerProps } from "@solidrt/core"

export interface ImageProps extends LayoutProps, TransformProps, PointerProps {
  src: string | Uint8Array
}

//TODO onLoad, onError
//TODO release texture in onCleanup
//TODO forward all LayoutProps (currently only the common subset)
export function Image(props: ImageProps) {
  let [res] = createSignal(async () => {
    let bytes: Uint8Array
    if (typeof props.src === "string") {
      let response = await fetch(props.src)
      bytes = await response.bytes()
    } else {
      bytes = props.src
    }
    let { data, width, height } = decodeImage(bytes)
    let id = createTexture(data, width, height)
    return { id, width, height }
  })

  let src = () => res()?.id

  onCleanup(() => {
    //TODO release texture
  })

  return (
    <view
      width={props.width}
      height={props.height}
      flex={props.flex}
      flexGrow={props.flexGrow}
      flexShrink={props.flexShrink}
      alignSelf={props.alignSelf}
      margin={props.margin}
      marginTop={props.marginTop}
      marginBottom={props.marginBottom}
      marginLeft={props.marginLeft}
      marginRight={props.marginRight}
      padding={props.padding}
      position={props.position}
      top={props.top}
      right={props.right}
      bottom={props.bottom}
      left={props.left}
      x={props.x}
      y={props.y}
      scale={props.scale}
      rotate={props.rotate}
      onPointerEnter={props.onPointerEnter}
      onPointerLeave={props.onPointerLeave}
      onPointerDown={props.onPointerDown}
      onPointerUp={props.onPointerUp}
      onPointerMove={props.onPointerMove}
      onWheel={props.onWheel}
      pointerEvents={props.pointerEvents}
    >
      <texture src={src()} />
    </view>
  )
}