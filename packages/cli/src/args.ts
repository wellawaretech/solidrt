import { parseArgs } from "node:util"
import { resolve } from "node:path"
import { clientsRoot } from "./dev-dir"

// Everything after a bare "--" is the app's argument vector, kept out of
// parseArgs (which would fold it into positionals) and forwarded verbatim to
// the runner, where it becomes flux:process argv. srt's own flags may follow
// the source file, so the separator is required.
let rawArgs = process.argv.slice(2)
let appArgsSep = rawArgs.indexOf("--")
export let appArgs = appArgsSep === -1 ? [] : rawArgs.slice(appArgsSep + 1)

export let { values, positionals } = parseArgs({
  args: appArgsSep === -1 ? rawArgs : rawArgs.slice(0, appArgsSep),
  options: {
    dev: { type: "boolean", short: "d", default: false },
    minify: { type: "boolean", short: "m", default: false },
    compile: { type: "boolean", default: false },
    flux: { type: "boolean", short: "f", default: false },
    session: { type: "string", short: "s" },
    folder: { type: "boolean", default: false },
    stdout: { type: "boolean", default: false },
    output: { type: "string", short: "o" },
    "proxy-http": { type: "boolean", default: false },
    fps: { type: "string" },
    duration: { type: "string" },
    size: { type: "string" },
    script: { type: "string" },
    capture: { type: "string" },
    tunnel: { type: "boolean", default: false },
    stats: { type: "boolean", default: false },
    "data-root": { type: "string" },
    client: { type: "string", short: "c" },
    server: { type: "string" },
    port: { type: "string" },
    android: { type: "boolean", default: false },
    device: { type: "string" },
    template: { type: "string", short: "t" },
  },
  allowPositionals: true,
})

export const DEFAULT_DEV_PORT = 0x8844

// The session number: -s/--session <N> selects the dev server (port
// DEFAULT_DEV_PORT + N, see dev-server.ts resolveDevPort) and the default
// client slot. Resolved at load, like the port.
function resolveSession(): number {
  let raw = values.session
  if (raw === undefined) return 0
  let n = Number(raw)
  if (!/^\d+$/.test(raw) || DEFAULT_DEV_PORT + n > 65535) {
    console.error(
      `Invalid --session value "${raw}": expected a non-negative integer with ${DEFAULT_DEV_PORT} + N at most 65535`,
    )
    process.exit(1)
  }
  return n
}
export let session = resolveSession()

// Storage flags for a locally spawned client. Dev client trees live in
// ~/.solidrt/clients/client<M>/, reached through --data-root so the client
// runtime keeps its single pref-path default rule (see lattice/src/storage.rs
// and okf/backlog/parallel-dev-servers.md); an explicit --data-root wins. The
// client slot defaults to the session number, so each session gets its own
// tree with a single flag. Roots are passed absolute because the client
// chdirs into its app sandbox at startup.
export function clientStorageArgs(): string[] {
  let root = values["data-root"]
  let args = ["--data-root", root ? resolve(root) : clientsRoot()]
  let client = values.client ?? String(session)
  if (!/^\d+$/.test(client)) {
    console.error(`Invalid --client value "${client}": expected a non-negative integer`)
    process.exit(1)
  }
  args.push("--client", client)
  return args
}

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
  // --server points a standalone client at a dev server; `run` and `server`
  // own their server side, so it is only valid for `client`.
  if (values.server && command !== "client") {
    usage("srt client --server <host[:port]>  (--server is only valid with the client command)")
  }
  // --port moves the dev server off its default port, so it belongs to the
  // commands that start one (`run`, `server`) or attach to one (`mcp`). A
  // standalone client carries the port in --server <host:port> instead.
  if (values.port !== undefined && command !== "run" && command !== "server" && command !== "mcp") {
    usage("srt <run|server|mcp> --port <N>  (--port is only valid with the run, server and mcp commands)")
  }
  // --session selects a dev server (and the default client slot), so it is
  // valid wherever a dev server is started, attached to, or resolved.
  if (
    values.session !== undefined &&
    command !== "run" &&
    command !== "server" &&
    command !== "client" &&
    command !== "mcp"
  ) {
    usage("srt <run|server|client|mcp> -s <N>  (--session is only valid with the run, server, client and mcp commands)")
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
  -s, --session <N>      Session number: dev server on port 34884+N, client slot N (default: 0)
      --port <N>         Dev server port (default: 34884 + session)
      --proxy-http       Route fetch calls through the dev server (HTTP cache enabled)
      --capture <file>   Record connected clients' key events to a script file
      --tunnel           Accept ticket-paired clients through the p2p tunnel
      -- <args...>       Everything after -- reaches the app on every client (flux:process argv)

run/client options:
      --size <WxH>       Window size (default: 1280x720)
      --stats            Show the debug stats overlay (FPS, memory, frame timings)
      --data-root <dir>  Client data root (default: ~/.solidrt/clients)
  -c, --client <N>       Client number: its own data tree under the data root (default: the session)

client options:
  -s, --session <N>      Connect to this session's dev server on this machine
                         (127.0.0.1:34884+N); without it, start on the connect screen
      --server <host[:port]>  Connect to a dev server at this address (default port: 34884 + session)
      --android          Install and launch the client on a connected Android device
      --device <serial>  Target a specific adb device by serial or unique prefix

mcp options:
  -s, --session <N>      Attach to the dev server of this session (default: resolve by project)
      --port <N>         Port of the dev server to attach to (default: resolve by project)

bundle options:
  -f, --flux             Bundle for the bare Flux runtime, without SolidJS (entry must be .ts|.js)
  -d, --dev              Use development build of SolidJS (default: production)
  -m, --minify           Minify the output
      --compile          Compile to bytecode
  -o, --output <name>    Output filename
      --stdout           Write bundle to stdout

pack options:
      --folder           Write the flat app folder (runner + manifest + bundle + assets)
                         instead of the single-file executable
  -f, --flux             Pack for the bare Flux runtime instead of SolidRT (entry must be .ts|.js)
  -m, --minify           Minify the output
  -o, --output <name>    Output filename

render options:
      --script <file>    Script file to replay (default: no scripted input)
      --fps <N>          Frames per second (default: 60)
      --duration <N>     Duration in seconds, fractions allowed (default: 1)
      --size <WxH>       Frame size in physical pixels (default: 1280x720)
  -o, --output <path>    Where frames land: a directory (frame-NNNNNN.png inside it)
                         or a path prefix (default: the current directory)
      -- <args...>       Everything after -- is passed to the app (flux:process argv)`)
}
