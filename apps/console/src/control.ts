// The dev server's control API, as this console talks to it: the same
// /__control__/ endpoints the MCP bridge wraps one tool each
// (packages/cli/src/mcp/main.ts documents every one, and is the spec for
// what a reply holds). One helper builds the request, and one function per
// endpoint gives its answer a type; a command in commands.ts is one of these
// behind a label.
import type { Client, Server } from "./servers"

type Params = Record<string, string | number | boolean | undefined>

/** One control call: `path` under /__control__/, `params` as its query
 * string (undefined values dropped), GET unless `init` says otherwise, and
 * the JSON answer. A failing status throws the server's own `error` line -
 * every endpoint answers `{ error }` when it refuses - or the status when it
 * sent none. */
export async function control(
  server: Server,
  path: string,
  params: Params = {},
  init?: Parameters<typeof fetch>[1],
): Promise<any> {
  let query = Object.entries(params)
    .filter(([, value]) => value !== undefined)
    .map(([key, value]) => `${key}=${encodeURIComponent(String(value))}`)
    .join("&")
  let url = `http://${server.address}:${server.port}/__control__/${path}${query ? `?${query}` : ""}`
  let resp = await fetch(url, init)
  let body = await resp.json().catch(() => null)
  if (!resp.ok) throw new Error(body?.error ?? `${path} failed (${resp.status})`)
  return body
}

/** One line a client logged, as the server keeps it. */
export type LogLine = { seq: number; at: number; level: string; text: string; repeats?: number }

/** One client's log lines after `since` (the previous `latest`; 0 for the
 * whole buffer), with the cursor for the next call and the server
 * generation: a generation that changed means the server restarted and the
 * cursor (and the client id) are stale. */
export async function fetchLogs(
  server: Server,
  client: Client,
  since: number,
): Promise<{ lines: LogLine[]; latest: number; generation: number }> {
  let body = await control(server, "logs", { client: client.id, since })
  let lines: LogLine[] = Array.isArray(body?.entries)
    ? body.entries.map((e: any) => ({
        seq: e.seq,
        at: e.at,
        level: typeof e.level === "string" ? e.level : "log",
        text: typeof e.text === "string" ? e.text : "",
        repeats: typeof e.repeats === "number" ? e.repeats : undefined,
      }))
    : []
  return { lines, latest: typeof body?.latest === "number" ? body.latest : since, generation: body?.generation ?? 0 }
}

/** Switch one client's stats overlay on or off. The server records the
 * state per client, so the next poll shows it. */
export async function setClientStats(server: Server, client: Client, on: boolean): Promise<void> {
  await control(server, "stats", { client: client.id, active: on }, { method: "POST" })
}

/** A picture: the PNG bytes and the size they are. */
export type Shot = { png: Uint8Array; width: number; height: number }

// A picture as the snapshot and texture queries answer it.
function toShot(body: any): Shot {
  if (typeof body?.pngBase64 !== "string") throw new Error("The client sent no picture")
  let png = Uint8Array.from(atob(body.pngBase64), (c) => c.charCodeAt(0))
  return { png, width: body.width, height: body.height }
}

/** A picture of one node's subtree, by id (from the tree), as it is
 * rendered right now. */
export async function snapshotNode(server: Server, client: Client, nodeId: number): Promise<Shot> {
  return toShot(await control(server, "snapshot", { client: client.id, node: nodeId }))
}

/** A picture of one client's window right now. The root node's id is per
 * client and changes on reload, so it is read fresh each time. */
export async function snapshotClient(server: Server, client: Client): Promise<Shot> {
  let root = await control(server, "tree", { client: client.id, depth: 0 })
  if (typeof root?.id !== "number") throw new Error("The client reported no window")
  return snapshotNode(server, client, root.id)
}

/** A GPU texture read back at its native size, by id (from the GPU
 * inventory). */
export async function readTexture(server: Server, client: Client, id: number): Promise<Shot> {
  return toShot(await control(server, "texture", { client: client.id, id }))
}

/** The render tree from the window root down to `depth` levels below it,
 * nested as the server answers it: id, kind, box, text, children (a node
 * cut off by the depth carries `childCount` instead). */
export async function fetchTree(server: Server, client: Client, depth: number): Promise<any> {
  return control(server, "tree", { client: client.id, depth })
}

/** The client's performance statistics, with the summary of the last
 * `windowMs` under `window` (mcp/main.ts get_stats documents every field). */
export async function fetchStats(server: Server, client: Client, windowMs: number = 5000): Promise<any> {
  return control(server, "stats", { client: client.id, window: windowMs })
}

/** The GPU resource inventory: textures, buffers, pipelines. */
export async function fetchGpu(server: Server, client: Client): Promise<any> {
  return control(server, "gpu", { client: client.id })
}

/** The debug commands the app registered (registerDebug from srt:dev). */
export async function listDebug(server: Server, client: Client): Promise<string[]> {
  let body = await control(server, "debug", { client: client.id })
  if (!Array.isArray(body?.commands)) return []
  return body.commands.map((c: any) => (typeof c === "string" ? c : String(c?.name ?? c)))
}

/** Call one debug command with no argument; resolves with what it returned
 * (undefined as null). */
export async function callDebug(server: Server, client: Client, name: string): Promise<unknown> {
  return control(server, "debug", { client: client.id, name }, { method: "POST" })
}

/** Rebuild the server's entry and push it to every client. Throws the build
 * error when it does not compile. */
export async function reloadServer(server: Server): Promise<{ clients: number }> {
  let body = await control(server, "reload", {}, { method: "POST" })
  return { clients: typeof body?.clients === "number" ? body.clients : 0 }
}

/** Mute or unmute the user's own input on every client of the server. */
export async function setMute(server: Server, on: boolean): Promise<void> {
  await control(server, "mute", { active: on }, { method: "POST" })
}

/** Resume (`watching` true) or pause the server's reload-on-save: `active`
 * on the endpoint is whether it watches, not whether it is paused. */
export async function setWatch(server: Server, watching: boolean): Promise<void> {
  await control(server, "watch", { active: watching }, { method: "POST" })
}

/** A client's clock as it reports it after a change. */
export type Clock = { scale: number; pendingSteps: number }

/** Set one client's time scale: 0 pauses it, 1 is real time. Resolves with
 * the clock as the client now has it. */
export async function setClock(server: Server, client: Client, scale: number): Promise<Clock> {
  return control(server, "clock", { client: client.id, scale }, { method: "POST" })
}

/** Advance a paused client by `n` frames, resolving once they have been
 * presented: the client applies steps at its frame rate, so the request
 * itself returns before the frames have run. Asks the clock again (a
 * scale-0 write, which changes nothing on a paused client) until no step is
 * pending, up to about two seconds. */
export async function stepFrames(server: Server, client: Client, n: number): Promise<Clock> {
  let clock: Clock = await control(server, "clock", { client: client.id, step: n }, { method: "POST" })
  for (let tries = 0; clock.pendingSteps > 0 && tries < 40; tries++) {
    await new Promise<void>((resolve) => setTimeout(() => resolve(), 50))
    clock = await control(server, "clock", { client: client.id, scale: 0 }, { method: "POST" })
  }
  return clock
}
