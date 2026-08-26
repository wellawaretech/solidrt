declare module "flux:tty" {
  /**
   * Whether stdin is a terminal this process can use. False for a pipe, a
   * file, or no stdin at all (a GUI launch), the cases where nobody is there
   * to prompt; on unix also false for a job backgrounded from an interactive
   * shell (`cmd &`), which still has the terminal as stdin but would be
   * stopped by job control the moment it touched it.
   */
  export let isTTY: boolean
  /** One key press in raw mode (see {@link setRawMode}). */
  export interface Key {
    /**
     * Node's keypress names: "return", "backspace", "tab", "escape",
     * "delete", "insert", "up", "down", "left", "right", "home", "end",
     * "pageup", "pagedown", "space", "f1".."f12", or the lowercase letter
     * or symbol typed.
     */
    name: string
    /** The character typed, with its case, for a printable key; else undefined. */
    char: string | undefined
    ctrl: boolean
    /** Alt (Option) held. */
    meta: boolean
    shift: boolean
  }
  /**
   * Listen for input on stdin. `"line"` delivers one line per call as the
   * terminal's own line discipline hands it over (cooked mode: the terminal
   * does the editing), with the newline stripped; `"key"` delivers one key
   * press per call while raw mode is on (and nothing arrives as a line
   * then); `"close"` fires once when stdin reaches end of file (Ctrl-D in
   * cooked mode, or the pipe closing). A listener holds the process alive
   * until it unsubscribes; after `"close"` nothing can come, so every tty
   * listener is dropped then, and a later `on` registers nothing. stdin is
   * read once per process: a second engine in the same process (an isolate)
   * gets no input.
   *
   * @param event  `"line"`, `"key"` or `"close"`.
   * @param callback  Receives the line text or the {@link Key}; nothing for
   *                  `"close"`.
   * @returns An unsubscribe function.
   */
  export function on(event: "line", callback: (line: string) => void): () => void
  export function on(event: "key", callback: (key: Key) => void): () => void
  export function on(event: "close", callback: () => void): () => void
  /**
   * Like {@link on}, but the listener fires at most once and then unsubscribes.
   */
  export function once(event: "line", callback: (line: string) => void): () => void
  export function once(event: "key", callback: (key: Key) => void): () => void
  export function once(event: "close", callback: () => void): () => void
  /**
   * Switch the terminal's raw mode: no echo, no line editing, no signal keys
   * (Ctrl-C arrives as a key), and stdin delivers `"key"` events instead of
   * `"line"`s. The change applies from the next read: a line the terminal
   * is already collecting is delivered as a line. Turn it off before
   * exiting; the runtime also restores the terminal on exit and on a panic,
   * but not on a kill. Throws when stdin is not a terminal.
   *
   * While raw, `console.log` output still breaks lines correctly (the
   * runtime writes "\r\n"); your own {@link write} calls must use "\r\n".
   */
  export function setRawMode(on: boolean): void
  /**
   * Write `text` to stdout as is and flush: no newline appended, unlike
   * `console.log`. What a prompt needs.
   */
  export function write(text: string): void
}
