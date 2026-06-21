/// <reference path="./modules/process.d.ts" />
/// <reference path="./modules/path.d.ts" />
/// <reference path="./modules/http.d.ts" />
/// <reference path="./modules/fs.d.ts" />
/// <reference path="./modules/sqlite.d.ts" />
/// <reference path="./modules/subprocess.d.ts" />
/// <reference path="./modules/p2p.d.ts" />

// Web-standard globals. The runtime is QuickJS, not a browser or Node, so it
// ships no lib.dom / @types/bun: these declarations are the sole source for
// console, fetch, the Fetch types, timers, WebSocket, and the encoders.
/// <reference path="./standards/console.d.ts" />
/// <reference path="./standards/time.d.ts" />
/// <reference path="./standards/text.d.ts" />
/// <reference path="./standards/fetch.d.ts" />
/// <reference path="./standards/websocket.d.ts" />

// GUI globals (present only on a gui-enabled runtime).
/// <reference path="./gui/camera.d.ts" />
/// <reference path="./gui/microphone.d.ts" />
/// <reference path="./gui/gpu.d.ts" />
/// <reference path="./gui/raf.d.ts" />
/// <reference path="./gui/ffi.d.ts" />

declare let Flux: {
  /** The flux runtime version. */
  version: string
  /**
   * Feature names this build/runtime provides. Branch on availability rather
   * than the OS, e.g. `Flux.capabilities.includes("subprocess")`.
   */
  capabilities: string[]
}