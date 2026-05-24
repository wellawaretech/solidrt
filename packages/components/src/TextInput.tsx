import { createSignal, onCleanup } from "@solidjs/signals"
import { measureText, setFocus } from "@solidrt/core"
import { theme } from "./theme"

type Dimension = number | "auto" | `${number}%`

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

  width?: Dimension
}

// V1: single-line, caret-at-end only, no selection, no mid-string editing.
// Printable text arrives via onTextInput (post-IME commit). onKeyDown handles
// Backspace, Enter, Escape. Outside-click-to-blur is the caller's job.
export function TextInput(props: TextInputProps) {
  let [internalValue, setInternalValue] = createSignal(props.defaultValue ?? "")
  let [focused, setFocused] = createSignal(false)
  let [caretOn, setCaretOn] = createSignal(true)

  let node: { id: number } | undefined
  let blinkId: any = null

  let value = () => props.value ?? internalValue()

  let commit = (next: string) => {
    if (props.maxLength != null && next.length > props.maxLength) {
      next = next.slice(0, props.maxLength)
    }
    if (props.value == null) setInternalValue(next)
    props.onInput?.(next)
  }

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
      let v = value()
      if (v.length > 0) commit(v.slice(0, -1))
      setCaretOn(true)
    } else if (e.key === "Return" || e.key === "Enter") {
      props.onSubmit?.(value())
    } else if (e.key === "Escape") {
      if (node) setFocus(null)
    }
  }

  let handleTextInput = (e: any) => {
    if (props.disabled) return
    commit(value() + (e.text ?? ""))
    setCaretOn(true)
  }

  onCleanup(() => {
    if (blinkId != null) clearInterval(blinkId)
  })

  let showPlaceholder = () => !focused() && value().length === 0 && (props.placeholder ?? "").length > 0
  let displayText = () => (showPlaceholder() ? (props.placeholder ?? "") : value())
  let displayColor = () => (showPlaceholder() ? theme.color.textMuted : theme.color.text)

  // V1: viewport width derived from numeric props.width minus padding.
  // For "auto" / "%" widths, fall back to 0 (no scroll, caret may overflow).
  let viewportWidth = () => (typeof props.width === "number" ? props.width - 2 * theme.spacing.md : 0)
  let caretWidth = () => (focused() && !showPlaceholder() ? 1 : 0)
  let scrollX = () => {
    if (showPlaceholder()) return 0
    let tw = measureText(value(), { fontSize: theme.fontSize.body }).width
    let vw = viewportWidth()
    if (vw <= 0) return 0
    return Math.max(0, tw + caretWidth() - vw)
  }

  return (
    <view
      ref={(n: { id: number }) => {
        node = n
        if (props.autoFocus) setFocus(n.id)
      }}
      flexDirection="row"
      alignItems="center"
      width={props.width}
      paddingLeft={theme.spacing.md}
      paddingRight={theme.spacing.md}
      paddingTop={theme.spacing.md}
      paddingBottom={theme.spacing.md}
      onPointerDown={handlePointerDown}
      onFocus={handleFocus}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
      onTextInput={handleTextInput}
    >
      <d-rect color={theme.color.surface} radius={theme.radius.sm} />
      <d-rect
        drawStyle="stroke"
        color={theme.color.border}
        strokeWidth={theme.borderWidth.sm}
        radius={theme.radius.sm}
      />
      <view
        flex={1}
        flexDirection="row"
        alignItems="center"
        overflow="hidden"
        scrollX={scrollX()}
      >
        <text fontSize={theme.fontSize.body} color={displayColor()} maxLines={1} flexShrink={0}>
          {displayText()}
        </text>
        {focused() && caretOn() && !showPlaceholder() ? (
          <rect color={theme.color.text} w={1} h={theme.fontSize.body} flexShrink={0} />
        ) : null}
      </view>
    </view>
  )
}