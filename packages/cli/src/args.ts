import { parseArgs } from "node:util"

export let { values, positionals } = parseArgs({
  options: {
    minify: { type: "boolean", short: "m", default: false },
    compile: { type: "boolean", short: "c", default: false },
    stdout: { type: "boolean", default: false },
    output: { type: "string", short: "o" },
    client: { type: "boolean", default: false },
    server: { type: "boolean", default: false },
    cache: { type: "boolean", default: false },
  },
  allowPositionals: true,
})

export let command = positionals[0]
export let source = positionals[1]
export let isTsx = source?.endsWith(".tsx")
export let isPrebuilt = source?.endsWith(".srt.js") || source?.endsWith(".srt.bin")

export function printUsage() {
  console.error(`Usage: srt <command> [options] [entry.tsx]

Commands:
  run [file.tsx]        Start dev server + local solidrt-go client
  run --client          Start solidrt-go client only
  run --server [file]   Start dev server only
  record <file.tsx>     Record frames

Options:
  -m, --minify          Minify the output
  -c, --compile         Compile to bytecode (build only)
  -o, --output <name>   Bundle filename (build only)
      --stdout          Write bundle to stdout (build only)
      --cache           Enable HTTP cache`)
}