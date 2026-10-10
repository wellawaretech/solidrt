import { createPortal, onBack, onCleanup } from "@solidrt/core"
import type { Color } from "@solidrt/core"
import { theme } from "./theme"
import { pushNavScope } from "./focus-nav"
import { colorFade, popupFade } from "./motion"

export interface ModalProps {
  // Called when the backdrop (the area around the content) is pressed, or on
  // Escape or the platform's back, unless `dismissable` is false.
  onClose?: () => void
  // The modal content, centered over the backdrop.
  children?: any
  // Scrim color behind the content. Defaults to the theme scrim; pass
  // "transparent" for no dim.
  backdropColor?: Color
  // Whether a backdrop press, Escape or back calls onClose. Defaults to true.
  // False keeps the modal up until the app removes it; back still stops at
  // the modal, never reaching the screen beneath.
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
 *
 * While mounted the modal is one step of core's back stack: Escape and the
 * user's back close it as the backdrop does. Registered at mount, so a field
 * focused inside it (which registers at focus) sits above it and a back
 * leaves the field first, the modal next.
 */
export function Modal(props: ModalProps) {
  let dismiss = () => {
    if (props.dismissable !== false) props.onClose?.()
  }

  // A modal that is not dismissable still takes the step and does nothing:
  // it wants an answer, and the screen beneath must not get the back.
  onBack((e) => {
    e.preventDefault()
    dismiss()
  })

  // While mounted, the modal is a focus-navigation trap: its container tops
  // the nav scope stack, so createFocusNav only reaches controls inside it.
  let popNavScope: (() => void) | null = null
  onCleanup(() => popNavScope?.())

  // The whole overlay (scrim and content together) fades in at mount and
  // out on removal via popupFade; an exiting modal is hit-test invisible,
  // so a press during the fade-out reaches what is behind it.
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
      opacity={1}
      transition={popupFade()}
    >
      <view
        position="absolute"
        top={0}
        left={0}
        right={0}
        bottom={0}
        onPointerDown={dismiss}
      >
        <d-rect transition={colorFade()} color={props.backdropColor ?? theme.color.scrim} />
      </view>
      {props.children}
    </view>
  )
}