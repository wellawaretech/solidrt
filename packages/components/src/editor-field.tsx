// The editable field shared by TextInput and the rich text editor: a
// focusable box that edits a text buffer through the keyboard and text
// session, lays its lines out with core's createTextEditorLayout, keeps the
// caret in view and draws the caret; how a line's text is drawn and what the
// buffer holds are the caller's (renderLine, buffer). Internal to the
// package.
import {
  For,
  arena,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  focusedNode,
  setFocus,
  startTextInput,
  textInputActive,
  untrack,
} from "@solidrt/core"
import { createTextEditorLayout } from "@solidrt/core/text-input"
import type { EditorLine, TextBuffer } from "@solidrt/core/text-input"
import type { Color, Gradient, KeyEvent, LayoutProps, PointerEvent, TextInputHints } from "@solidrt/core"
import type { Element } from "solid-js"
import type { MeasureTextOptions, TextRunRange } from "flux:rendertree"
import { registerNavAction } from "./focus-nav"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"

// Caret thickness. Shared so the drawn caret and the scroll offset's reserved
// edge column cannot drift apart.
const CARET_WIDTH = 1

// Shaping width of the placeholder: effectively unbounded, so it never wraps;
// the viewport clips it.
const PLACEHOLDER_SHAPE_WIDTH = 1e9

/** What the shell hands renderLine for one laid-out line. */
export type LineRender = {
  line: () => EditorLine
  /** The field's font options (size, line height), the base for the line's text. */
  font: () => MeasureTextOptions
  /** The field's text color. */
  color: () => Color | Gradient
}

export interface EditorFieldProps extends TransitionProps {
  /**
   * Creates the buffer the field edits, given the grapheme `step` from the
   * field's geometry (createTextEditorLayout.step); called once.
   */
  buffer: (step: (text: string, offset: number, direction: "left" | "right") => number) => TextBuffer
  /** Styled ranges over the text for the geometry (prepareText `runs`). */
  runs?: () => TextRunRange[] | undefined
  /** Draws one line: detached content at the line's y inside the viewport. */
  renderLine: (r: LineRender) => Element

  onSubmit?: (value: string) => void
  onFocus?: () => void
  onBlur?: () => void
  placeholder?: string
  disabled?: boolean
  autoFocus?: boolean
  multiline?: boolean
  maxRows?: number
  hints?: TextInputHints
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
}

