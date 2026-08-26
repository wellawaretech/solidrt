declare module "flux:tty" {
  /**
   * Whether stdin is a terminal. False for a pipe, a file, or no stdin at all
   * (a GUI launch), the cases where nobody is there to prompt.
   */
  export let isTTY: boolean
  /**
   * Listen for input on stdin. `"line"` delivers one line per call as the
   * terminal's own line discipline hands it over (cooked mode: the terminal
   * does the editing), with the newline stripped; `"close"` fires once when
   * stdin reaches end of file (Ctrl-D, or the pipe closing). A listener holds
   * the process alive until it unsubscribes; after `"close"` no line can
   * come, so every tty listener is dropped then, and a later `on` registers
   * nothing. stdin is read once per process: a second engine in the same
   * process (an isolate) gets no input.
   *
   * @param event  `"line"` or `"close"`.
   * @param callback  Receives the line text; nothing for `"close"`.
   * @returns An unsubscribe function.
   */
  export function on(event: "line", callback: (line: string) => void): () => void
  export function on(event: "close", callback: () => void): () => void
  /**
   * Like {@link on}, but the listener fires at most once and then unsubscribes.
   */
  export function once(event: "line", callback: (line: string) => void): () => void
  export function once(event: "close", callback: () => void): () => void
  /**
   * Write `text` to stdout as is and flush: no newline appended, unlike
   * `console.log`. What a prompt needs.
   */
  export function write(text: string): void
}
