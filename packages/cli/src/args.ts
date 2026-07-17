import { parseArgs } from "node:util"

export let { values, positionals } = parseArgs({
  options: {
    dev: { type: "boolean", short: "d", default: false },
    minify: { type: "boolean", short: "m", default: false },
    compile: { type: "boolean", short: "c", default: false },
    flux: { type: "boolean", short: "f", default: false },
    stdout: { type: "boolean", default: false },
    output: { type: "string", short: "o" },
    "proxy-files": { type: "boolean", default: false },
    "proxy-http": { type: "boolean", default: false },
    fps: { type: "string" },
    duration: { type: "string" },
    size: { type: "string" },
    script: { type: "string" },
    capture: { type: "string" },
    tunnel: { type: "boolean", default: false },
    stats: { type: "boolean", default: false },
    android: { type: "boolean", default: false },
    device: { type: "string" },
    template: { type: "string", short: "t" },
  },
  allowPositionals: true,
})

export let command = positionals[0]
export let source = positionals[1]
export let isTsx = source?.endsWith(".tsx") || source?.endsWith(".jsx")
export let isTs = source?.endsWith(".ts") || source?.endsWith(".js")
export let isSource = isTsx || isTs
export let isPrebuilt = source?.endsWith(".srt.js") || source?.endsWith(".srt.bin")

function usage(line: string): never {
  console.error("Usage: " + line)
  process.exit(1)
}

// Per-command argument requirements. Called once before dispatch.
export function validateArgs() {
  switch (command) {
    case "bundle":
      if (values.flux) {
        if (!source || !isTs) usage("srt bundle --flux [options] <entry.[ts|js]>")
      } else if (!source || (!isSource && !isPrebuilt)) {
        usage("srt bundle [options] <entry.[tsx|jsx|ts|js|srt.js|srt.bin]>")
      }
      break
    case "check":
      if (!source || !isSource) usage("srt check <entry.[tsx|jsx|ts|js]>")
      break
    case "render":
      if (!source || !isTsx) usage("srt render <entry.[tsx|jsx]>")
      break
    case "pack":
      if (values.flux) {
        if (!source || !isTs) usage("srt pack --flux [options] <entry.[ts|js]>")
      } else if (!source || !isSource) {
        usage("srt pack [options] <entry.[tsx|jsx|ts|js]>")
      }
      break
  }

  // --android installs/launches the client on a device; it is a client action,
  // so it is only valid for `client`.
  if (values.android && command !== "client") {
    usage("srt client --android  (--android is only valid with the client command)")
  }
}

export function printUsage() {
  console.error(`Usage: srt <command> [options] [file]

Commands:
  init <dir>             Scaffold a new SolidRT project into a new (empty) folder
  run [file]             Start dev server + local solidrt-go client
  server [file]          Start dev server only
  client                 Start solidrt-go client only
  bundle <file>          Transpile TS/JS/TSX/JSX to JS or bytecode
  check <file>           Verify the app builds and typechecks, without writing anything
  render <file.tsx|jsx>  Replay a script (optional) and render frames for video generation
  pack <file>            Bundle + compile to a standalone executable (experimental)
  mcp                    MCP server (stdio) exposing the running dev server to coding agents

init options:
  -t, --template <name>  Start from a named template (skips the interactive picker)

run/server options:
      --proxy-files      Route file/dir access through the dev server
      --proxy-http       Route fetch calls through the dev server (HTTP cache enabled)
      --capture <file>   Record connected clients' key events to a script file
      --tunnel           Accept ticket-paired clients through the p2p tunnel

run/client options:
      --size <WxH>       Window size (default: 1280x720)
      --stats            Show the debug stats overlay (FPS, memory, frame timings)

client options:
      --android          Install and launch the client on a connected Android device
      --device <serial>  Target a specific adb device by serial or unique prefix

bundle options:
  -f, --flux             Bundle for the bare Flux runtime, without SolidJS (entry must be .ts|.js)
  -d, --dev              Use development build of SolidJS (default: production)
  -m, --minify           Minify the output
  -c, --compile          Compile to bytecode
  -o, --output <name>    Output filename
      --stdout           Write bundle to stdout

pack options:
  -f, --flux             Pack for the bare Flux runtime instead of SolidRT (entry must be .ts|.js)
  -m, --minify           Minify the output
  -o, --output <name>    Output filename

render options:
      --script <file>    Script file to replay (default: no scripted input)
      --fps <N>          Frames per second (default: 60)
      --duration <N>     Duration in seconds (default: 1)
      --size <WxH>       Frame size (default: 1280x720)`)
}
