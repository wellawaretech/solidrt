// Headless rich-text document buffer: the value model of a rich text editor
// (okf/backlog/rich-text-editor.md) over the same string offsets the text
// buffer and the editor geometry use. Flat, Delta-like: one text with "\n"
// between paragraphs, attributed runs tiling it, one attribute set per
// paragraph. Attributes are opaque key/values here; what {bold: true} or an
// atom's {atom: "image"} mean is the component's business.

import { createSignal, flush } from "@solidjs/signals"
import { createTextBuffer, type TextBuffer, type TextBufferOptions } from "@solidrt/core/text-input"

/** Formatting as opaque key/values, compared with `===`. */
export type Attributes = Record<string, string | number | boolean>
/** A change to apply: a `null` value removes the key. */
export type AttributePatch = Record<string, string | number | boolean | null>

/** An attributed range of the document text; runs tile the text. */
export type DocumentRun = { start: number; end: number; attributes: Attributes }

/**
 * A rich text value. `text` is the plain text (what a caret moves over):
 * "\n" separates paragraphs, an inline atom is one U+FFFC ({@link ATOM}).
 * `runs` tile the text (none when it is empty) with no two neighbours
 * carrying equal attributes; `blocks` holds one attribute set per paragraph
 * (`\n` count + 1). Plain data: build one with {@link plainDocument}, or
 * literally.
 */
export type Document = { text: string; runs: DocumentRun[]; blocks: Attributes[] }

/** The character standing in for an inline atom (object replacement character). */
export const ATOM = "\uFFFC"

/** A document of `text` with no formatting. */
export function plainDocument(text = ""): Document {
  return {
    text,
    runs: text ? [{ start: 0, end: text.length, attributes: {} }] : [],
    blocks: Array.from({ length: countNewlines(text, 0, text.length) + 1 }, () => ({})),
  }
}

export type DocumentBufferOptions = {
  /** Controlled document accessor; edits then flow out through onInput only. */
  value?: () => Document | undefined
  /** Initial document when uncontrolled. */
  defaultValue?: Document
  /** Called with the new document after every edit. */
  onInput?: (document: Document) => void
  maxLength?: TextBufferOptions["maxLength"]
  step?: TextBufferOptions["step"]
}

export type DocumentBuffer = TextBuffer & {
  /** The current document (`value()` is its text). */
  document(): Document
  /**
   * Inline attributes at the caret: the pending typing attributes if
   * {@link format} was called on a collapsed selection, else those of the
   * character before the caret (the ones typed text will take).
   */
  attributes(): Attributes
  /**
   * Set or remove (`null`) inline attributes on the selection. On a
   * collapsed selection they become the typing attributes of the next
   * insert instead, dropped when the caret moves.
   */
  format(patch: AttributePatch): void
  /** Set or remove (`null`) block attributes on the paragraphs the selection touches. */
  formatBlock(patch: AttributePatch): void
  /** Replace the selection with an inline atom carrying `attributes`. */
  insertAtom(attributes: Attributes): void
  /** Replace the whole document, caret to the end. */
  setDocument(next: Document): void
}

/**
 * An editable rich text document with the contract of createTextBuffer
 * (controlled/uncontrolled, buffer-owned selection, grapheme `step`,
 * synchronous commits) plus formatting. Inserted text takes the inline
 * attributes of the character before the caret; a "\n" splits its paragraph
 * into two with the same block attributes, deleting one merges (the first
 * paragraph's attributes win).
 */
