import { on, setRawMode, write } from "flux:tty"
import type { Key } from "flux:tty"

// A line editor over flux:tty raw mode: the terminal echoes nothing, every
// key arrives as an event, and this redraws the prompt line itself. Cursor
// movement, history (this session) and Tab completion; Ctrl-C, or Ctrl-D on
// an empty line, quits. While a submitted line runs, keys still edit the
// buffer, and the prompt comes back when it is done. Log lines written by
// console.* while the editor is up are placed above the prompt line: the
// line is cleared, the message printed, the prompt redrawn.

export type Completion = {
  /** The candidates for the trailing part of the line. */
  matches: string[]
  /** The trailing part of the line a candidate replaces. */
  replace: string
}

export type EditorOptions = {
  prompt: string
  /** A submitted line; the prompt returns once it settles. */
  onLine: (line: string) => Promise<void> | void
  onQuit: () => void
  complete?: (line: string) => Promise<Completion>
}

// Clear the line and return to column 0, then the prompt and the buffer; the
// cursor ends after the buffer, so move it back for a cursor inside it.
const CLEAR_LINE = "\r\x1b[K"

function commonPrefix(items: string[]): string {
  let prefix = items[0] ?? ""
  for (let item of items) {
    let i = 0
    while (i < prefix.length && i < item.length && prefix[i] === item[i]) i++
    prefix = prefix.slice(0, i)
  }
  return prefix
}

// Start the editor; returns the function that stops it (restores console,
// ends the line so the shell prompt starts fresh, leaves raw mode).
export function startLineEditor(opts: EditorOptions): () => void {
  let buffer = ""
  let cursor = 0
  let history: string[] = []
  let historyAt = -1
  let draft = ""
  let busy = false
  let stopped = false

  let redraw = () => {
    if (stopped || busy) return
    let back = buffer.length - cursor
    write(CLEAR_LINE + opts.prompt + buffer + (back > 0 ? `\x1b[${back}D` : ""))
  }

  let submit = () => {
    let line = buffer
    write("\r\n")
    if (line.trim() && history[history.length - 1] !== line) history.push(line)
    historyAt = -1
    buffer = ""
    cursor = 0
    busy = true
    Promise.resolve()
      .then(() => opts.onLine(line))
      .catch((e) => console.error(`[cli] ${String(e)}`))
      .then(() => {
        busy = false
        redraw()
      })
  }

  let recall = (at: number) => {
    if (historyAt === -1) draft = buffer
    historyAt = at
    buffer = at === -1 ? draft : history[at]!
    cursor = buffer.length
  }

  let complete = async () => {
    if (!opts.complete) return
    let head = buffer.slice(0, cursor)
    let asked = buffer
    let { matches, replace } = await opts.complete(head)
    // Typed on (or submitted) while the candidates were being looked up:
    // they answer a line that no longer exists.
    if (buffer !== asked || cursor !== head.length) return
    if (matches.length === 0) return
    let insert = matches.length === 1 ? matches[0]! : commonPrefix(matches)
    if (insert.length > replace.length) {
      buffer = head.slice(0, head.length - replace.length) + insert + buffer.slice(cursor)
      cursor += insert.length - replace.length
    } else if (matches.length > 1) {
      write("\r\n" + matches.join("  ") + "\r\n")
    }
    redraw()
  }

  let onKey = (key: Key) => {
    if (key.ctrl) {
      switch (key.name) {
        case "c":
          opts.onQuit()
          return
        case "d":
          if (buffer.length === 0) opts.onQuit()
          else if (cursor < buffer.length) buffer = buffer.slice(0, cursor) + buffer.slice(cursor + 1)
          break
        case "a":
          cursor = 0
          break
        case "e":
          cursor = buffer.length
          break
        case "u":
          buffer = buffer.slice(cursor)
          cursor = 0
          break
        default:
          return
      }
      redraw()
      return
    }
    if (key.meta) return
    switch (key.name) {
      case "return":
        submit()
        return
      case "backspace":
        if (cursor > 0) {
          buffer = buffer.slice(0, cursor - 1) + buffer.slice(cursor)
          cursor--
        }
        break
      case "delete":
        if (cursor < buffer.length) buffer = buffer.slice(0, cursor) + buffer.slice(cursor + 1)
        break
      case "left":
        if (cursor > 0) cursor--
        break
      case "right":
        if (cursor < buffer.length) cursor++
        break
      case "home":
        cursor = 0
        break
      case "end":
        cursor = buffer.length
        break
      case "up":
        if (history.length && historyAt !== 0) recall(historyAt === -1 ? history.length - 1 : historyAt - 1)
        break
      case "down":
        if (historyAt !== -1) recall(historyAt === history.length - 1 ? -1 : historyAt + 1)
        break
      case "tab":
        complete()
        return
      default:
        if (key.char === undefined) return
        buffer = buffer.slice(0, cursor) + key.char + buffer.slice(cursor)
        cursor += key.char.length
    }
    redraw()
  }

  setRawMode(true)
  let offKey = on("key", onKey)
  let offClose = on("close", opts.onQuit)

  // console.* output lands above the prompt line: the runtime writes the
  // message with "\r\n" line breaks in raw mode, this side clears the prompt
  // first and redraws it after.
  let original = { log: console.log, warn: console.warn, error: console.error, debug: console.debug }
  let wrap =
    (print: (...args: unknown[]) => void) =>
    (...args: unknown[]) => {
      if (!busy) write(CLEAR_LINE)
      print(...args)
      redraw()
    }
  console.log = wrap(original.log)
  console.warn = wrap(original.warn)
  console.error = wrap(original.error)
  console.debug = wrap(original.debug)

  redraw()
  return () => {
    if (stopped) return
    stopped = true
    Object.assign(console, original)
    offKey()
    offClose()
    write("\r\n")
    setRawMode(false)
  }
}
