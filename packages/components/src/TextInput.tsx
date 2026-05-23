import { createSignal, onCleanup } from "@solidjs/signals"
import { setFocus } from "@solidrt/core"

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

  fontSize?: number
  color?: string
  placeholderColor?: string
  background?: string
  borderColor?: string
  borderWidth?: number
  borderRadius?: number
  caretColor?: string
  padding?: number
  width?: Dimension
  height?: number
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
  let fontSize = () => props.fontSize ?? 14
  let color = () => props.color ?? "black"
  let placeholderColor = () => props.placeholderColor ?? "rgba(0,0,0,0.4)"
  let background = () => props.background ?? "white"
  let borderColor = () => props.borderColor ?? "rgba(0,0,0,0.2)"
  let borderWidth = () => props.borderWidth ?? 1
  let borderRadius = () => props.borderRadius ?? 4
  let caretColor = () => props.caretColor ?? color()
  let padding = () => props.padding ?? 8
  let height = () => props.height ?? 32

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
  let displayColor = () => (showPlaceholder() ? placeholderColor() : color())

  return (
    <view
      ref={(n: { id: number }) => {
        node = n
        if (props.autoFocus) setFocus(n.id)
      }}
      flexDirection="row"
      alignItems="center"
      overflow="hidden"
      height={height()}
      width={props.width}
      paddingLeft={padding()}
      paddingRight={padding()}
      onPointerDown={handlePointerDown}
      onFocus={handleFocus}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
      onTextInput={handleTextInput}
    >
      <d-rect color={background()} radius={borderRadius()} />
      <d-rect
        drawStyle="stroke"
        color={borderColor()}
        strokeWidth={borderWidth()}
        radius={borderRadius()}
      />
      <text fontSize={fontSize()} color={displayColor()} maxLines={1}>
        {displayText()}
      </text>
      {focused() && caretOn() && !showPlaceholder() ? (
        <rect color={caretColor()} w={1} h={fontSize()} />
      ) : null}
    </view>
  )
}