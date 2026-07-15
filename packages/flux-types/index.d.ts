/// <reference path="./modules/process.d.ts" />
/// <reference path="./modules/path.d.ts" />
/// <reference path="./modules/http.d.ts" />
/// <reference path="./modules/fs.d.ts" />
/// <reference path="./modules/sqlite.d.ts" />
/// <reference path="./modules/subprocess.d.ts" />
/// <reference path="./modules/p2p.d.ts" />
/// <reference path="./modules/net.d.ts" />
/// <reference path="./modules/mdns.d.ts" />
/// <reference path="./modules/wasm.d.ts" />

// Web-standard globals. The runtime is QuickJS, not a browser or Node, so it
// ships no lib.dom / @types/bun: these declarations are the sole source for
// console, fetch, the Fetch types, timers, WebSocket, and the encoders.
/// <reference path="./standards/console.d.ts" />
/// <reference path="./standards/time.d.ts" />
/// <reference path="./standards/text.d.ts" />
/// <reference path="./standards/fetch.d.ts" />
/// <reference path="./standards/websocket.d.ts" />

// GUI capabilities (present only on a gui-enabled runtime). rendertree/camera/
// microphone/gpu are flux:* modules like the rest; requestAnimationFrame stays a
// global (web-standard name). flux:rendertree is the render-tree bridge the
// renderer drives; displaying the built tree (renderFrame) is the runner's
// concern (srt:render in lattice), not part of flux.
/// <reference path="./gui/rendertree.d.ts" />
/// <reference path="./gui/camera.d.ts" />
/// <reference path="./gui/microphone.d.ts" />
/// <reference path="./gui/audio.d.ts" />
/// <reference path="./gui/gpu.d.ts" />
/// <reference path="./gui/raf.d.ts" />

declare let Flux: {
  /** The flux runtime version. */
  version: string
  /**
   * Feature names this build/runtime provides. Branch on availability rather
   * than the OS, e.g. `Flux.capabilities.includes("subprocess")`.
   */
  capabilities: string[]
}