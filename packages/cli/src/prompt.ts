import { createInterface, emitKeypressEvents } from "node:readline"

// Single-line text prompt with an optional default (shown in parentheses, used
// when the answer is blank). Non-TTY stdin resolves the default rather than
// blocking on input that will never arrive.
export function text(message: string, def = ""): Promise<string> {
  return new Promise<string>((resolve) => {
    if (!process.stdin.isTTY) return resolve(def)
    let rl = createInterface({ input: process.stdin, output: process.stdout })
    let suffix = def ? ` (${def})` : ""
    rl.question(`? ${message}${suffix}: `, (answer) => {
      rl.close()
      resolve(answer.trim() || def)
    })
  })
}

export interface SelectOption {
  label: string
  value: string
}

// Minimal arrow-key single-select prompt, built on node:readline (same
// dependency-free approach as repl.ts). Renders the option list, moves the
// highlight on up/down, resolves the chosen value on enter. Options are plain
// strings or { label, value } pairs when the display text differs from the
// resolved value. Callers guard on process.stdin.isTTY; a non-TTY stdin here
// resolves the first option rather than hanging on input that will never
// arrive.
export function select(message: string, options: Array<string | SelectOption>): Promise<string> {
  let items = options.map((o) => (typeof o === "string" ? { label: o, value: o } : o))
  return new Promise((resolve) => {
    let input = process.stdin
    let output = process.stdout
    if (!input.isTTY) return resolve(items[0]!.value)

    let selected = 0
    emitKeypressEvents(input)
    let wasRaw = input.isRaw
    input.setRawMode(true)

    let render = (first = false) => {
      // After the first paint the cursor sits below the block; move it back up
      // to the message line so the list redraws in place.
      if (!first) output.write(`\x1b[${items.length + 1}A`)
      output.write(`\x1b[K? ${message}\n`)
      for (let i = 0; i < items.length; i++) {
        let active = i === selected
        let pointer = active ? "\x1b[36m> " : "  "
        let reset = active ? "\x1b[0m" : ""
        output.write(`\x1b[K${pointer}${items[i]!.label}${reset}\n`)
      }
    }

    let cleanup = () => {
      input.off("keypress", onKey)
      input.setRawMode(wasRaw)
      input.pause()
    }

    let onKey = (_str: string, key: { name: string; ctrl: boolean } | undefined) => {
      if (!key) return
      if (key.name === "up") {
        selected = (selected - 1 + items.length) % items.length
        render()
      } else if (key.name === "down") {
        selected = (selected + 1) % items.length
        render()
      } else if (key.name === "return" || key.name === "enter") {
        cleanup()
        output.write("\n")
        resolve(items[selected]!.value)
      } else if (key.ctrl && (key.name === "c" || key.name === "d")) {
        cleanup()
        output.write("\n")
        process.exit(130)
      }
    }

    input.on("keypress", onKey)
    input.resume()
    render(true)
  })
}
