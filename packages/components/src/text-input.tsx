import { untrack } from "@solidrt/core"
import { createTextBuffer } from "@solidrt/core/text-input"
import type { TextInputHints } from "@solidrt/core"
import { EditorField } from "./editor-field"
import type { EditorLayoutProps, StyleProps, TransitionProps } from "./types"
import { theme } from "./theme"

export interface TextInputProps extends TransitionProps {
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
  /**
   * Multi-line editing: lines wrap at the field's width, Enter inserts a
   * newline (onSubmit never fires), Up/Down move by line. Unconstrained,
   * the field grows with its content (up to `maxRows` rows, then scrolls);
   * sized by its layout - a `height`, `flexGrow: 1` in a sized parent, a
   * parent too small for the content - it is a fixed box that scrolls to
   * the caret.
   */
  multiline?: boolean
  /** Multiline, unconstrained: rows to grow to before scrolling. Default unbounded. */
  maxRows?: number
  /**
   * IME behavior for the field's text sessions (keyboard type,
   * capitalization, autocorrect). Identifier-like fields want
   * `{ capitalize: "none", autocorrect: false }`.
   */
  hints?: TextInputHints

  ref?: (node: { id: number }) => void
  /**
   * The box, plus the font fields (`fontSize`, `fontFamily`, `lineHeight`,
   * `fontStyle`, `fontWeight`) the text is shaped in, as on Text: the theme
   * body font by default, `fontSize` scaled by `policy.textScale`. The rows,
   * caret and scrolling follow the font.
   */
  layout?: EditorLayoutProps
  style?: StyleProps
}

// A plain string field over the shared EditorField shell: the buffer is
// core's createTextBuffer and every line is one d-text of the value's slice.
// Detached text takes its width from the box it is drawn in; +1 so the ink
// width rounding never wraps a line's own text.
export function TextInput(props: TextInputProps) {
  let value = (): string => ""
  return (
    <EditorField
      transition={props.transition}
      onTransitionEnd={props.onTransitionEnd}
      buffer={(step) => {
        let buffer = createTextBuffer({
          value: () => props.value,
          // A one-shot initial value by contract (see createTextBuffer): read
          // once, deliberately untracked.
          defaultValue: untrack(() => props.defaultValue),
          onInput: (v) => props.onInput?.(v),
          maxLength: () => props.maxLength,
          step,
        })
        value = buffer.value
        return buffer
      }}
      renderLine={({ line, font, color }) => (
        <d-text y={line().y} w={line().width + 1} {...font()} color={color()} maxLines={1}>
          {value().slice(line().start, line().end)}
        </d-text>
      )}
      onSubmit={props.onSubmit}
      onFocus={props.onFocus}
      onBlur={props.onBlur}
      placeholder={props.placeholder}
      disabled={props.disabled}
      autoFocus={props.autoFocus}
      multiline={props.multiline}
      maxRows={props.maxRows}
      hints={props.hints}
      ref={props.ref}
      layout={props.layout}
      style={{ ...theme.components.textInput, ...props.style }}
    />
  )
}
