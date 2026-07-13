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
import { DEV_PORT } from "../dev-server"

const CONTROL_BASE = `http://127.0.0.1:${DEV_PORT}/__control__`

type ControlResult = { ok: true; body: any } | { ok: false; message: string }

async function control(path: string, method: "GET" | "POST" = "GET"): Promise<ControlResult> {
  let resp
  try {
    resp = await fetch(CONTROL_BASE + path, { method })
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

let TOOLS: { name: string; description: string; inputSchema: Record<string, z.ZodTypeAny> }[] = [
  {
    name: "list_clients",
    description:
      "List the app clients connected to the SolidRT dev server. Each entry has id (pass it as `client` to the other tools), platform, runtime version, and the capability names compiled into that client's runtime.",
    inputSchema: {},
  },
  {
    name: "get_logs",
    description:
      "Read console output and runtime errors from connected app clients. Returns entries (seq, at, client, level, text) plus `latest`, the newest seq. Pass `since` (a seq or `latest` from a previous call) to only get newer entries; pass `wait_ms` to hold the call until new output arrives, e.g. right after triggering a reload.",
    inputSchema: {
      since: z
        .number()
        .int()
        .describe("Only return entries with seq greater than this (default 0: the whole buffer)")
        .optional(),
      wait_ms: z
        .number()
        .int()
        .describe("If nothing is newer than `since`, wait up to this many milliseconds for new output (max 30000)")
        .optional(),
    },
  },
  {
    name: "get_stats",
    description:
      "Performance statistics from a running app client: fps, CPU%, memory, smoothed JS/layout/paint/hover frame times (ms), setProperty writes per frame, demand-gate reuse/skip counts per second, and live texture count.",
    inputSchema: { client: CLIENT_ARG },
  },
  {
    name: "get_render_tree",
    description:
      "Snapshot of a running app client's render tree: node id, kind, window-relative box (x, y, width, height), text content, and children. Use it to verify what the app actually rendered and where.",
    inputSchema: { client: CLIENT_ARG },
  },
  {
    name: "get_snapshot",
    description:
      "Capture a PNG image of any node in a running app client's render tree, by node id (get ids from get_render_tree). Returns the rendered pixels of that node's subtree, so you can see what the app actually drew. The node must be currently mounted and have a non-zero layout box.",
    inputSchema: {
      nodeId: z.number().int().describe("Id of the node to capture, from get_render_tree"),
      client: CLIENT_ARG,
    },
  },
  {
    name: "reload",
    description:
      "Rebuild the app from source and push it to every connected client. Call this after editing the app's .tsx/.jsx source to apply the changes: it bundles once and reloads all clients, so a burst of edits becomes a single explicit reload. Returns the number of clients reloaded, or a build error if the source failed to compile. Follow with get_logs to see runtime output from the reloaded app.",
    inputSchema: {},
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
    case "reload":
      return control("/reload", "POST")
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

function toContent(name: string, result: ControlResult): CallToolResult {
  if (!result.ok) return { content: [{ type: "text", text: result.message }], isError: true }
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
}

export async function runMcpCommand() {
  let server = new McpServer({ name: "solidrt", version: "0.0.0" })

  for (let tool of TOOLS) {
    server.registerTool(
      tool.name,
      { description: tool.description, inputSchema: tool.inputSchema },
      async (args: any) => toContent(tool.name, await callTool(tool.name, args ?? {})),
    )
  }

  // The stdin read keeps the process alive; it exits when the agent host
  // closes the pipe.
  await server.connect(new StdioServerTransport())
}
