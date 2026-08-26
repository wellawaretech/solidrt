import { existsSync, statSync } from "node:fs"
import { parseArgs } from "node:util"
import { resolve } from "node:path"
import { clientsRoot } from "./dev-dir"
import { hint } from "./usage"

// Everything after a bare "--" is the app's argument vector, kept out of
// parseArgs (which would fold it into positionals) and forwarded verbatim to
// the runner, where it becomes flux:process argv. srt's own flags may follow
// the source file, so the separator is required.
let rawArgs = process.argv.slice(2)
let appArgsSep = rawArgs.indexOf("--")
export let appArgs = appArgsSep === -1 ? [] : rawArgs.slice(appArgsSep + 1)

// `srt tool <package>/<name> ...`: everything after the tool name belongs to
// the tool (its own flags, its own usage), so only the command and the tool
// name go through parseArgs and the rest is forwarded verbatim (tool/main.ts).
let isTool = rawArgs[0] === "tool"
export let toolArgs = isTool ? rawArgs.slice(2) : []

// A parse failure (unknown flag, missing value) is a usage error, not a
// crash: the parser's first sentence names it, the hint says where to look.
function parse() {
  try {
    return parseArgs({
      args: isTool ? rawArgs.slice(0, 2) : appArgsSep === -1 ? rawArgs : rawArgs.slice(0, appArgsSep),
      options: OPTIONS,
      allowPositionals: true,
    })
  } catch (e: any) {
    hint(String(e?.message ?? e).split(". ")[0])
  }
}

const OPTIONS = {
    help: { type: "boolean", default: false },
    version: { type: "boolean", default: false },
    dev: { type: "boolean", short: "d", default: false },
    minify: { type: "boolean", short: "m", default: false },
    compile: { type: "boolean", default: false },
    flux: { type: "boolean", short: "f", default: false },
    file: { type: "boolean", default: false },
    project: { type: "boolean", default: false },
    lan: { type: "boolean", default: false },
    folder: { type: "boolean", default: false },
    app: { type: "boolean", default: false },
    stdout: { type: "boolean", default: false },
    json: { type: "boolean", default: false },
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
    device: { type: "string" },
} as const

export let { values, positionals } = parse()

// An explicit --port: the dev server binds it (run, server), or the caller
// picks the server on it (client, mcp). Without it the server binds its
// remembered port or a free one, and callers resolve by project (mode.ts,
// registry.ts).
function resolvePort(): number | undefined {
  let raw = values.port
  if (raw === undefined) return undefined
  let port = Number(raw)
  if (!/^\d+$/.test(raw) || port < 1 || port > 65535) {
    console.error(`Invalid --port value "${raw}": expected a port number between 1 and 65535`)
    process.exit(1)
  }
  return port
}
export let port = resolvePort()

// Storage flags for a locally spawned client. Dev client trees live in
// ~/.solidrt/clients/client<M>/, reached through --data-root so the client
// runtime keeps its single pref-path default rule (see lattice/src/storage.rs
// and okf/backlog/cli-flux-migration.md); an explicit --data-root wins.
// Storage is per app under a tree, so two projects share client 0 without
// colliding; only two clients of the same app need distinct slots. Roots are
// passed absolute because the client chdirs into its app sandbox at startup.
export function clientStorageArgs(): string[] {
  let root = values["data-root"]
  let args = ["--data-root", root ? resolve(root) : clientsRoot()]
  let client = values.client ?? "0"
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
export let isPrebuilt = source?.endsWith(".srt.js") || source?.endsWith(".srt.bin")
// A .srt.js also ends with .js: prebuilt wins, or the server would re-bundle
// a prebuilt bundle as source and skip the prebuilt load path.
export let isTs = (source?.endsWith(".ts") || source?.endsWith(".js")) && !isPrebuilt
export let isSource = isTsx || isTs

function usage(line: string): never {
  console.error("Usage: " + line)
  process.exit(1)
}

// Per-command argument requirements. Called once before dispatch. The app
// commands take an optional entry: none means the project at the cwd
// (mode.ts decides, and reports a missing or wrong entry itself).
export function validateArgs() {
  switch (command) {
    case "bundle":
      // A prebuilt .srt.js compiles to bytecode (bundle.ts); a .srt.bin
      // already is bytecode, so there is nothing to do with it.
      if (values.flux) {
        if (!source || !isTs) usage("srt bundle --flux [options] <entry.[ts|js]>")
      } else if (source && !isSource && !source.endsWith(".srt.js")) {
        usage("srt bundle [options] [entry.[tsx|jsx|ts|js|srt.js]]")
      }
      break
    case "check":
      // An entry file, or a folder whose entries are discovered (check.ts).
      if (source && !isSource && !(existsSync(source) && statSync(source).isDirectory())) {
        usage("srt check [entry.[tsx|jsx|ts|js] | folder]")
      }
      break
    case "demo":
      // A number from the printed list, or the qualified name beside it.
      if (source && !/^\d+$/.test(source) && !/^[\w.-]+\/[\w.-]+$/.test(source)) {
        usage("srt demo [<number> | <package>/<demo>]")
      }
      break
    case "render":
      if (source && !isTsx) usage("srt render [entry.[tsx|jsx]]")
      break
    case "pack":
      if (values.flux) {
        if (!source || !isTs) usage("srt pack --flux [options] <entry.[ts|js]>")
      } else if (source && !isSource) {
        usage("srt pack [options] [entry.[tsx|jsx|ts|js]]")
      }
      break
  }

  // The commands that start a dev server; `demo` resolves its own project.
  let serves = command === "run" || command === "server" || command === "demo"
  // The commands that work on a project or a file (mode.ts).
  let onApp = command === "run" || command === "server" || command === "bundle" || command === "pack" || command === "render"
  // --folder and --app pick a pack output shape.
  if ((values.folder || values.app) && command !== "pack") {
    usage("srt pack --folder|--app  (--folder and --app are only valid with the pack command)")
  }
  if (values.folder && values.app) usage("srt pack --folder|--app  (--folder and --app exclude each other)")
  // --device picks the adb device `android` installs on.
  if (values.device && command !== "android") {
    usage("srt android --device <serial>  (--device is only valid with the android command)")
  }
  // --server points a standalone client at a dev server; `run` and `server`
  // own their server side, so it is valid nowhere else.
  if (values.server && command !== "client") {
    usage("srt client --server <host:port>  (--server is only valid with the client command)")
  }
  // --json is the dev server's rebuild contract (bundle.ts).
  if (values.json && command !== "bundle") {
    usage("srt bundle --json  (--json is only valid with the bundle command)")
  }
  // --port binds the dev server (`run`, `server`) or picks one (`client`,
  // `android`, `mcp`).
  if (port !== undefined && !serves && command !== "client" && command !== "android" && command !== "mcp") {
    usage("srt <run|server|demo|client|android|mcp> --port <N>  (--port is only valid with the run, server, demo, client, android and mcp commands)")
  }
  // --file/--project resolve a file argument in a project directory.
  if ((values.file || values.project) && !onApp) {
    usage("srt <run|server|bundle|pack|render> <file> [--file|--project]  (only valid with the run, server, bundle, pack and render commands)")
  }
  // --lan binds every interface of a server being started.
  if (values.lan && !serves) {
    usage("srt <run|server|demo> --lan  (--lan is only valid with the run, server and demo commands)")
  }
}
