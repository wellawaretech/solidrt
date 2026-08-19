import { untrack } from "@solidrt/core"
import type { LayoutProps, TextInputHints } from "@solidrt/core"
import type { TextRunRange } from "flux:rendertree"
import { EditorField } from "./editor-field"
import { createDocumentBuffer, type Attributes, type Document, type DocumentBuffer } from "./rich-text-document"
import type { StyleProps } from "./types"
import { theme } from "./theme"
import { policy } from "./policy"

export interface RichTextEditorProps {
  value?: Document
  defaultValue?: Document
  onInput?: (value: Document) => void
  /**
   * Receives the editor's document buffer, the formatting API: `format`,
   * `formatBlock`, `insertAtom`, `attributes` (for toolbar state), plus the
   * text buffer's selection and edit methods. The app renders its own
   * controls around the editor and calls these.
   */
  editorRef?: (editor: DocumentBuffer) => void
  onFocus?: () => void
  onBlur?: () => void

  placeholder?: string
  disabled?: boolean
  autoFocus?: boolean
  /** Without a `layout.height`: rows to grow to before scrolling. Default unbounded. */
  maxRows?: number
  hints?: TextInputHints

  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
}

// The attributes the editor draws (anything else is carried in the document
// and ignored here). Inline: bold, italic, underline, code (mono), color,
// link (a URL string: primary color, underlined). Block: heading 1-3.
// Font-affecting ones also feed the geometry (prepareText runs), so the
// caret and wrapping follow the drawn glyphs.
type Font = Pick<TextRunRange, "fontFamily" | "fontSize" | "fontStyle" | "fontWeight">

function fontOf(inline: Attributes, block: Attributes, base: number): Font {
  let font: Font = {}
  let heading = block.heading
  if (heading === 1) font.fontSize = theme.text.heading.size * policy.textScale
  else if (heading === 2) font.fontSize = theme.text.title.size * policy.textScale
  else if (heading === 3) font.fontSize = base
  if (heading === 1 || heading === 2 || heading === 3 || inline.bold) font.fontWeight = 700
  if (inline.italic) font.fontStyle = "italic"
  if (inline.code) font.fontFamily = "mono"
  return font
}

// A document as style intervals: the document runs cut at paragraph
// boundaries, each with the paragraph's block attributes alongside.
type Interval = { start: number; end: number; inline: Attributes; block: Attributes }

function intervals(doc: Document): Interval[] {
  let out: Interval[] = []
  let paragraph = 0
  for (let run of doc.runs) {
    let at = run.start
    // Runs are clamped to the text: a malformed document draws what it can.
    let runEnd = Math.min(run.end, doc.text.length)
    while (at < runEnd) {
      let next = doc.text.indexOf("\n", at)
      let paragraphEnd = next < 0 ? doc.text.length : next + 1
      let end = Math.min(runEnd, paragraphEnd)
      out.push({ start: at, end, inline: run.attributes, block: doc.blocks[paragraph] ?? {} })
      if (end === paragraphEnd && next >= 0) paragraph++
      at = end
    }
  }
  return out
}

/**
 * Edits a rich text {@link Document} (styled runs, paragraph attributes,
 * inline atoms as U+FFFC) in the TextInput field: same focus, caret, keys,
 * wrapping and scrolling, always multiline. Formatting is driven through
 * `editorRef` (the document buffer), not a built-in toolbar. Atoms are drawn
 * as their placeholder character for now.
 */
export function RichTextEditor(props: RichTextEditorProps) {
  let editor!: DocumentBuffer
  let base = () => theme.text.body.size * policy.textScale
  let doc = () => editor.document()

  // Geometry runs: every interval whose font differs from the base.
  let runs = (): TextRunRange[] => {
    let size = base()
    let out: TextRunRange[] = []
    for (let { start, end, inline, block } of intervals(doc())) {
      let font = fontOf(inline, block, size)
      if (Object.keys(font).length === 0) continue
      out.push({ start, end, ...font })
    }
    return out
  }

  // Only set props are passed: an explicit undefined would reach the tree.
  let spanProps = (i: Interval): Record<string, unknown> => {
    let out: Record<string, unknown> = fontOf(i.inline, i.block, base())
    let link = typeof i.inline.link === "string"
    if (typeof i.inline.color === "string") out.color = i.inline.color
    else if (link) out.color = theme.color.primary
    if (i.inline.underline || link) out.textDecoration = "underline"
    return out
  }

  return (
    <EditorField
      buffer={(step) => {
        editor = createDocumentBuffer({
          value: () => props.value,
          defaultValue: untrack(() => props.defaultValue),
          onInput: (d) => props.onInput?.(d),
          step,
        })
        props.editorRef?.(editor)
        return editor
      }}
      runs={runs}
      renderLine={({ line, font, color }) => (
        <d-text y={line().y} w={line().width + 1} {...font()} color={color()} maxLines={1}>
          {intervals(doc())
            .filter((i) => i.end > line().start && i.start < line().end)
            .map((i) => (
              <span {...spanProps(i)}>
                {doc().text.slice(Math.max(i.start, line().start), Math.min(i.end, line().end))}
              </span>
            ))}
        </d-text>
      )}
      onFocus={props.onFocus}
      onBlur={props.onBlur}
      placeholder={props.placeholder}
      disabled={props.disabled}
      autoFocus={props.autoFocus}
      multiline
      maxRows={props.maxRows}
      hints={props.hints}
      ref={props.ref}
      layout={props.layout}
      style={props.style}
    />
  )
}
