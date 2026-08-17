// The MCP bridge: a stdio Model Context Protocol server exposing the dev
// server's control API (/__control__/) as tools for coding agents. Stateless
// glue: every tool call is one HTTP request to the running dev server, so the
// bridge works no matter which process (or how many agents) spawned it.
//
// stdout is the JSON-RPC channel; nothing here may print to it.

import { z } from "zod"
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js"
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js"
import { dirname, join, resolve } from "node:path"
import { existsSync, readdirSync, readFileSync } from "node:fs"
import { values } from "../args"
import { DEV_PORT } from "../dev-server"
import { devDir } from "../dev-dir"

// An explicit -s/--port pins the port for the bridge's lifetime. Otherwise
// the port is resolved per tool call from the server registry, so one bridge
// (started when the workspace opens, kept alive across server restarts)
// follows whichever server is currently serving this project - and the
// scaffold's mcp.json never carries a port.
const FIXED_PORT = values.port !== undefined || values.session !== undefined ? DEV_PORT : null

// The projectDir the bridge is working in: the nearest package.json above its
// own cwd, the same rule srt applies to an entry (project.ts projectDirFor),
// so both sides derive the same string.
function findProjectDir(): string | null {
  let dir = process.cwd()
  while (true) {
    if (existsSync(join(dir, "package.json"))) return dir
    let parent = dirname(dir)
    if (parent === dir) return null
    dir = parent
  }
}

function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}

type LiveRecord = { pid: number; port: number; projectDir: string }

// The global server registry: every running dev server keeps a live.json in
// ~/.solidrt/servers/<port>/ (see dev-server.ts writeLiveRecord). Unreadable or
// malformed records are skipped, not fatal - the registry is a hint.
function liveRecords(): LiveRecord[] {
  let root = devDir("servers")
  let names: string[]
  try {
    names = readdirSync(root)
  } catch {
    return []
  }
  let records: LiveRecord[] = []
  for (let name of names) {
    try {
      let record = JSON.parse(readFileSync(join(root, name, "live.json"), "utf8"))
      if (typeof record?.pid === "number" && typeof record?.port === "number" && typeof record?.projectDir === "string") {
        records.push(record)
      }
    } catch {}
  }
  return records
}

type PortResult = { ok: true; port: number } | { ok: false; message: string }

async function resolvePort(): Promise<PortResult> {
  if (FIXED_PORT !== null) return { ok: true, port: FIXED_PORT }
  let project = findProjectDir()
  if (!project) {
    return {
      ok: false,
      message: `No package.json found above ${process.cwd()}, so no dev server can be resolved by project. Pass -s <N> or --port <N> to srt mcp.`,
    }
  }
  let matches = liveRecords().filter((r) => r.projectDir === project && pidAlive(r.pid))
  if (matches.length > 1) {
    let ports = matches
      .map((r) => r.port)
      .sort((a, b) => a - b)
      .join(", ")
    return { ok: false, message: `${matches.length} dev servers are serving this project (ports ${ports}); pass -s <N> to srt mcp` }
  }
  if (matches.length === 0) {
    return { ok: false, message: `No dev server for ${project}. Start one with srt run, or pass -s <N> to srt mcp.` }
  }
  let port = matches[0]!.port
  // The record is a hint; the server is authoritative. The probe catches a
  // stale record whose pid was reused by an unrelated process.
  try {
    let probe = await fetch(`http://127.0.0.1:${port}/__control__/clients`)
    let body: any = await probe.json().catch(() => null)
    if (!probe.ok || body?.projectDir !== project) {
      return {
        ok: false,
        message: `The server on port ${port} is not serving ${project}${
          typeof body?.projectDir === "string" ? ` (it serves ${body.projectDir})` : ""
        }. Start one with srt run, or pass -s <N> to srt mcp.`,
      }
    }
  } catch {
    return {
      ok: false,
      message: `No dev server for ${project}: the registry lists port ${port} but nothing answers there. Start one with srt run.`,
    }
  }
  return { ok: true, port }
}

