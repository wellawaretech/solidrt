import * as clack from "@clack/prompts"

// Thin wrappers over @clack/prompts. Every prompt guards on a TTY: a non-TTY
// stdin resolves the default rather than blocking on input that will never
// arrive. Cancelling (ctrl-c) exits the process. The output helpers (step,
// outro, abort, spinner) print plain `>>`/`!!` lines on a non-TTY instead.

function unwrap<T>(value: T | symbol): T {
  if (clack.isCancel(value)) {
    clack.cancel("Cancelled")
    process.exit(130)
  }
  return value as T
}

// Single-line text prompt; a blank answer resolves the default.
export async function text(message: string, def = ""): Promise<string> {
  if (!process.stdin.isTTY) return def
  return unwrap(await clack.text({ message, defaultValue: def, placeholder: def }))
}

export interface SelectOption {
  label: string
  value: string
}

// Arrow-key single-select; non-TTY resolves the first option.
export async function select(message: string, options: Array<string | SelectOption>): Promise<string> {
  let items = options.map((o) => (typeof o === "string" ? { label: o, value: o } : o))
  if (!process.stdin.isTTY) return items[0]!.value
  return unwrap(await clack.select({ message, options: items }))
}

export interface MultiSelectOption {
  label: string
  value: string
  // Dim description, shown next to the focused option.
  hint?: string
  checked?: boolean
}

// Space toggles, enter confirms; resolves the selected values in option
// order. Non-TTY resolves the preselected values.
export async function multiselect(message: string, options: MultiSelectOption[]): Promise<string[]> {
  let preset = options.filter((o) => o.checked).map((o) => o.value)
  if (!process.stdin.isTTY) return preset
  let picked = unwrap(
    await clack.multiselect({
      message,
      options: options.map((o) => ({ label: o.label, value: o.value, hint: o.hint })),
      initialValues: preset,
      required: false,
    }),
  )
  return options.filter((o) => picked.includes(o.value)).map((o) => o.value)
}

// Yes/no prompt; non-TTY resolves the default.
export async function confirm(message: string, def = true): Promise<boolean> {
  if (!process.stdin.isTTY) return def
  return unwrap(await clack.confirm({ message, initialValue: def }))
}

// Boxed informational message; silent on a non-TTY.
export function note(message: string, title?: string) {
  if (process.stdin.isTTY) clack.note(message, title)
}

// Opens an interactive flow with a title line; silent on a non-TTY.
export function intro(title: string) {
  if (process.stdin.isTTY) clack.intro(title)
}

// A completed step of the flow.
export function step(message: string) {
  if (process.stdin.isTTY) clack.log.step(message)
  else console.log(`>> ${message}`)
}

// Closes the flow.
export function outro(message: string) {
  if (process.stdin.isTTY) clack.outro(message)
  else console.log(`>> ${message}`)
}

// Closes the flow on an error and exits 1.
export function abort(message: string): never {
  if (process.stdin.isTTY) clack.cancel(message, { output: process.stderr })
  else console.error(`!! ${message}`)
  process.exit(1)
}

export interface Spinner {
  start(message: string): void
  stop(message: string): void
  error(message: string): void
}

// Progress for one long step, replaced by its outcome on `stop` or `error`.
// On a non-TTY `start` and `error` print plain lines and `stop` is silent.
export function spinner(): Spinner {
  if (process.stdin.isTTY) return clack.spinner()
  return {
    start: (message) => console.log(`>> ${message}`),
    stop: () => {},
    error: (message) => console.error(`!! ${message}`),
  }
}