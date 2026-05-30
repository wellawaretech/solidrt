import { createSignal, flush, onCleanup } from "@solidjs/signals"
import { getBoundingBox, measureText, onLayout, setFocus } from "@solidrt/core"
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
  let [internalValue, setInternalValue] = createSignal(props.defaultValue ?? "")
  let [focused, setFocused] = createSignal(false)
  let [caretOn, setCaretOn] = createSignal(true)
  let [viewportWidth, setViewportWidth] = createSignal(0)

  let node: { id: number } | undefined
  let viewport: { id: number } | undefined
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

  // Style overrides fall back to theme defaults.
  let textColor = () => props.style?.color ?? theme.color.text
  let surfaceColor = () => props.style?.backgroundColor ?? theme.color.surface
  let borderColor = () => props.style?.borderColor ?? theme.color.border
  let borderWidth = () => props.style?.borderWidth ?? theme.borderWidth.sm
  let borderRadius = () => props.style?.borderRadius ?? theme.radius.sm

  let showPlaceholder = () => !focused() && value().length === 0 && (props.placeholder ?? "").length > 0
  let displayText = () => (showPlaceholder() ? (props.placeholder ?? "") : value())
  let displayColor = () => (showPlaceholder() ? theme.color.textMuted : textColor())

  // Viewport width is the inner scroll container's own laid-out width, read
  // after layout. This works for numeric, "auto" and "%" widths alike (no need
  // to subtract padding: the padding lives on the outer view). getBoundingBox
  // is a snapshot, so we push it into a signal from onLayout. The flush drains
  // the resulting scrollX update synchronously, before paint, so the scroll
  // tracks the width in the same frame instead of trailing it by one.
  onLayout(() => {
    if (!viewport) return
    let w = getBoundingBox(viewport)?.width ?? 0
    if (w !== viewportWidth()) setViewportWidth(w)
    flush()
  })

  let caretWidth = () => (focused() && !showPlaceholder() ? 1 : 0)
  let scrollX = () => {
    if (showPlaceholder()) return 0
    let tw = measureText(value(), { fontSize: theme.text.body.size }).width
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