export function createDocumentBuffer(options: DocumentBufferOptions = {}): DocumentBuffer {
  let [internal, setInternal] = createSignal<Document>(options.defaultValue ?? plainDocument())
  let current = () => options.value?.() ?? internal()
  // Typing attributes set on a collapsed selection, valid while the caret
  // stays where they were set.
  let [pending, setPending] = createSignal<{ at: number; attributes: Attributes } | null>(null)

  let commit = (next: Document) => {
    if (options.value?.() == null) setInternal(next)
    options.onInput?.(next)
  }

  let inherited = (offset: number): Attributes => {
    let runs = current().runs
    let at = offset > 0 ? offset - 1 : 0
    return runs.find((r) => r.start <= at && at < r.end)?.attributes ?? {}
  }

  let text = createTextBuffer({
    value: () => current().text,
    maxLength: options.maxLength,
    step: options.step,
    onReplace: (start, end, inserted) => {
      let doc = current()
      let typing = pending()
      let attributes = typing && typing.at === start && start === end ? typing.attributes : inherited(start)
      setPending(null)
      let paragraph = countNewlines(doc.text, 0, start)
      let removed = countNewlines(doc.text, start, end)
      let added = countNewlines(inserted, 0, inserted.length)
      let block = doc.blocks[paragraph] ?? {}
      commit({
        text: doc.text.slice(0, start) + inserted + doc.text.slice(end),
        runs: spliceRuns(doc.runs, start, end, inserted.length, attributes),
        blocks: [
          ...doc.blocks.slice(0, paragraph),
          ...Array.from({ length: added + 1 }, () => block),
          ...doc.blocks.slice(paragraph + removed + 1),
        ],
      })
    },
  })

  let range = (): [number, number] => {
    let { anchor, focus } = text.selection()
    return anchor <= focus ? [anchor, focus] : [focus, anchor]
  }

  let attributes = (): Attributes => {
    let typing = pending()
    let caret = text.caret()
    return typing && typing.at === caret ? typing.attributes : inherited(caret)
  }

  return {
    ...text,
    move: (direction, opts) => {
      setPending(null)
      text.move(direction, opts)
    },
    setSelection: (anchor, focus) => {
      setPending(null)
      text.setSelection(anchor, focus)
    },
    document: current,
    attributes,
    format: (patch) => {
      let [start, end] = range()
      if (start === end) {
        setPending({ at: start, attributes: patched(attributes(), patch) })
        flush()
        return
      }
      let doc = current()
      commit({ ...doc, runs: formatRuns(doc.runs, start, end, patch) })
      flush()
    },
    formatBlock: (patch) => {
      let [start, end] = range()
      let doc = current()
      let first = countNewlines(doc.text, 0, start)
      let last = countNewlines(doc.text, 0, end)
      commit({ ...doc, blocks: doc.blocks.map((b, i) => (first <= i && i <= last ? patched(b, patch) : b)) })
      flush()
    },
    insertAtom: (attributes) => {
      let [start, end] = range()
      if (start !== end) text.insertText("")
      setPending({ at: start, attributes })
      flush()
      text.insertText(ATOM)
    },
    setDocument: (next) => {
      commit(next)
      text.setSelection(next.text.length, next.text.length)
    },
  }
}

function countNewlines(text: string, start: number, end: number): number {
  let n = 0
  for (let i = start; i < end; i++) if (text.charCodeAt(i) === 10) n++
  return n
}

function sameAttributes(a: Attributes, b: Attributes): boolean {
  let keys = Object.keys(a)
  return keys.length === Object.keys(b).length && keys.every((k) => a[k] === b[k])
}

function patched(attributes: Attributes, patch: AttributePatch): Attributes {
  let out: Attributes = { ...attributes }
  for (let key of Object.keys(patch)) {
    let value = patch[key]
    if (value == null) delete out[key]
    else out[key] = value
  }
  return out
}

// Drop empty runs and merge equal neighbours: the tiling invariant.
function normalize(runs: DocumentRun[]): DocumentRun[] {
  let out: DocumentRun[] = []
  for (let run of runs) {
    if (run.end <= run.start) continue
    let last = out[out.length - 1]
    if (last && sameAttributes(last.attributes, run.attributes)) last.end = run.end
    else out.push({ ...run })
  }
  return out
}

// Runs after replacing [start, end) by `length` characters in `attributes`.
function spliceRuns(runs: DocumentRun[], start: number, end: number, length: number, attributes: Attributes): DocumentRun[] {
  let delta = length - (end - start)
  let out: DocumentRun[] = []
  let inserted = false
  for (let run of runs) {
    if (run.start < start) out.push({ start: run.start, end: Math.min(run.end, start), attributes: run.attributes })
    if (!inserted && run.end >= start) {
      out.push({ start, end: start + length, attributes })
      inserted = true
    }
    if (run.end > end) out.push({ start: Math.max(run.start, end) + delta, end: run.end + delta, attributes: run.attributes })
  }
  if (!inserted) out.push({ start, end: start + length, attributes })
  return normalize(out)
}

// Runs with `patch` applied over [start, end).
function formatRuns(runs: DocumentRun[], start: number, end: number, patch: AttributePatch): DocumentRun[] {
  let out: DocumentRun[] = []
  for (let run of runs) {
    let from = Math.max(run.start, start)
    let to = Math.min(run.end, end)
    if (from >= to) {
      out.push(run)
      continue
    }
    if (run.start < from) out.push({ start: run.start, end: from, attributes: run.attributes })
    out.push({ start: from, end: to, attributes: patched(run.attributes, patch) })
    if (to < run.end) out.push({ start: to, end: run.end, attributes: run.attributes })
  }
  return normalize(out)
}
