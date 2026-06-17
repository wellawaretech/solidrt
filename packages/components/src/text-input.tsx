import { createEffect, createSignal, onCleanup } from "@solidjs/signals"
import { setFocus } from "@solidrt/core"
import { createCaretScroll, createTextBuffer } from "@solidrt/core/text-input"
import type { LayoutProps } from "@solidrt/core"
import type { StyleProps } from "./types"
import { theme } from "./theme"

// Caret thickness. Shared so the drawn caret and the scroll offset's reserved
// edge column cannot drift apart.
const CARET_WIDTH = 1

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

  // Style overrides fall back to theme defaults.
  let textColor = () => props.style?.color ?? theme.color.text
  let surfaceColor = () => props.style?.backgroundColor ?? theme.color.surface
  let borderColor = () => props.style?.borderColor ?? theme.color.border
  let borderWidth = () => props.style?.borderWidth ?? theme.borderWidth.sm
  let borderRadius = () => props.style?.borderRadius ?? theme.radius.sm

  let showPlaceholder = () => !focused() && value().length === 0 && (props.placeholder ?? "").length > 0
  let showCaret = () => focused() && caretOn() && !showPlaceholder()

  // The text is split at the caret into two nodes with a zero-size anchor view
  // between them. Flow places the anchor at the caret x (the before-text width),
  // and the caret is a detached d-rect inside it: detached nodes take no layout
  // slot, so the anchor stays zero-width and the after-text is not shifted. The
  // anchor sits at the row's vertical center (alignItems center, zero height),
  // so the caret is offset up by half its height to straddle it. While the
  // placeholder shows, value() is "" so the slices and the scroll offset are 0
  // with no special case. The viewport node is the inner scroll container;
  // createCaretScroll reads its laid-out width after layout, keeps the caret in
  // view, and flushes the offset before paint.
  let beforeCaret = () => value().slice(0, buffer.caret())
  let afterCaret = () => value().slice(buffer.caret())
  let scrollX = createCaretScroll(
    () => viewport,
    () => ({
      text: value(),
      fontSize: theme.text.body.size,
      caret: buffer.caret(),
      // Constant, not tied to caret visibility: the caret's footprint does not
      // change as it blinks, so reserving the column only when shown would swing
      // the scroll offset every blink and shift text that exactly fills the box.
      caretWidth: CARET_WIDTH,
    }),
  )

  let textStyle = (color: string) => ({
    fontSize: theme.text.body.size,
    lineHeight: theme.text.body.lineHeight,
    color,
    maxLines: 1,
    flexShrink: 0,
  })

  return (
    <view
      ref={(n: { id: number }) => (node = n)}
      flexDirection="row"
      alignItems="center"
      paddingLeft={theme.spacing.md}
      paddingRight={theme.spacing.md}
      paddingTop={theme.spacing.sm}
      paddingBottom={theme.spacing.sm}
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
        flexDirection="row"
        alignItems="center"
        overflow="hidden"
        scrollX={scrollX()}
      >
        {showPlaceholder() ? (
          <text {...textStyle(theme.color.textMuted)}>{props.placeholder ?? ""}</text>
        ) : (
          <view flexDirection="row" alignItems="center" flexShrink={0}>
            <text {...textStyle(textColor())}>{beforeCaret()}</text>
            {showCaret() ? (
              <view>
                <d-rect
                  color={textColor()}
                  y={-theme.text.body.size / 2}
                  w={CARET_WIDTH}
                  h={theme.text.body.size}
                />
              </view>
            ) : null}
            <text {...textStyle(textColor())}>{afterCaret()}</text>
          </view>
        )}
      </view>
    </view>
  )
}