import { createPortal, onCleanup } from "@solidrt/core"
import type { Color, PointerEvent } from "@solidrt/core"
import { theme } from "./theme"
import { pushNavScope } from "./focus-nav"

export interface ModalProps {
  // Called when the backdrop (the area around the content) is pressed, unless
  // `dismissable` is false.
  onClose?: () => void
  // The modal content, centered over the backdrop.
  children?: any
  // Scrim color behind the content. Defaults to the theme scrim; pass
  // "transparent" for no dim.
  backdropColor?: Color
  // Whether pressing the backdrop calls onClose. Defaults to true.
  dismissable?: boolean
}

/**
 * A centered overlay rendered at the window root via core's createPortal, so it
 * escapes the layout and stacking of its surrounding tree. It fills the window
 * with a dimming backdrop and centers `children` on top. Control visibility by
 * mounting/unmounting it, e.g. `<Show when={open()}><Modal .../></Show>`: the
 * portal's onCleanup removes it when the surrounding scope disposes. The
 * gating signal must start false: portals cannot mount during the app's
 * initial render (see createPortal), so a modal visible at startup throws.
 *
 * Pressing the backdrop calls `onClose`; pressing the content does not. This
 * works because pointer events dispatch to the whole hit path with no
 * stopPropagation, so the content is kept a sibling of the backdrop (not a
 * child): a press on the content never has the backdrop on its path.
 */
export function Modal(props: ModalProps) {
  let dismiss = (_e: PointerEvent) => {
    if (props.dismissable !== false) props.onClose?.()
  }

  // While mounted, the modal is a focus-navigation trap: its container tops
  // the nav scope stack, so createFocusNav only reaches controls inside it.
  let popNavScope: (() => void) | null = null
  onCleanup(() => popNavScope?.())

  return createPortal(
    <view
      ref={(n: { id: number }) => {
        popNavScope = pushNavScope(n)
      }}
      position="absolute"
      top={0}
      left={0}
      right={0}
      bottom={0}
      alignItems="center"
      justifyContent="center"
    >
      <view
        position="absolute"
        top={0}
        left={0}
        right={0}
        bottom={0}
        onPointerDown={dismiss}
      >
        <d-rect color={props.backdropColor ?? theme.color.scrim} />
      </view>
      {props.children}
    </view>
  )
}