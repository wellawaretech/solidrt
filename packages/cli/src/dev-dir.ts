import { homedir } from "node:os"
import { join } from "node:path"

// The folder name is deliberately isolated here: one switch point if it ever
// changes or becomes configurable.
const DEV_DIR_NAME = ".solidrt"

// All dev-tooling state lives in one home dotdir, one rule on every platform
// (okf/backlog/parallel-dev-servers.md): servers/<port>/ holds a dev server's
// identity (tunnel.key) and its registry record (live.json), clients/ is the
// data root srt passes for every locally spawned client, so dev client trees
// land in clients/client<M>/. Deleting the dir resets every bit of dev state.
export function devDir(...parts: string[]): string {
  return join(homedir(), DEV_DIR_NAME, ...parts)
}

/** `servers/<port>/` under the dev dir - a dev server's identity and registry record, keyed by port. */
export function serverDir(port: number): string {
  return devDir("servers", String(port))
}

/** `clients/` under the dev dir - the --data-root for locally spawned dev clients. */
export function clientsRoot(): string {
  return devDir("clients")
}