type ControlResult = { ok: true; body: any } | { ok: false; message: string }

async function control(path: string, method: "GET" | "POST" = "GET", payload?: unknown): Promise<ControlResult> {
  let resolved = await resolvePort()
  if (!resolved.ok) return resolved
  let resp
  try {
    let init: RequestInit = { method }
    if (payload !== undefined) {
      init.headers = { "content-type": "application/json" }
      init.body = JSON.stringify(payload)
    }
    resp = await fetch(`http://127.0.0.1:${resolved.port}/__control__${path}`, init)
  } catch {
    return {
      ok: false,
      message: "Dev server not running. Start it in the project first: srt run src/index.tsx (or srt server)",
    }
  }
  let body: any = null
  try {
    body = await resp.json()
  } catch {}
  if (!resp.ok) return { ok: false, message: String(body?.error ?? `Dev server responded with HTTP ${resp.status}`) }
  return { ok: true, body }
}

let CLIENT_ARG = z
  .number()
  .int()
  .describe("Client id from list_clients (default: the only connected client; required when several are connected)")
  .optional()

let SAVE_TO_ARG = z
  .string()
  .describe(
    "Also write the PNG to this file path (relative paths resolve against the project root; parent directories are created)",
  )
  .optional()

// readOnly marks tools that only inspect state; it is surfaced as the
// MCP-standard readOnlyHint annotation so agent harnesses that honor it can
// auto-approve the inspection majority. load, reload, call_debug, and
// send_input mutate the running app and keep the default hints (destructive,
// not idempotent);
// `annotations` overrides those defaults where a mutating tool is benign
// (watch: a reversible, idempotent toggle). Every tool gets
// openWorldHint: false - the bridge only ever talks to the local dev server.
let TOOLS: {
  name: string
  description: string
  inputSchema: Record<string, z.ZodTypeAny>
  readOnly?: boolean
  annotations?: { destructiveHint?: boolean; idempotentHint?: boolean }
}[] = [
  {
    name: "list_clients",
    readOnly: true,
    description:
      "List the app clients connected to the SolidRT dev server. Returns `generation` (identity of this server run: client ids and log cursors are only valid within one generation, so if it changed since your last call, re-fetch ids and cursors), `entry` (the app source file this server currently serves and rebuilds - check it matches the app you intend to drive before acting, since the dev port is fixed and a `load` moves the entry mid-session), `projectDir` (the project root the server was started in), and `clients`. Each entry has id (pass it as `client` to the other tools), platform, runtime version (git describe; a -dirty suffix means the binary was built from uncommitted engine changes), build profile (debug/release), and the capability names compiled into that client's runtime, and `queries` - the dev-tool query kinds that client's runtime answers (clock, input, snapshot, tree, ...). Check `queries` before planning a verification strategy: a client whose list lacks \"input\" predates send_input, one that lacks \"clock\" predates set_time_scale/step_frames (an empty list means the runtime predates the advertisement itself). Use version/profile to check whether a connected binary contains a given engine change before debugging against it.",
    inputSchema: {},
  },
  {
    name: "get_logs",
    readOnly: true,
    description:
      "Read console output and runtime errors from connected app clients. Returns entries (seq, at, client, level, text; consecutive identical entries are collapsed into one with a `repeats` count and the run's last seq), plus `latest` (the newest seq) and `generation` (identity of this server run; if it changed since your last call, your seq cursor and client ids are stale - start over from since 0). Pass `since` (a seq or `latest` from a previous call) to only get newer entries; pass `wait_ms` to hold the call until new output arrives, e.g. right after triggering a reload; pass `level`/`contains` to filter, e.g. level \"error\" to skip chatty output.",
    inputSchema: {
      since: z
        .number()
        .int()
        .describe("Only return entries with seq greater than this (default 0: the whole buffer)")
        .optional(),
      wait_ms: z
        .number()
        .int()
        .describe("If nothing matches newer than `since`, wait up to this many milliseconds for new output (max 30000)")
        .optional(),
      level: z
        .string()
        .describe('Only return entries with one of these levels, comma-separated (e.g. "error" or "error,warn")')
        .optional(),
      contains: z
        .string()
        .describe("Only return entries whose text contains this substring (case-insensitive)")
        .optional(),
    },
  },
  {
    name: "get_stats",
    readOnly: true,
    description:
      "Performance statistics from a running app client: fps, CPU%, memory, smoothed JS/layout/paint/hover frame times (ms), setProperty writes per frame, demand-gate reuse/skip counts per second, and live texture count. Layout-activity counters cover the last full rebuild, raw: nodes (live node count, mounted AND detached), mountedNodes/orphanNodes (live at query time: nodes reachable from the root vs not - orphans growing at a stable tree shape mean an unmount leak; absent when no engine is running), measureCalls (text measures; mostly cache hits, cheap), paraShapes (paragraphs actually shaped, i.e. words the shared word cache did not have; the expensive signal - high layoutMs with near-zero paraShapes means the cost is not text shaping), wordHits (words answered from the shared word cache; hits high and paraShapes near zero on a text change means only the changed words were reshaped), dirtiedNodes (layout caches cleared by property writes since the previous rebuild; how much of the tree a write burst invalidated), cacheGets/cacheHits (layout-cache lookups during the rebuild; a hit on a container skips its whole subtree, so a healthy incremental rebuild shows a near-100% hit rate - a low rate at scale means the layout cache is being defeated). GPU-side health, read live at query time (absent when no engine is running): rasterQueue (raster commands sent but not yet executed; stuck nonzero means the raster thread is backlogged - the state where fps and frameMs go blind because no frames complete), idleTicks (cumulative idle frame signals emitted while the GPU had nothing queued; idleTicks racing while rasterQueue sits nonzero would mean the idle-tick gate is broken), fenceTimeouts (cumulative present-fence waits that expired instead of signaling - each one is a frame where the GPU was over budget for 100ms+ and one-frame-in-flight pacing was lost; zero on a healthy machine, climbing means the GPU is the bottleneck right now), gpuPasses/gpuPassMs (cumulative shader/pipeline target renders on the raster thread and the wall time they took in whole ms - diff two queries to get a rate; passes racing far ahead of frames means redundant target re-renders, the failure mode where fps and frameMs look healthy while the raster thread drowns; the ms figure is raster-thread occupancy issuing the passes, not GPU-side duration), rasterCmdMs (cumulative wall time in whole ms the raster thread spent executing non-frame commands - texture uploads, readbacks, offscreen rasterizations, shader compiles, param writes and the target re-renders they trigger; the work frameMs never sees, so rasterCmdMs growing much faster than frames are presented means the raster thread is drowning in side work even if every counter above looks calm).",
    inputSchema: { client: CLIENT_ARG },
  },
  {
    name: "get_render_tree",
    readOnly: true,
    description:
      "Snapshot of a running app client's render tree: node id, kind, window-relative box (x, y, width, height), text content, and children. Use it to verify what the app actually rendered and where. Pass props: true to also get each node's current property values (JSX names, only values that differ from the defaults - so an empty/absent props object means everything is at its default) and, for nodes moved off their box by a rotate/scale/3D transform anywhere on their ancestor chain, `quad`: the four painted corners in window coordinates [x0,y0, x1,y1, x2,y2, x3,y3] (pre-transform top-left, top-right, bottom-right, bottom-left). The box is always the quad's axis-aligned bounds, so under a transform the box alone overstates the footprint - read the quad for where edges actually landed. Use props to answer 'is rotate/color/d applied right now' in one call instead of loading probe entries. Whole trees get large: prefer `query` to find nodes by kind or text first, then `root` + `depth` (+ props) to inspect the region around a match. A node whose children were cut off by `depth` carries `childCount`; descend into it with root=<its id>.",
    inputSchema: {
      root: z
        .number()
        .int()
        .describe("Only return the subtree under this node id (default: the whole tree)")
        .optional(),
      depth: z
        .number()
        .int()
        .describe("Levels of children to include below the root (default: unlimited; 0 = the root node only)")
        .optional(),
      query: z
        .string()
        .describe(
          "Search instead of snapshot: return `matches`, nodes whose kind equals or text contains this " +
            "(case-insensitive), each with a `path` of ancestor ids from the search root. Combine with `root` to " +
            "scope the search; `depth` is ignored.",
        )
        .optional(),
      props: z
        .boolean()
        .describe("Include each node's current off-default property values and, for transformed nodes, the painted quad")
        .optional(),
      client: CLIENT_ARG,
    },
  },
  {
    name: "get_snapshot",
    readOnly: true,
    description:
      "Capture a PNG image of any node in a running app client's render tree, by node id (get ids from get_render_tree). Returns the rendered pixels of that node's subtree, so you can see what the app actually drew. Capture the smallest node that contains what you are checking (e.g. the <texture> leaf itself) - that is exactly the content at its own pixel size; the window root is mostly empty layout around it and orders of magnitude more pixels. Reserve root captures for when layout/positioning itself is the question. The node must be currently mounted and paint a non-zero box. Detached (`d-*`) nodes capture their painted box: their own `w`/`h` when set, else the box inherited from the nearest laid-out ancestor (the same box get_render_tree reports for them). A capture renders only that node's subtree, with no ancestor paint: pixels nothing in the subtree draws come back transparent, not the background an ancestor draws behind the node - capture the window root when the background matters. Pass x/y/width/height to crop and `scale` to magnify: captures may be downscaled before you see them, so verify small hand-authored geometry (sprites, path data, icons) with a tight crop at 4x-8x rather than squinting at a full capture. Crop coordinates are in captured-image pixels (the width x height a capture of that node reports - device pixels), not the logical units get_render_tree reports. Works on an idle client (the capture requests its own frame); a timeout means the client's JS thread is busy or wedged, not that the app is idle.",
    inputSchema: {
      nodeId: z
        .number()
        .int()
        .describe("Id of the node to capture, from get_render_tree; prefer the smallest relevant node over the root"),
      x: z
        .number()
        .int()
        .describe("Crop rect left edge in captured-image pixels (requires y, width, height)")
        .optional(),
      y: z.number().int().describe("Crop rect top edge in captured-image pixels").optional(),
      width: z.number().int().describe("Crop rect width in captured-image pixels").optional(),
      height: z.number().int().describe("Crop rect height in captured-image pixels").optional(),
      scale: z
        .number()
        .int()
        .min(1)
        .max(8)
        .describe(
          "Integer magnification, 1-8: each captured pixel becomes an NxN block (nearest-neighbour), so you see " +
            "the actual rendered pixels enlarged. Combine with a crop; the scaled output is capped at 8192 px per side",
        )
        .optional(),
      save_to: SAVE_TO_ARG,
      client: CLIENT_ARG,
    },
  },
  {
    name: "get_gpu_resources",
    readOnly: true,
    description:
      "Inventory of a running app client's GPU resources: textures (id, size, whether a shader renders into it), vertex buffers (id, byteLength), and shader/pipeline targets (output textureId, kind, bufferId, topology, drawCount plus firstVertex/instanceCount when off their 0/1 defaults, depth, attribute layout, bound sampler texture ids, current uniform values - the most recent writes, which the next frame or readback draws with - plus passes/passMs, cumulative per-target render count and raster-thread wall time in whole ms: when get_stats shows gpuPasses running hot, these attribute the cost to the specific target). Use it when the render tree is just a <texture> leaf and the interesting state lives behind it; follow up with get_texture or get_buffer to see contents.",
    inputSchema: { client: CLIENT_ARG },
  },
  {
    name: "get_texture",
    readOnly: true,
    description:
      "Read back any GPU texture from a running app client as a PNG, by texture id (from get_gpu_resources, or the id returned by createImage/createShaderTexture/createPipelineTexture in app code). Works on sampled textures (atlases, data textures) and shader/pipeline render targets alike, without needing a frame: a render target reads as its current output, with any pending params, geometry or sampled-input changes resolved first. Pass x/y/width/height to crop, e.g. one tile of an atlas, and `scale` to magnify small content like a single tile or glyph.",
    inputSchema: {
      id: z.number().int().describe("Texture id, from get_gpu_resources"),
      x: z.number().int().describe("Crop rect left edge in texture pixels (requires y, width, height)").optional(),
      y: z.number().int().describe("Crop rect top edge in texture pixels").optional(),
      width: z.number().int().describe("Crop rect width in texture pixels").optional(),
      height: z.number().int().describe("Crop rect height in texture pixels").optional(),
      scale: z
        .number()
        .int()
        .min(1)
        .max(8)
        .describe(
          "Integer magnification, 1-8: each texture pixel becomes an NxN block (nearest-neighbour). Combine with " +
            "a crop; the scaled output is capped at 8192 px per side",
        )
        .optional(),
      save_to: SAVE_TO_ARG,
      client: CLIENT_ARG,
    },
  },
  {
    name: "get_buffer",
    readOnly: true,
    description:
      "Read back part of a GPU vertex buffer from a running app client, decoded to numbers. Returns values plus byteOffset/byteLength actually read and bufferByteLength. Reads are capped at 64 KiB per call; page through larger buffers with offset. Use it to verify geometry after a writeBuffer, e.g. the dynamic sprite tail of a vertex buffer.",
    inputSchema: {
      id: z.number().int().describe("Buffer id, from get_gpu_resources"),
      offset: z.number().int().describe("Byte offset to start reading at (default 0)").optional(),
      length: z.number().int().describe("Number of values to read (default: the rest of the buffer, capped)").optional(),
      as: z.enum(["f32", "u16", "u8"]).describe("How to decode the bytes (default f32)").optional(),
      client: CLIENT_ARG,
    },
  },
  {
    name: "list_debug",
    readOnly: true,
    description:
      "List the debug commands the running app registered via registerDebug from srt:dev. Returns the command names; call one with call_debug. Empty when the app registered none.",
    inputSchema: { client: CLIENT_ARG },
  },
  {
    name: "call_debug",
    description:
      "Call a debug command the running app registered via registerDebug from srt:dev, by name (from list_debug). `args` is passed to the command's function as its single argument; the command's return value comes back JSON-serialized (undefined as null). Commands run synchronously on the app's JS thread - use them to query app state (positions, counters, internal flags) or trigger app behavior (toggle a mode, open a door) without touching its real input handling.",
    inputSchema: {
      name: z.string().describe("Debug command name, from list_debug"),
      args: z.record(z.string(), z.any()).describe("Argument object passed to the command (default: none)").optional(),
      client: CLIENT_ARG,
    },
  },
  {
    name: "reload",
    description:
      "Rebuild the app from source and push it to every connected client. Call this after editing the app's .tsx/.jsx source to apply the changes: it bundles once and reloads all clients, so a burst of edits becomes a single explicit reload. Returns the number of clients reloaded, or a build error if the source failed to compile. A successful reload re-enables the file watcher if you paused it with the watch tool. Follow with get_logs to see runtime output from the reloaded app.",
    inputSchema: {},
  },
  {
    name: "load",
    description:
      "Load an app entry: bundle the given .tsx/.jsx source file and push it to every connected client, replacing whatever is running. Use it when the dev server has no app loaded yet, or to switch to a different app; later reload calls rebuild this entry. Returns the number of clients loaded, or a build error if the source failed to compile. A successful load re-enables the file watcher if you paused it with the watch tool.",
    inputSchema: {
      entry: z.string().describe("App entry source file to load (relative paths resolve against the project root)"),
    },
  },
  {
    name: "set_time_scale",
    annotations: { destructiveHint: false, idempotentHint: true },
    description:
      "Control a running app client's clock. scale=0 freezes app time: onFrame/requestAnimationFrame stop being delivered, setTimeout/setInterval freeze, performance.now() holds still, and the picture stops - so get_snapshot can capture an exact frame of any animation instead of racing it (tool round trips are usually slower than the animation). Combine with a registerDebug command that sets up the state to photograph: set state, pause, snapshot. Other values scale time for dt-driven apps (0.5 = half speed, 2 = double); apps that advance a fixed amount per onFrame call only respond to 0 and 1. Date.now() stays wall time throughout. The scale is client runtime state: it survives across your snapshots but resets to 1 on reload/load and on client restart. ALWAYS set it back to 1 when you are done - a paused client looks wedged to the human watching the screen.",
    inputSchema: {
      scale: z
        .number()
        .min(0)
        .describe("Time scale: 0 = pause, 1 = normal, 0.5 = half speed, 2 = double speed"),
      client: CLIENT_ARG,
    },
  },
  {
    name: "step_frames",
    annotations: { destructiveHint: false },
    description:
      "While paused (set_time_scale 0), advance a running app client by exactly n frames: each frame moves app time forward one refresh period (~16.7 ms at 60 Hz), runs onFrame/requestAnimationFrame and any timers that come due, and presents the result. Deterministic single-stepping for animations and game logic: pause, snapshot, step, snapshot again to see exactly what changed in n frames. With the clock running this is a no-op (frames already flow). Steps are applied at the client's frame rate, so n frames take about n refresh periods of wall time before a following snapshot shows the result.",
    inputSchema: {
      n: z.number().int().min(1).max(1000).describe("Number of frames to advance (1-1000)"),
      client: CLIENT_ARG,
    },
  },
  {
    name: "send_input",
    description:
      "Send synthetic input to a running app client through the real input pipeline (hit testing, focus, event bubbling) - the same path physical input takes, unlike call_debug which sets state directly, so use this to verify interactions actually work. Events run in order; each may wait delayMs (0-5000 ms) before firing, and the call returns after the last event has entered the pipeline, so a following get_snapshot sees the result. Event kinds: {type:'pointer', action:'down'|'up'|'move'|'tap', x, y} for clicks and drags - coordinates in logical points, the same space get_render_tree reports; 'tap' is down+up with an optional holdMs between; button 0 = left (default), 1 = middle, 2 = right; pointerType 'mouse' (default) keeps hovering at its last position afterwards like a real cursor, use 'touch' for gestures that should end hover-free. {type:'key', action:'down'|'up'|'tap', key} with W3C key names exactly as the runtime reports them ('w', 'ArrowLeft', 'Enter', ' '); a 'tap' with holdMs holds the key down that long, e.g. holdMs 500 = walk forward half a second in one call; modifier booleans shift/ctrl/alt/meta. {type:'text', text} enters text through the TextInput path - focus the target first with a pointer tap on it (the tap also activates the text session). {type:'wheel', x, y, deltaX, deltaY} scrolls; positive deltaY scrolls content down. Recipes: click a button = [{type:'pointer',action:'tap',x:400,y:300}]. Drag = down, then moves with delayMs 16 each, then up. Deterministic interaction test = set_time_scale 0, send_input, step_frames, get_snapshot. A down/up over empty space hits nothing, exactly like real input - check coordinates against get_render_tree when a click seems to do nothing.",
    inputSchema: {
      events: z
        .array(
          z.object({
            type: z.enum(["key", "pointer", "wheel", "text"]),
            action: z.enum(["down", "up", "move", "tap"]).optional(),
            key: z.string().optional().describe("W3C key name, required for type key"),
            text: z.string().optional().describe("Text to enter, required for type text"),
            x: z.number().optional().describe("Logical points, required for pointer and wheel"),
            y: z.number().optional().describe("Logical points, required for pointer and wheel"),
            deltaX: z.number().optional(),
            deltaY: z.number().optional(),
            button: z.number().int().min(0).max(4).optional(),
            pointerType: z.enum(["mouse", "touch"]).optional(),
            delayMs: z.number().int().min(0).max(5000).optional().describe("Wait before this event"),
            holdMs: z.number().int().min(0).max(5000).optional().describe("Tap only: time between down and up"),
            shift: z.boolean().optional(),
            ctrl: z.boolean().optional(),
            alt: z.boolean().optional(),
            meta: z.boolean().optional(),
          }),
        )
        .min(1)
        .max(200)
        .describe("Event sequence, executed in order"),
      client: CLIENT_ARG,
    },
  },
  {
    name: "watch",
    annotations: { destructiveHint: false, idempotentHint: true },
    description:
      "Pause or resume the dev server's automatic reload-on-save. The srt file watcher pushes a rebuild whenever app source changes on disk; call watch with enabled: false BEFORE creating or editing source files so your half-finished work is not pushed to the user's screens mid-burst, then apply everything with one explicit reload (a successful reload or load re-enables the watcher, so pause again before the next burst of file changes). The human's own saves auto-reload only while the watcher is enabled, so do not leave it paused when you stop working.",
    inputSchema: {
      enabled: z.boolean().describe("false pauses auto-reload-on-save, true resumes it"),
    },
  },
]

