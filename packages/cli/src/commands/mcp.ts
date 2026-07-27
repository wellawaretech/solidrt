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
import { resolve } from "node:path"
import { DEV_PORT } from "../dev-server"

const CONTROL_BASE = `http://127.0.0.1:${DEV_PORT}/__control__`

type ControlResult = { ok: true; body: any } | { ok: false; message: string }

async function control(path: string, method: "GET" | "POST" = "GET", payload?: unknown): Promise<ControlResult> {
  let resp
  try {
    let init: RequestInit = { method }
    if (payload !== undefined) {
      init.headers = { "content-type": "application/json" }
      init.body = JSON.stringify(payload)
    }
    resp = await fetch(CONTROL_BASE + path, init)
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
  .describe("Client id from list_clients (default: the only connected client)")
  .optional()

let SAVE_TO_ARG = z
  .string()
  .describe(
    "Also write the PNG to this file path (relative paths resolve against the project root; parent directories are created)",
  )
  .optional()

// readOnly marks tools that only inspect state; it is surfaced as the
// MCP-standard readOnlyHint annotation so agent harnesses that honor it can
// auto-approve the inspection majority. load, reload, and call_debug mutate
// the running app and keep the default hints (destructive, not idempotent);
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
      "List the app clients connected to the SolidRT dev server. Returns `generation` (identity of this server run: client ids and log cursors are only valid within one generation, so if it changed since your last call, re-fetch ids and cursors) and `clients`. Each entry has id (pass it as `client` to the other tools), platform, runtime version (git describe; a -dirty suffix means the binary was built from uncommitted engine changes), build profile (debug/release), and the capability names compiled into that client's runtime. Use version/profile to check whether a connected binary contains a given engine change before debugging against it.",
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
      "Performance statistics from a running app client: fps, CPU%, memory, smoothed JS/layout/paint/hover frame times (ms), setProperty writes per frame, demand-gate reuse/skip counts per second, and live texture count. Layout-activity counters cover the last full rebuild, raw: nodes (live node count, mounted AND detached), mountedNodes/orphanNodes (live at query time: nodes reachable from the root vs not - orphans growing at a stable tree shape mean an unmount leak; absent when no engine is running), measureCalls (text measures; mostly cache hits, cheap), paraShapes (paragraphs actually shaped; the expensive signal - high layoutMs with near-zero paraShapes means the cost is not text shaping), dirtiedNodes (layout caches cleared by property writes since the previous rebuild; how much of the tree a write burst invalidated), cacheGets/cacheHits (layout-cache lookups during the rebuild; a hit on a container skips its whole subtree, so a healthy incremental rebuild shows a near-100% hit rate - a low rate at scale means the layout cache is being defeated).",
    inputSchema: { client: CLIENT_ARG },
  },
  {
    name: "get_render_tree",
    readOnly: true,
    description:
      "Snapshot of a running app client's render tree: node id, kind, window-relative box (x, y, width, height), text content, and children. Use it to verify what the app actually rendered and where. Whole trees get large: prefer `query` to find nodes by kind or text first, then `root` + `depth` to inspect the region around a match. A node whose children were cut off by `depth` carries `childCount`; descend into it with root=<its id>.",
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
      client: CLIENT_ARG,
    },
  },
  {
    name: "get_snapshot",
    readOnly: true,
    description:
      "Capture a PNG image of any node in a running app client's render tree, by node id (get ids from get_render_tree). Returns the rendered pixels of that node's subtree, so you can see what the app actually drew. Capture the smallest node that contains what you are checking (e.g. the <texture> leaf itself) - that is exactly the content at its own pixel size; the window root is mostly empty layout around it and orders of magnitude more pixels. Reserve root captures for when layout/positioning itself is the question. The node must be currently mounted and have a non-zero layout box. Works on an idle client (the capture requests its own frame); a timeout means the client's JS thread is busy or wedged, not that the app is idle.",
    inputSchema: {
      nodeId: z
        .number()
        .int()
        .describe("Id of the node to capture, from get_render_tree; prefer the smallest relevant node over the root"),
      save_to: SAVE_TO_ARG,
      client: CLIENT_ARG,
    },
  },
  {
    name: "get_gpu_resources",
    readOnly: true,
    description:
      "Inventory of a running app client's GPU resources: textures (id, size, whether a shader renders into it), vertex buffers (id, byteLength), and shader/pipeline targets (output textureId, kind, bufferId, topology, drawCount, depth, attribute layout, bound sampler texture ids, last-applied uniform values). Use it when the render tree is just a <texture> leaf and the interesting state lives behind it; follow up with get_texture or get_buffer to see contents.",
    inputSchema: { client: CLIENT_ARG },
  },
  {
    name: "get_texture",
    readOnly: true,
    description:
      "Read back any GPU texture from a running app client as a PNG, by texture id (from get_gpu_resources, or the id returned by createImage/createShader/createPipeline in app code). Works on sampled textures (atlases, data textures) and shader/pipeline render targets alike, without needing a frame. Pass x/y/width/height to crop, e.g. one tile of an atlas.",
    inputSchema: {
      id: z.number().int().describe("Texture id, from get_gpu_resources"),
      x: z.number().int().describe("Crop rect left edge in texture pixels (requires y, width, height)").optional(),
      y: z.number().int().describe("Crop rect top edge in texture pixels").optional(),
      width: z.number().int().describe("Crop rect width in texture pixels").optional(),
      height: z.number().int().describe("Crop rect height in texture pixels").optional(),
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
      if (typeof args?.client === "number") params.set("client", String(args.client))
      return control(`/snapshot?${params.toString()}`)
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
      for (let key of ["x", "y", "width", "height"]) {
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
