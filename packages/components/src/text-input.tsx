import { createSignal, onCleanup } from "@solidjs/signals"
import { setFocus } from "@solidrt/core"
import { createCaretScroll, createTextBuffer } from "@solidrt/core/text-input"
import type { LayoutProps } from "@solidrt/core"
import type { StyleProps } from "./types"
import { theme } from "./theme"

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

// V1: single-line, caret-at-end only, no selection, no mid-string editing.
// Printable text arrives via onTextInput (post-IME commit). onKeyDown handles
// Backspace, Enter, Escape. Outside-click-to-blur is the caller's job.
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
    } else if (e.key === "Return" || e.key === "Enter") {
      props.onSubmit?.(value())
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
  let displayText = () => (showPlaceholder() ? (props.placeholder ?? "") : value())
  let displayColor = () => (showPlaceholder() ? theme.color.textMuted : textColor())

  // Reserve a caret column at the end while editing so it stays in view. While
  // the placeholder shows, value() is "" so the offset is 0 with no special
  // case. The viewport node is the inner scroll container; createCaretScroll
  // reads its laid-out width after layout and flushes the offset before paint.
  let caretWidth = () => (focused() && !showPlaceholder() ? 1 : 0)
  let scrollX = createCaretScroll(
    () => viewport,
    () => ({ text: value(), fontSize: theme.text.body.size, caretWidth: caretWidth() }),
  )

  return (
    <view
      ref={(n: { id: number }) => {
        node = n
        if (props.autoFocus) setFocus(n.id)
      }}
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
        <text
          fontSize={theme.text.body.size}
          lineHeight={theme.text.body.lineHeight}
          color={displayColor()}
          maxLines={1}
          flexShrink={0}
        >
          {displayText()}
        </text>
        {focused() && caretOn() && !showPlaceholder() ? (
          <rect color={theme.color.text} w={1} h={theme.text.body.size} flexShrink={0} />
        ) : null}
      </view>
    </view>
  )
}