function clientParam(args: any): string {
  return typeof args?.client === "number" ? `?client=${args.client}` : ""
}

async function callTool(name: string, args: any): Promise<ControlResult> {
  switch (name) {
    case "list_clients":
      return control("/clients")
    case "get_logs": {
      let params = new URLSearchParams()
      if (typeof args?.since === "number") params.set("since", String(args.since))
      if (typeof args?.wait_ms === "number") params.set("wait", String(args.wait_ms))
      if (typeof args?.level === "string") params.set("level", args.level)
      if (typeof args?.contains === "string") params.set("contains", args.contains)
      let qs = params.toString()
      return control(qs ? `/logs?${qs}` : "/logs")
    }
    case "get_stats":
      return control(`/stats${clientParam(args)}`)
    case "get_render_tree": {
      let params = new URLSearchParams()
      if (typeof args?.root === "number") params.set("root", String(args.root))
      if (typeof args?.depth === "number") params.set("depth", String(args.depth))
      if (typeof args?.query === "string") params.set("query", args.query)
      if (args?.props === true) params.set("props", "true")
      if (typeof args?.client === "number") params.set("client", String(args.client))
      let qs = params.toString()
      return control(qs ? `/tree?${qs}` : "/tree")
    }
    case "reload":
      return control("/reload", "POST")
    case "load": {
      if (typeof args?.entry !== "string" || !args.entry) return { ok: false, message: "load requires an entry path" }
      // Resolved here in the bridge: this process runs at the project root,
      // the dev server may not.
      return control("/load", "POST", { entry: resolve(args.entry) })
    }
    case "watch": {
      if (typeof args?.enabled !== "boolean") return { ok: false, message: "watch requires enabled: true or false" }
      return control("/watch", "POST", { enabled: args.enabled })
    }
    case "get_snapshot": {
      if (typeof args?.nodeId !== "number") return { ok: false, message: "get_snapshot requires a numeric nodeId" }
      let params = new URLSearchParams({ node: String(args.nodeId) })
      for (let key of ["x", "y", "width", "height", "scale"]) {
        if (typeof args?.[key] === "number") params.set(key, String(args[key]))
      }
      if (typeof args?.client === "number") params.set("client", String(args.client))
      return control(`/snapshot?${params.toString()}`)
    }
    case "set_time_scale": {
      if (typeof args?.scale !== "number" || !(args.scale >= 0)) {
        return { ok: false, message: "set_time_scale requires scale >= 0" }
      }
      let params = new URLSearchParams({ scale: String(args.scale) })
      if (typeof args?.client === "number") params.set("client", String(args.client))
      return control(`/clock?${params.toString()}`, "POST")
    }
    case "step_frames": {
      if (typeof args?.n !== "number" || !(args.n >= 1)) return { ok: false, message: "step_frames requires n >= 1" }
      let params = new URLSearchParams({ step: String(args.n) })
      if (typeof args?.client === "number") params.set("client", String(args.client))
      return control(`/clock?${params.toString()}`, "POST")
    }
    case "send_input": {
      if (!Array.isArray(args?.events) || args.events.length === 0)
        return { ok: false, message: "send_input requires a non-empty events array" }
      return control(`/input${clientParam(args)}`, "POST", { events: args.events })
    }
    case "get_gpu_resources":
      return control(`/gpu${clientParam(args)}`)
    case "list_debug":
      return control(`/debug${clientParam(args)}`)
    case "call_debug": {
      if (typeof args?.name !== "string") return { ok: false, message: "call_debug requires a command name" }
      let params = new URLSearchParams({ name: args.name })
      if (typeof args?.client === "number") params.set("client", String(args.client))
      return control(`/debug?${params.toString()}`, "POST", args?.args)
    }
    case "get_texture": {
      if (typeof args?.id !== "number") return { ok: false, message: "get_texture requires a numeric id" }
      let params = new URLSearchParams({ id: String(args.id) })
      for (let key of ["x", "y", "width", "height", "scale"]) {
        if (typeof args?.[key] === "number") params.set(key, String(args[key]))
      }
      if (typeof args?.client === "number") params.set("client", String(args.client))
      return control(`/texture?${params.toString()}`)
    }
    case "get_buffer": {
      if (typeof args?.id !== "number") return { ok: false, message: "get_buffer requires a numeric id" }
      let params = new URLSearchParams({ id: String(args.id) })
      if (typeof args?.offset === "number") params.set("offset", String(args.offset))
      if (typeof args?.length === "number") params.set("length", String(args.length))
      if (typeof args?.as === "string") params.set("as", args.as)
      if (typeof args?.client === "number") params.set("client", String(args.client))
      return control(`/buffer?${params.toString()}`)
    }
    default:
      return { ok: false, message: `Unknown tool: ${name}` }
  }
}

