// The MCP bridge: a stdio Model Context Protocol server exposing the dev
// server's control API (/__control__/) as tools for coding agents. Stateless
// glue: every tool call is one HTTP request to the running dev server, so the
// bridge works no matter which process (or how many agents) spawned it.
//
// stdout is the JSON-RPC channel; nothing here may print to it.

import { DEV_PORT } from "../dev-server"

const CONTROL_BASE = `http://127.0.0.1:${DEV_PORT}/__control__`

type ControlResult = { ok: true; body: any } | { ok: false; message: string }

async function control(path: string): Promise<ControlResult> {
  let resp
  try {
    resp = await fetch(CONTROL_BASE + path)
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

let TOOLS = [
  {
    name: "list_clients",
    description:
      "List the app clients connected to the SolidRT dev server. Each entry has id (pass it as `client` to the other tools), platform, runtime version, and the capability names compiled into that client's runtime.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "get_logs",
    description:
      "Read console output and runtime errors from connected app clients. Returns entries (seq, at, client, level, text) plus `latest`, the newest seq. Pass `since` (a seq or `latest` from a previous call) to only get newer entries; pass `wait_ms` to hold the call until new output arrives, e.g. right after triggering a reload.",
    inputSchema: {
      type: "object",
      properties: {
        since: {
          type: "integer",
          description: "Only return entries with seq greater than this (default 0: the whole buffer)",
        },
        wait_ms: {
          type: "integer",
          description: "If nothing is newer than `since`, wait up to this many milliseconds for new output (max 30000)",
        },
      },
      additionalProperties: false,
    },
  },
  {
    name: "get_stats",
    description:
      "Performance statistics from a running app client: fps, CPU%, memory, smoothed JS/layout/paint/hover frame times (ms), setProperty writes per frame, demand-gate reuse/skip counts per second, and live texture count.",
    inputSchema: {
      type: "object",
      properties: {
        client: { type: "integer", description: "Client id from list_clients (default: the only connected client)" },
      },
      additionalProperties: false,
    },
  },
  {
    name: "get_render_tree",
    description:
      "Snapshot of a running app client's render tree: node id, kind, window-relative box (x, y, width, height), text content, and children. Use it to verify what the app actually rendered and where.",
    inputSchema: {
      type: "object",
      properties: {
        client: { type: "integer", description: "Client id from list_clients (default: the only connected client)" },
      },
      additionalProperties: false,
    },
  },
  {
    name: "get_snapshot",
    description:
      "Capture a PNG image of any node in a running app client's render tree, by node id (get ids from get_render_tree). Returns the rendered pixels of that node's subtree, so you can see what the app actually drew. The node must be currently mounted and have a non-zero layout box.",
    inputSchema: {
      type: "object",
      properties: {
        nodeId: { type: "integer", description: "Id of the node to capture, from get_render_tree" },
        client: { type: "integer", description: "Client id from list_clients (default: the only connected client)" },
      },
      required: ["nodeId"],
      additionalProperties: false,
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
      let qs = params.toString()
      return control(qs ? `/logs?${qs}` : "/logs")
    }
    case "get_stats":
      return control(`/stats${clientParam(args)}`)
    case "get_render_tree":
      return control(`/tree${clientParam(args)}`)
    case "get_snapshot": {
      if (typeof args?.nodeId !== "number") return { ok: false, message: "get_snapshot requires a numeric nodeId" }
      let params = new URLSearchParams({ node: String(args.nodeId) })
      if (typeof args?.client === "number") params.set("client", String(args.client))
      return control(`/snapshot?${params.toString()}`)
    }
    default:
      return { ok: false, message: `Unknown tool: ${name}` }
  }
}

export async function runMcpCommand() {
  let { Server } = await import("@modelcontextprotocol/sdk/server/index.js")
  let { StdioServerTransport } = await import("@modelcontextprotocol/sdk/server/stdio.js")
  let { ListToolsRequestSchema, CallToolRequestSchema } = await import("@modelcontextprotocol/sdk/types.js")

  let server = new Server({ name: "solidrt", version: "0.0.0" }, { capabilities: { tools: {} } })

  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: TOOLS }))

  server.setRequestHandler(CallToolRequestSchema, async (request: any) => {
    let name = request.params.name
    let result = await callTool(name, request.params.arguments ?? {})
    if (!result.ok) {
      return { content: [{ type: "text", text: result.message }], isError: true }
    }
    if (name === "get_snapshot") {
      let { pngBase64, width, height } = result.body
      return {
        content: [
          { type: "image", data: pngBase64, mimeType: "image/png" },
          { type: "text", text: `Captured node snapshot: ${width}x${height} px` },
        ],
      }
    }
    return { content: [{ type: "text", text: JSON.stringify(result.body, null, 2) }] }
  })

  // The stdin read keeps the process alive; it exits when the agent host
  // closes the pipe.
  await server.connect(new StdioServerTransport())
}
