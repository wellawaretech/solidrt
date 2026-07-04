import { emitKeypressEvents } from "node:readline"

// Minimal arrow-key single-select prompt, built on node:readline (same
// dependency-free approach as repl.ts). Renders the option list, moves the
// highlight on up/down, resolves the chosen value on enter. Callers guard on
// process.stdin.isTTY; a non-TTY stdin here resolves the first option rather
// than hanging on input that will never arrive.
export function select(message: string, options: string[]): Promise<string> {
  return new Promise((resolve) => {
    let input = process.stdin
    let output = process.stdout
    if (!input.isTTY) return resolve(options[0])

    let selected = 0
    emitKeypressEvents(input)
    let wasRaw = input.isRaw
    input.setRawMode(true)

    let render = (first = false) => {
      // After the first paint the cursor sits below the block; move it back up
      // to the message line so the list redraws in place.
      if (!first) output.write(`\x1b[${options.length + 1}A`)
      output.write(`\x1b[K? ${message}\n`)
      for (let i = 0; i < options.length; i++) {
        let active = i === selected
        let pointer = active ? "\x1b[36m> " : "  "
        let reset = active ? "\x1b[0m" : ""
        output.write(`\x1b[K${pointer}${options[i]}${reset}\n`)
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
        selected = (selected - 1 + options.length) % options.length
        render()
      } else if (key.name === "down") {
        selected = (selected + 1) % options.length
        render()
      } else if (key.name === "return" || key.name === "enter") {
        cleanup()
        output.write("\n")
        resolve(options[selected])
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
