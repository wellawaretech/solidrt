import * as clack from "@clack/prompts"

// Thin wrappers over @clack/prompts. Every prompt guards on a TTY: a non-TTY
// stdin resolves the default rather than blocking on input that will never
// arrive. Cancelling (ctrl-c) exits the process.

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
      options: options.map((o) => ({ label: o.label, value: o.value })),
      initialValues: preset,
      required: false,
    }),
  )
  return options.filter((o) => picked.includes(o.value)).map((o) => o.value)
}

// Boxed informational message; silent on a non-TTY.
export function note(message: string, title?: string) {
  if (process.stdin.isTTY) clack.note(message, title)
}