async function toContent(name: string, result: ControlResult, args?: any): Promise<CallToolResult> {
  if (!result.ok) return { content: [{ type: "text", text: result.message }], isError: true }
  if (name === "get_snapshot" || name === "get_texture") {
    let { pngBase64, width, height } = result.body
    let label = name === "get_snapshot" ? "Captured node snapshot" : "Texture contents"
    let text = `${label}: ${width}x${height} px`
    // save_to is handled here in the bridge, not by the dev server: this
    // process runs on the caller's machine, so the path lands where the
    // agent expects it. The image content block alone is a dead end for
    // that - the model sees the pixels but never the bytes.
    if (typeof args?.save_to === "string") {
      let path = resolve(args.save_to)
      try {
        await Bun.write(path, Buffer.from(pngBase64, "base64"))
        text += `, saved to ${path}`
      } catch (e) {
        return { content: [{ type: "text", text: `Captured, but saving to ${path} failed: ${e}` }], isError: true }
      }
    }
    return {
      content: [
        { type: "image", data: pngBase64, mimeType: "image/png" },
        { type: "text", text },
      ],
    }
  }
  return { content: [{ type: "text", text: JSON.stringify(result.body, null, 2) }] }
}

export async function runMcpCommand() {
  let server = new McpServer({ name: "solidrt", version: "0.0.0" })

  for (let tool of TOOLS) {
    server.registerTool(
      tool.name,
      {
        description: tool.description,
        inputSchema: tool.inputSchema,
        annotations: { readOnlyHint: !!tool.readOnly, openWorldHint: false, ...tool.annotations },
      },
      async (args: any) => toContent(tool.name, await callTool(tool.name, args ?? {}), args),
    )
  }

  // The stdin read keeps the process alive; it exits when the agent host
  // closes the pipe.
  await server.connect(new StdioServerTransport())
}