// The caret moves through the text (Left/Right/Home/End, Up/Down by line when
// multiline), edits happen at the caret, and the inner box scrolls to keep it
// in view. Printable text arrives via onTextInput (post-IME commit).
// onKeyDown handles caret movement, Backspace/Delete, Enter/select, Escape -
// and stops those keys from bubbling further. Focused and editing are
// distinct (see activateField): navigation focuses, select begins editing,
// Enter while editing submits (single-line) or inserts a newline
// (multiline). A tap puts the caret at the nearest position. Range selection:
// Shift+movement (and Shift+tap) extends from the anchor, a mouse/pen drag
// selects (stealing the gesture arena on its first move), Ctrl/Cmd+A selects
// all; the highlight draws behind the lines while focused, and edits on a
// range replace it (the buffer's own behavior). Outside-click-to-blur is the
// caller's job.
export function EditorField(props: EditorFieldProps) {
  let [caretOn, setCaretOn] = createSignal(true)

  let node: { id: number } | undefined
  let viewport: { id: number } | undefined
  let blinkId: any = null

  // Derived from core's reactive focus (setFocus is the only writer); the
  // onFocus/onBlur handlers below keep only their side effects (blink timer,
  // caller callbacks). focusedNode() is read FIRST, unconditionally: the
  // memo may first compute before the ref has set `node`, and
  // short-circuiting past the read would leave it dependency-free, frozen
  // false forever.
  let focused = createMemo(() => {
    let id = focusedNode()
    return id != null && id === node?.id
  })

  // Grapheme steps from the editor's caret stops; the editor is created
  // below and only consulted from event handlers, after both exist. The
  // factory is a one-shot by contract: read once, deliberately untracked.
  let buffer = untrack(() => props.buffer)((_text, offset, direction) => editor.step(offset, direction))
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

  // The viewport's local point plus its scroll is a content point; the
  // nearest caret stop on the line under it. Locals are exact even on the
  // frozen down path, so a drag keeps mapping after the pointer leaves the
  // field.
  let offsetAt = (e: PointerEvent) => {
    let line = editor.lineAtY(e.localY + editor.scrollY())
    return editor.offsetAtX(line, e.localX + editor.scrollX())
  }

  // Drag-select: a mouse/pen down on the viewport arms the pointer, and its
  // first move steals the gesture arena - precise pointers select text over
  // an enclosing scroller's pan, as desktop editors do. Touch never arms, so
  // a finger drag still scrolls (touch range selection is a later,
  // handle-based interaction); Shift+tap extends everywhere.
  let dragArmed: number | null = null
  let dragActive: number | null = null
  let dragOwner = {
    cancel: () => {
      dragActive = null
    },
  }

  // Tap-to-position: the offset under the tap takes the caret (Shift keeps
  // the anchor to extend). Runs before the field's own handler above
  // (bubbling), which focuses.
  let handleViewportPointerDown = (e: PointerEvent) => {
    if (props.disabled) return
    let offset = offsetAt(e)
    buffer.setSelection(e.shiftKey ? buffer.selection().anchor : offset, offset)
    if (e.pointerType !== "touch" && (e.button == null || e.button === 0)) dragArmed = e.pointerId
    setCaretOn(true)
  }

  let handleViewportPointerMove = (e: PointerEvent) => {
    if (props.disabled) return
    if (dragArmed === e.pointerId) {
      dragArmed = null
      if (arena.steal(e.pointerId, dragOwner)) dragActive = e.pointerId
    }
    if (dragActive === e.pointerId) {
      buffer.setSelection(buffer.selection().anchor, offsetAt(e))
      setCaretOn(true)
    }
  }

  let handleViewportPointerUp = (e: PointerEvent) => {
    if (dragArmed === e.pointerId) dragArmed = null
    if (dragActive === e.pointerId) {
      arena.release(e.pointerId, dragOwner)
      dragActive = null
    }
  }

  let handleFocus = () => {
    setCaretOn(true)
    if (blinkId == null) {
      blinkId = setInterval(() => setCaretOn((v) => !v), 500)
    }
    props.onFocus?.()
  }

  let handleBlur = () => {
    if (blinkId != null) {
      clearInterval(blinkId)
      blinkId = null
    }
    props.onBlur?.()
  }

  // Keys the input consumes stop propagating: an ancestor (or an app-global
  // shortcut on the window) must not also act on an ArrowLeft that moved the
  // caret. Anything else (e.g. ctrl+s) bubbles on.
  let handleKeyDown = (e: KeyEvent) => {
    if (props.disabled) return
    let consumed = true
    if (e.key === "Backspace") {
      buffer.deleteBackward()
      setCaretOn(true)
    } else if (e.key === "Delete") {
      buffer.deleteForward()
      setCaretOn(true)
    } else if (e.key === "ArrowLeft") {
      buffer.move("left", { extend: e.shiftKey })
      setCaretOn(true)
    } else if (e.key === "ArrowRight") {
      buffer.move("right", { extend: e.shiftKey })
      setCaretOn(true)
    } else if (e.key === "Home" || e.key === "End") {
      // Multiline: the current line's ends (offsetAtX at 0 / far right, so a
      // wrap boundary resolves to the position that shows on this line).
      if (props.multiline) {
        let offset = editor.offsetAtX(editor.caretLine(), e.key === "Home" ? 0 : 1e9)
        buffer.setSelection(e.shiftKey ? buffer.selection().anchor : offset, offset)
      } else {
        buffer.move(e.key === "Home" ? "start" : "end", { extend: e.shiftKey })
      }
      setCaretOn(true)
    } else if (props.multiline && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      moveLine(e.key === "ArrowUp" ? -1 : 1, e.shiftKey)
      setCaretOn(true)
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      buffer.setSelection(0, value().length)
      setCaretOn(true)
    } else if (props.multiline && e.key === "Enter" && textInputActive()) {
      buffer.insertText("\n")
      setCaretOn(true)
    } else if (e.key === "Enter" || e.code === "Select") {
      // The remote center key's `key` is "Unidentified"; match its code.
      activateField()
    } else if (e.key === "Escape") {
      if (node) setFocus(null)
    } else {
      consumed = false
    }
    if (consumed) e.stopPropagation()
  }

  let handleTextInput = (e: any) => {
    if (props.disabled) return
    buffer.insertText(e.text ?? "")
    setCaretOn(true)
  }

  // Up/Down: the offset on the neighbouring line nearest the caret's x; on
  // the first/last line they go to the text's start/end, as editors do.
  // `extend` (Shift held) keeps the anchor to grow a selection.
  let moveLine = (delta: number, extend: boolean) => {
    let target = editor.caretLine() + delta
    let count = editor.lines().length
    let offset =
      target < 0 ? 0 : target >= count ? value().length : editor.offsetAtX(target, editor.caret().x)
    buffer.setSelection(extend ? buffer.selection().anchor : offset, offset)
  }

  // Select on the focused field: focused and editing are distinct states. A
  // field reached by navigation is focused but has no text session yet -
  // select begins one (raising the on-screen keyboard where used, e.g. a TV
  // with no keyboard attached); while editing, it submits (a multiline field
  // has no submit: Enter inserts a newline and select is left to bubble to
  // the caller). On platforms where the session starts invisibly at focus
  // (desktop, physical keyboard) the first branch never runs and Enter
  // submits as always. Registered as the nav action too, for a controller's
  // south button.
  let activateField = () => {
    if (props.disabled) return
    if (!textInputActive()) {
      startTextInput()
    } else if (!props.multiline) {
      props.onSubmit?.(value())
      setFocus(null)
    }
  }

  let unregisterNav: (() => void) | null = null

  onCleanup(() => {
    if (blinkId != null) clearInterval(blinkId)
    unregisterNav?.()
    // An unmount mid-drag must not leave a resolved arena claim behind.
    if (dragActive != null) arena.release(dragActive, dragOwner)
  })

  // Style overrides fall back to theme defaults. The border doubles as the
  // focus ring (ring color at the focus width) while focused, when the
  // focus-ring policy asks for a visible indicator.
  let textColor = () => props.style?.color ?? theme.color.text
  let surfaceColor = () => props.style?.backgroundColor ?? theme.color.surface
  let ring = () => focused() && policy.focusRing
  let borderColor = () => props.style?.borderColor ?? (ring() ? theme.color.ring : theme.color.border)
  let borderWidth = () => props.style?.borderWidth ?? (ring() ? theme.borderWidth.focus : theme.borderWidth.sm)
  let borderRadius = () => props.style?.borderRadius ?? theme.radius.md

  let showPlaceholder = () => !focused() && value().length === 0 && (props.placeholder ?? "").length > 0
  let showCaret = () => focused() && caretOn() && !showPlaceholder()

  // Everything inside the viewport is detached: the value is drawn per
  // laid-out line by renderLine (createTextEditorLayout breaks the lines from
  // the prepared text, at the viewport width when multiline) and the caret is
  // a d-rect at the measured before-caret width on its line, so typing, caret
  // movement, blink and scroll never touch layout. The single-line viewport
  // carries an explicit height (detached content takes no layout slot) equal
  // to the one-line height; a multiline viewport stretches to the field's
  // height. The editor layout keeps the caret in view and flushes the offsets
  // before paint; scrollX/scrollY are paint-time translates that also apply
  // to detached children.
  // All metrics derive from the scaled body size, so the field, the caret,
  // and the scroll math grow together under policy.textScale.
  let fontSize = () => theme.text.body.size * policy.textScale
  let font = () => ({ fontSize: fontSize(), lineHeight: theme.text.body.lineHeight })
  let rowHeight = () => Math.round(fontSize() * theme.text.body.lineHeight)
  let editor = createTextEditorLayout(
    () => viewport,
    () => ({
      text: value(),
      font: font(),
      runs: props.runs?.(),
      caret: buffer.caret(),
      // Constant, not tied to caret visibility: the caret's footprint does not
      // change as it blinks, so reserving the column only when shown would swing
      // the scroll offset every blink and shift text that exactly fills the box.
      caretWidth: CARET_WIDTH,
      wrap: props.multiline ?? false,
    }),
  )
  let caret = editor.caret

  // Multiline viewport height: a caller-given field height stretches the
  // viewport (fixed box, scrolls); otherwise the content height, at least one
  // row and at most maxRows rows. Single-line is always one row.
  let viewportHeight = (): number | undefined => {
    if (!props.multiline) return rowHeight()
    if (props.layout?.height != null) return undefined
    let lines = editor.lines()
    let last = lines[lines.length - 1]!
    let content = Math.ceil(last.y + last.height)
    let max = props.maxRows != null ? props.maxRows * rowHeight() : Infinity
    return Math.max(rowHeight(), Math.min(content, max))
  }

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => {
        node = n
        unregisterNav?.()
        unregisterNav = registerNavAction(n.id, activateField)
        props.ref?.(n)
      }}
      textInputHints={props.multiline ? { multiline: true, ...props.hints } : props.hints}
      focusable
      flexDirection="row"
      alignItems="center"
      paddingLeft={space("md")}
      paddingRight={space("md")}
      paddingTop={space("md")}
      paddingBottom={space("md")}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      opacity={props.style?.opacity}
      onPointerDown={handlePointerDown}
      onFocus={handleFocus}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
      onTextInput={handleTextInput}
    >
      <d-rect transition={split().background} onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)} color={surfaceColor()} radius={borderRadius()} />
      <d-rect
        drawStyle="stroke"
        transition={split().border}
        onTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
        color={borderColor()}
        strokeWidth={borderWidth()}
        radius={borderRadius()}
      />
      <view
        ref={(n: { id: number }) => (viewport = n)}
        flex={1}
        height={viewportHeight()}
        alignSelf={props.multiline ? "stretch" : undefined}
        overflow="hidden"
        scrollX={editor.scrollX()}
        scrollY={editor.scrollY()}
        onPointerDown={handleViewportPointerDown}
        onPointerMove={handleViewportPointerMove}
        onPointerUp={handleViewportPointerUp}
      >
        {showPlaceholder() ? (
          <d-text w={PLACEHOLDER_SHAPE_WIDTH} {...font()} color={theme.color.textMuted} maxLines={1}>
            {props.placeholder ?? ""}
          </d-text>
        ) : (
          <>
            {focused() ? (
              <For each={editor.selectionRects(buffer.selection().anchor, buffer.selection().focus)} keyed={false}>
                {(r) => <d-rect color={theme.color.selection} x={r().x} y={r().y} w={r().width} h={r().height} />}
              </For>
            ) : null}
            <For each={editor.lines()} keyed={false}>
              {(line) => props.renderLine({ line, font, color: textColor })}
            </For>
            {showCaret() ? (
              <d-rect
                color={textColor()}
                x={caret().x}
                y={caret().y + (caret().height - fontSize()) / 2}
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
