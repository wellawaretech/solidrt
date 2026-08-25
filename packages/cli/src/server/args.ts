// The server's command line: the interface `srt run` / `srt server` (a bun
// launcher that spawns this process) and the console (a flux app that spawns
// it or imports it) share, so it takes flags and never a config blob
// (okf/done/srt-command-folders.md):
//
//   flux server.js [file] [--project|--file] [--port N] [--lan] [--proxy-http]
//                  [--capture f] [--tunnel] [--stats] [--minify]
//                  [--client N [--data-root d] [--size WxH]] [-- args]
//
// --client N spawns the local solidrt-go client with data slot N (`srt run`);
// without it the server runs alone (`srt server`). Everything after a bare
// "--" is the app's argument vector (flux:process argv on every client).
// flux:process argv is the tail after the script, so a small parser covers
// it: boolean flags, valued flags, one positional, the "--" tail.

import { argv } from "flux:process"

export type ServerArgs = {
  /** The entry file argument, or none (the project at the cwd). */
  entry: string | undefined
  project: boolean
  file: boolean
  lan: boolean
  proxyHttp: boolean
  tunnel: boolean
  stats: boolean
  minify: boolean
  port: number | undefined
  capture: string | undefined
  /** The local client's data slot, or null for no local client. */
  client: number | null
  dataRoot: string | undefined
  size: string | undefined
  appArgs: string[]
}

// A fatal usage or configuration error. flux has no exit call: an uncaught
// error at startup ends the process with a nonzero status and the message
// on stderr, which is what a failed launch needs.
export function fail(message: string): never {
  throw new Error(message)
}

function integer(name: string, raw: string, min: number, max: number): number {
  let value = Number(raw)
  if (!/^\d+$/.test(raw) || value < min || value > max) {
    fail(`Invalid --${name} value "${raw}": expected an integer between ${min} and ${max}`)
  }
  return value
}

export function parseArgs(raw: string[] = argv): ServerArgs {
  let sep = raw.indexOf("--")
  let own = sep === -1 ? raw : raw.slice(0, sep)
  let args: ServerArgs = {
    entry: undefined,
    project: false,
    file: false,
    lan: false,
    proxyHttp: false,
    tunnel: false,
    stats: false,
    minify: false,
    port: undefined,
    capture: undefined,
    client: null,
    dataRoot: undefined,
    size: undefined,
    appArgs: sep === -1 ? [] : raw.slice(sep + 1),
  }
  let value = (name: string, i: number): string => {
    let v = own[i]
    if (v === undefined || v.startsWith("--")) fail(`--${name} needs a value`)
    return v
  }
  for (let i = 0; i < own.length; i++) {
    let arg = own[i]!
    if (!arg.startsWith("--")) {
      if (args.entry !== undefined) fail(`Unexpected argument: ${arg}`)
      args.entry = arg
      continue
    }
    let name = arg.slice(2)
    switch (name) {
      case "project":
        args.project = true
        break
      case "file":
        args.file = true
        break
      case "lan":
        args.lan = true
        break
      case "proxy-http":
        args.proxyHttp = true
        break
      case "tunnel":
        args.tunnel = true
        break
      case "stats":
        args.stats = true
        break
      case "minify":
        args.minify = true
        break
      case "port":
        args.port = integer(name, value(name, ++i), 1, 65535)
        break
      case "capture":
        args.capture = value(name, ++i)
        break
      case "client":
        args.client = integer(name, value(name, ++i), 0, Number.MAX_SAFE_INTEGER)
        break
      case "data-root":
        args.dataRoot = value(name, ++i)
        break
      case "size":
        args.size = value(name, ++i)
        break
      default:
        fail(`Unknown flag: ${arg}`)
    }
  }
  return args
}
