import { CLI_VERSION } from "./project"

// What srt says about itself: the version, the usage, and the short hint a
// bare `srt`, an unknown command or a bad flag gets instead of the full usage
// (so an error line stays on screen).

export function printVersion() {
  console.log(CLI_VERSION)
}

// The banner is information, not the error: only the error line goes to
// stderr, and only an error fails the run.
export function hint(error?: string): never {
  if (error) console.error(error)
  console.log(`srt ${CLI_VERSION}\nRun srt --help for usage.`)
  process.exit(error ? 1 : 0)
}

export function printUsage() {
  console.log(USAGE)
}

const USAGE = `Usage: srt <command> [options] [file]

Commands:
  init <dir>             Scaffold a new SolidRT project into a new (empty) folder
  run [file]             Start dev server + local solidrt-go client
  server [file]          Start dev server only
  client                 Start solidrt-go client only
  demo [<number>]        List the demos the CLI ships, or run one
  tool [<pkg>/<name>]    List the tools the installed packages ship, or run one
                         (everything after the tool name is the tool's own arguments)
  console                Start the dev console: the dev servers on this machine and their clients
  android [file.apk]     Launch the client on a connected Android device (--install to install it first)
                         (a packed APK: install and launch that app instead)
  bundle [file]          Transpile TS/JS/TSX/JSX to JS or bytecode
                         (a prebuilt <name>.srt.js: compile it to bytecode)
  check [file]           Verify the app builds and typechecks, without writing anything
                         (no file: every examples/*/src/index.tsx, packages/*/examples/*.tsx
                         and packages/*/demos/src/*.tsx)
  render [file]          Replay a script (optional) and render frames for video generation
  pack [file]            Bundle + compile to a standalone executable (experimental)
  mcp                    MCP server (stdio) exposing the running dev server to coding agents

Global options:
      --help             Print this usage and exit
      --version          Print the srt version and exit

run/server/bundle/pack/render: what the command works on
  srt run                In a project root (package.json): the project, entry from
                         "solidrt": { "entry" } (default src/index.tsx)
  srt run <file>         Outside a project: the file on its own (no assets, no isolates)
  srt run <file> --project   In a project root: the project, with this entry
  srt run <file> --file      In a project root: the file on its own, ignoring the project
  Build outputs land under dist/ in the current directory.
  One server per project or file; each keeps the port it had last time,
  else the first free one from 34884 up (see the startup line). Loopback only unless --lan.

run/server/demo options:
      --port <N>         Bind this port instead of the remembered/next free one
      --lan              Bind every interface and announce the LAN address (QR)
      --proxy-http       Route fetch calls through the dev server (HTTP cache enabled)
      --capture <file>   Record connected clients' key events to a script file
      --tunnel           Accept ticket-paired clients through the p2p tunnel
      -- <args...>       Everything after -- reaches the app on every client (flux:process argv)

run/client options:
      --size <WxH>       Window size (default: 1280x720)
      --stats            Show the debug stats overlay (FPS, memory, frame timings)
      --data-root <dir>  Client data root (default: ~/.solidrt/clients)
  -c, --client <N>       Client number: its own data tree under the data root (default: 0)

client options:
  (no flags)             Connect to the dev server of the project (or file) in the current directory
      --port <N>         Connect to the local dev server on this port
      --server <host:port>  Connect to a dev server at this address

android options:
  (no flags)             Launch the installed client, pointed at the dev server of the project (or file)
                         in the current directory (the server must run with --lan, or be reached from an emulator)
  <file.apk>             Install a packed APK (srt pack --apk) and launch it, nothing dev-flavored
      --install          Install or update the client first, from the project's @solidrt/android-<abi> package
      --port <N>         Point it at the local dev server on this port
      --device <serial>  Target a specific adb device by serial or unique prefix

mcp options:
      --port <N>         Attach to the dev server on this port (default: resolve by project)

bundle options:
  -f, --flux             Bundle for the bare Flux runtime, without SolidJS (entry must be .ts|.js)
  -d, --dev              Use development build of SolidJS (default: production)
  -m, --minify           Minify the output
      --compile          Compile to bytecode
  -o, --output <dir>     Output directory (default: dist/bundle; for a prebuilt
                         .srt.js, its own directory)
      --stdout           Write bundle to stdout
      --json             Write the bundle, its manifest and its isolates as one JSON object
                         to stdout (the dev server's rebuild; --server <host:port> names it)

console options:
      -- <args...>       Everything after -- reaches the console (flux:process argv)

pack options:
      --folder           Write the flat app folder (runner + manifest + bundle + assets)
                         instead of the single-file executable
      --app              Write the app alone as one <entry>.srtapp (manifest + bundle + assets,
                         no runner), for a runner to load: solidrt <file>.srtapp
      --apk              Patch the app into an installable Android APK (id, label, icon,
                         version, payload; no Android SDK needed; base: the runner APK)
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
      -- <args...>       Everything after -- is passed to the app (flux:process argv)`
