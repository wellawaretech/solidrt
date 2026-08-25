import { homedir } from "node:os"
import { join } from "node:path"

// The folder name is deliberately isolated here: one switch point if it ever
// changes or becomes configurable.
const DEV_DIR_NAME = ".solidrt"

// All dev-tooling state lives in one home dotdir, one rule on every platform
// (okf/backlog/cli-flux-migration.md): servers/<key hash>/ holds a dev
// server's registry record (live.json, written by the server itself, which
// also names the folder: server/registry.ts), its remembered port and its
// tunnel key; clients/ is the data root for every locally spawned client,
// so dev client trees land in clients/client<M>/. Deleting the dir resets
// every bit of dev state.
export function devDir(...parts: string[]): string {
  return join(homedir(), DEV_DIR_NAME, ...parts)
}

/** `servers/` under the dev dir: one folder per server key. */
export function serversRoot(): string {
  return devDir("servers")
}

/** `clients/` under the dev dir - the --data-root for locally spawned dev clients. */
export function clientsRoot(): string {
  return devDir("clients")
}
