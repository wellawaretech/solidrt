import { createEffect, createSignal, onCleanup } from "@solidjs/signals"
import { measureText, setFocus } from "@solidrt/core"
import { createCaretScroll, createTextBuffer } from "@solidrt/core/text-input"
import type { LayoutProps } from "@solidrt/core"
import type { StyleProps } from "./types"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"

// Caret thickness. Shared so the drawn caret and the scroll offset's reserved
// edge column cannot drift apart.
const CARET_WIDTH = 1

// Shaping width handed to the detached value/placeholder text: effectively
// unbounded, so a single line never wraps. The viewport clips it and scrollX
// slides it.
const TEXT_SHAPE_WIDTH = 1e9

export interface TextInputProps {
  value?: string
  defaultValue?: string
  onInput?: (value: string) => void
  onSubmit?: (value: string) => void
  onFocus?: () => void
  onBlur?: () => void

  placeholder?: string
  maxLength?: number
  disabled?: boolean
  autoFocus?: boolean

  layout?: LayoutProps
  style?: StyleProps
}

// Single-line. The caret moves through the text (Left/Right/Home/End), edits
// happen at the caret, and the inner box scrolls to keep it in view. Printable
// text arrives via onTextInput (post-IME commit). onKeyDown handles caret
// movement, Backspace/Delete, Enter, Escape. Range selection (shift-movement,
// highlight, click-to-position) is not wired yet. Outside-click-to-blur is the
// caller's job.
export function TextInput(props: TextInputProps) {
  let [focused, setFocused] = createSignal(false)
  let [caretOn, setCaretOn] = createSignal(true)

  let node: { id: number } | undefined
  let viewport: { id: number } | undefined
  let blinkId: any = null

  let buffer = createTextBuffer({
    value: () => props.value,
    defaultValue: props.defaultValue,
    onInput: (v) => props.onInput?.(v),
    maxLength: () => props.maxLength,
  })
  let value = buffer.value

  // autoFocus runs in an effect, not the ref: setFocus fires onFocus and reads
  // the node's onTextInput handler to toggle the keyboard, and those handlers
  // are only registered after the element's props are applied. The ref can fire
  // before that, so focusing there would no-op.
  createEffect(
    () => props.autoFocus,
    (autoFocus) => {
      if (autoFocus && node) setFocus(node.id)
    },
  )

  let handlePointerDown = () => {
    if (props.disabled) return
    if (node) setFocus(node.id)
  }

  let handleFocus = () => {
    setFocused(true)
    setCaretOn(true)
    if (blinkId == null) {
      blinkId = setInterval(() => setCaretOn((v) => !v), 500)
    }
    props.onFocus?.()
  }

  let handleBlur = () => {
    setFocused(false)
    if (blinkId != null) {
      clearInterval(blinkId)
      blinkId = null
    }
    props.onBlur?.()
  }

  let handleKeyDown = (e: any) => {
    if (props.disabled) return
    if (e.key === "Backspace") {
      buffer.deleteBackward()
      setCaretOn(true)
    } else if (e.key === "Delete") {
      buffer.deleteForward()
      setCaretOn(true)
    } else if (e.key === "Left") {
      buffer.move("left")
      setCaretOn(true)
    } else if (e.key === "Right") {
      buffer.move("right")
      setCaretOn(true)
    } else if (e.key === "Home") {
      buffer.move("start")
      setCaretOn(true)
    } else if (e.key === "End") {
      buffer.move("end")
      setCaretOn(true)
    } else if (e.key === "Return" || e.key === "Enter") {
      props.onSubmit?.(value())
      setFocus(null)
    } else if (e.key === "Escape") {
      if (node) setFocus(null)
    }
  }

  let handleTextInput = (e: any) => {
    if (props.disabled) return
    buffer.insertText(e.text ?? "")
    setCaretOn(true)
  }

  onCleanup(() => {
    if (blinkId != null) clearInterval(blinkId)
  })

  // Style overrides fall back to theme defaults. The border doubles as the
  // focus ring: primary while focused, when the focus-ring policy asks for a
  // visible indicator.
  let textColor = () => props.style?.color ?? theme.color.text
  let surfaceColor = () => props.style?.backgroundColor ?? theme.color.surface
  let borderColor = () =>
    props.style?.borderColor ?? (focused() && policy.focusRing ? theme.color.primary : theme.color.border)
  let borderWidth = () => props.style?.borderWidth ?? theme.borderWidth.sm
  let borderRadius = () => props.style?.borderRadius ?? theme.radius.sm

  let showPlaceholder = () => !focused() && value().length === 0 && (props.placeholder ?? "").length > 0
  let showCaret = () => focused() && caretOn() && !showPlaceholder()

  // Everything inside the viewport is detached: the value is one d-text shaped
  // at an unbounded width and the caret a d-rect at the measured before-caret
  // width, so typing, caret movement, blink and scroll never touch layout. The
  // viewport carries an explicit height (detached content takes no layout
  // slot) equal to the one-line paragraph height, which keeps the text where
  // the old centered attached row sat. createCaretScroll keeps the caret in
  // view and flushes the offset before paint; scrollX is a paint-time
  // translate that also applies to detached children.
  // All one-line metrics derive from the scaled body size, so the field, the
  // caret, and the scroll math grow together under policy.textScale.
  let fontSize = () => theme.text.body.size * policy.textScale
  let rowHeight = () => Math.round(fontSize() * theme.text.body.lineHeight)
  let caretX = () => measureText(value().slice(0, buffer.caret()), { fontSize: fontSize() }).width
  let scrollX = createCaretScroll(
    () => viewport,
    () => ({
      text: value(),
      fontSize: fontSize(),
      caret: buffer.caret(),
      // Constant, not tied to caret visibility: the caret's footprint does not
      // change as it blinks, so reserving the column only when shown would swing
      // the scroll offset every blink and shift text that exactly fills the box.
      caretWidth: CARET_WIDTH,
    }),
  )

  let textStyle = (color: string) => ({
    w: TEXT_SHAPE_WIDTH,
    fontSize: fontSize(),
    lineHeight: theme.text.body.lineHeight,
    color,
    maxLines: 1,
  })

  return (
    <view
      ref={(n: { id: number }) => (node = n)}
      flexDirection="row"
      alignItems="center"
      paddingLeft={space("md")}
      paddingRight={space("md")}
      paddingTop={space("sm")}
      paddingBottom={space("sm")}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      onPointerDown={handlePointerDown}
      onFocus={handleFocus}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
      onTextInput={handleTextInput}
    >
      <d-rect color={surfaceColor()} radius={borderRadius()} />
      <d-rect
        drawStyle="stroke"
        color={borderColor()}
        strokeWidth={borderWidth()}
        radius={borderRadius()}
      />
      <view
        ref={(n: { id: number }) => (viewport = n)}
        flex={1}
        height={rowHeight()}
        overflow="hidden"
        scrollX={scrollX()}
      >
        {showPlaceholder() ? (
          <d-text {...textStyle(theme.color.textMuted)}>{props.placeholder ?? ""}</d-text>
        ) : (
          <>
            <d-text {...textStyle(textColor())}>{value()}</d-text>
            {showCaret() ? (
              <d-rect
                color={textColor()}
                x={caretX()}
                y={(rowHeight() - fontSize()) / 2}
                w={CARET_WIDTH}
                h={fontSize()}
              />
            ) : null}
          </>
        )}
      </view>
    </view>
  )
}