/// <reference path="./modules/process.d.ts" />
/// <reference path="./modules/tty.d.ts" />
/// <reference path="./modules/path.d.ts" />
/// <reference path="./modules/http.d.ts" />
/// <reference path="./modules/fs.d.ts" />
/// <reference path="./modules/sqlite.d.ts" />
/// <reference path="./modules/subprocess.d.ts" />
/// <reference path="./modules/svg.d.ts" />
/// <reference path="./modules/image.d.ts" />
/// <reference path="./modules/p2p.d.ts" />
/// <reference path="./modules/net.d.ts" />
/// <reference path="./modules/mdns.d.ts" />
/// <reference path="./modules/wasm.d.ts" />
/// <reference path="./modules/isolate.d.ts" />
/// <reference path="./modules/test.d.ts" />

// Web-standard globals. The runtime is QuickJS, not a browser or Node, so it
// ships no lib.dom / @types/bun: these declarations are the sole source for
// console, fetch, the Fetch types, the streams, the compression streams,
// timers, WebSocket, the encoders, and crypto.
/// <reference path="./standards/console.d.ts" />
/// <reference path="./standards/time.d.ts" />
/// <reference path="./standards/text.d.ts" />
/// <reference path="./standards/streams.d.ts" />
/// <reference path="./standards/compression.d.ts" />
/// <reference path="./standards/base64.d.ts" />
/// <reference path="./standards/fetch.d.ts" />
/// <reference path="./standards/websocket.d.ts" />
/// <reference path="./standards/abort.d.ts" />
/// <reference path="./standards/crypto.d.ts" />
/// <reference path="./standards/clipboard.d.ts" />

// GUI capabilities (present only on a gui-enabled runtime). rendertree/camera/
// microphone/gpu are flux:* modules like the rest; requestAnimationFrame stays a
// global (web-standard name). flux:rendertree is the render-tree bridge the
// renderer drives; displaying the built tree (renderFrame) is the runner's
// concern: sol:render, below.
/// <reference path="./gui/rendertree.d.ts" />
/// <reference path="./gui/camera.d.ts" />
/// <reference path="./gui/microphone.d.ts" />
/// <reference path="./gui/audio.d.ts" />
/// <reference path="./gui/gpu.d.ts" />
/// <reference path="./gui/spatial.d.ts" />
/// <reference path="./gui/font.d.ts" />
/// <reference path="./gui/video.d.ts" />
/// <reference path="./gui/raf.d.ts" />
/// <reference path="./gui/test.d.ts" />

// The SolidRT runner's modules (the "sol:*" surface), provided by lattice, the
// layer over flux that runs an app: present on the solidrt client and on any
// custom runtime built over lattice, absent on the bare flux binary, where an
// import of one fails to resolve. Declared here rather than in a package of
// their own: every lattice-targeted project already names this package in its
// tsconfig `types`, and the gui section above already declares surface a given
// build may lack, so a second package would buy only a name boundary. Each
// file's header says which builds have its module.
/// <reference path="./lattice/events.d.ts" />
/// <reference path="./lattice/app.d.ts" />
/// <reference path="./lattice/dev.d.ts" />
/// <reference path="./lattice/apps.d.ts" />
/// <reference path="./lattice/render.d.ts" />
/// <reference path="./lattice/test.d.ts" />

declare let Flux: {
  /** The flux runtime version. */
  version: string
  /**
   * Feature names this build/runtime provides. Branch on availability rather
   * than the OS, e.g. `Flux.capabilities.includes("subprocess")`.
   */
  capabilities: string[]
}