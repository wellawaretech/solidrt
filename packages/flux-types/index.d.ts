/// <reference path="./modules/process.d.ts" />
/// <reference path="./modules/path.d.ts" />
/// <reference path="./modules/http.d.ts" />
/// <reference path="./modules/fs.d.ts" />
/// <reference path="./modules/sqlite.d.ts" />
/// <reference path="./modules/subprocess.d.ts" />
/// <reference path="./modules/p2p.d.ts" />

declare let Flux: {
  /** The flux runtime version. */
  version: string
  /**
   * Feature names this build/runtime provides. Branch on availability rather
   * than the OS, e.g. `Flux.capabilities.includes("subprocess")`.
   */
  capabilities: string[]
}