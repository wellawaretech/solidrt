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
    fps: { type: "string" },
    duration: { type: "string" },
    size: { type: "string" },
  },
  allowPositionals: true,
})

export let command = positionals[0]
export let source = positionals[1]
export let isTsx = source?.endsWith(".tsx")
export let isPrebuilt = source?.endsWith(".srt.js") || source?.endsWith(".srt.bin")

export function printUsage() {
  console.error(`Usage: srt <command> [options] [file]

Commands:
  run [file.tsx]              Start dev server + local solidrt-go client
  run --client                Start solidrt-go client only
  run --server [file.tsx]     Start dev server only
  build <file.tsx>            Bundle
  record <file.tsx>           Capture frames for video generation

run options:
      --client              Run client only
      --server              Run server only
      --cache               Enable HTTP cache
      --size <WxH>          Window size (default: 1280x720)

build options:
  -m, --minify          Minify the output
  -c, --compile         Compile to bytecode
  -o, --output <name>   Output filename
      --stdout          Write bundle to stdout

record options:
      --fps <N>             Frames per second (default: 30)
      --duration <N>        Duration in seconds (default: 1)
      --size <WxH>          Frame size (default: 1280x720)`)
}