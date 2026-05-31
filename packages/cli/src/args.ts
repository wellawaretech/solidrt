import { parseArgs } from "node:util"

export let { values, positionals } = parseArgs({
  options: {
    dev: { type: "boolean", short: "d", default: false },
    minify: { type: "boolean", short: "m", default: false },
    compile: { type: "boolean", short: "c", default: false },
    stdout: { type: "boolean", default: false },
    output: { type: "string", short: "o" },
    "proxy-files": { type: "boolean", default: false },
    "proxy-http": { type: "boolean", default: false },
    fps: { type: "string" },
    duration: { type: "string" },
    size: { type: "string" },
  },
  allowPositionals: true,
})

export let command = positionals[0]
export let source = positionals[1]
export let isTsx = source?.endsWith(".tsx") || source?.endsWith(".jsx")
export let isPrebuilt = source?.endsWith(".srt.js") || source?.endsWith(".srt.bin")

export function printUsage() {
  console.error(`Usage: srt <command> [options] [file]

Commands:
  run [file.tsx|jsx]     Start dev server + local solidrt-go client
  server [file.tsx|jsx]  Start dev server only
  client                 Start solidrt-go client only
  bundle <file.tsx|jsx>  Transpile TSX/JSX to JS or bytecode
  record <file.tsx|jsx>  Capture frames for video generation

run/server options:
      --proxy-files      Route file/dir access through the dev server
      --proxy-http       Route fetch calls through the dev server (HTTP cache enabled)

run/client options:
      --size <WxH>       Window size (default: 1280x720)

bundle options:
  -d, --dev              Use development build of SolidJS (default: production)
  -m, --minify           Minify the output
  -c, --compile          Compile to bytecode
  -o, --output <name>    Output filename
      --stdout           Write bundle to stdout

record options:
      --fps <N>          Frames per second (default: 60)
      --duration <N>     Duration in seconds (default: 1)
      --size <WxH>       Frame size (default: 1280x720)`)
}
