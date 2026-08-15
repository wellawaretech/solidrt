---
title: Inspector - a visual devtool app over the dev-server control API
description: A packed SolidRT app presenting live runtime introspection (stats, logs, tree over snapshot, clock transport) as a peer front-end to the MCP bridge, both clients of /__control__. Never a dev-server client.
created: 2026-08-14
---

# Inspector

A first-party SolidRT application that presents live runtime introspection
visually: the human-facing peer of the MCP bridge. Shaped 2026-08-13,
implementation started; lives at `apps/inspector/`.

## The shape

The MCP server is not the data source, it is a thin stateless wrapper: every
tool is one HTTP call to `/__control__/*` on the dev server
(`packages/cli/server/control.ts`, ~15 endpoints: clients, tree, stats, logs,
snapshot, texture, buffer, gpu, debug, clock, input, reload, load, watch).

So the inspector talks to the control API directly, not MCP. The devtool and
the MCP bridge are two peer front-ends over one control API: anything the
agent can see, the human can see, and the API stays the single place to
extend. This is also the positioning: one documented API through which the
app is fully observable and drivable, with two clients on it - one for
agents, one for humans. Everyone else built the human tool first and is
retrofitting agent access; the API being agent-shaped from the start is the
part that is hard to copy.

Honest capability claim: "engine-grade introspection with app-framework
ergonomics", not "we see everything". Genuinely unusual: GPU truth (read back
any buffer or texture, enumerate live pipelines) plus virtual-time transport
(`set_time_scale` / `step_frames`) - freeze an app, single-step, read tree,
pixels and GPU state at each step. That is RenderDoc/Unity-frame-debugger
territory, which app frameworks do not ship. Known gaps, from QuickJS being
less instrumented than V8/Dart VM: no JS sampling profiler, no heap
snapshots, no breakpoints.

## Hard invariant: never a dev-server client

The inspector must never join a dev server as a websocket client. The server
broadcasts `load`/`reload` to every connected client when ids are omitted,
and latches the message for late joiners - an inspector connected as a client
gets replaced by the app under inspection. It talks `/__control__` over HTTP
only, and "not a client" must hold by construction, not by careful operation.

This is why it ships as a **packed app** (production runtime plus appended
bundle): a packed binary is never a dev-server client. Not embedded in the
runtime like the launcher (the launcher earns its bytes as the mandatory
fallback screen; the inspector is optional and will grow large). Not via the
version store initially (that puts it inside a client). Store distribution
becomes reasonable once OTA lands; a minimal embedded fallback for
inspecting a broken device is a possible later answer to the bootstrap case,
only if that bites.

Distribution of the packed binaries: a CI step over artifacts that already
exist (packing is concatenation, not compilation), shipped as downloadable
release artifacts - not payload inside the platform packages every project
installs.

## Reach and targeting

The control API is served on the dev-server port with no hostname
restriction (only `/__internal__/` is loopback-gated), and the server also
accepts p2p tunnel connections with ticket pairing. So the inspector can
target localhost, LAN (a Pi, a TV box), or a remote device through the
tunnel. This makes remote control-API access a designed feature rather than
a side effect: `/__control__` includes `/input`, `/load`, `/reload`, so
anything that reaches the port drives the app - fine on a trusted network,
ticket pairing for everything else. Choose deliberately.

Server discovery: the machine-wide registry from parallel-dev-servers
(`okf/done/parallel-dev-servers.md`, completed 2026-08-13) - port folders
under `~/.solidrt/servers/<port>/`, where only a live answer to
`/__control__/clients` proves a server. The inspector reads it the same way
the MCP bridge does. Manual host:port entry covers remote targets (the
inspector has a human in front of it); `okf/backlog/mdns-discovery.md`
would later populate the list automatically.

Graceful degradation: `list_clients` reports a `queries` list per client, so
an independently versioned inspector degrades cleanly against older
runtimes.

## Panels, in value order

1. **Stats over time** - poll `/stats`, draw latched counters as live
   sparklines. Layout-class problems become shapes you recognize; highest
   value per line.
2. **Log tail** - `/logs` is cursor-based, tailing is cheap.
3. **Tree over snapshot** - `/tree` quads composited on `/snapshot` pixels;
   hover a node, outline it on the real frame. With `/clock` this becomes a
   transport bar: pause, single-step, watch tree and pixels change together.
4. **Debug commands** - `/debug` + `list_debug` turn `registerDebug`
   commands into generated forms.
5. Later: `/gpu` resource table, `/buffer` vertex-decoding hexdump,
   `/input` gesture record/replay next to `srt record`.

Side value: the inspector leans hard on our weakest surfaces (tree widgets
with disclosure, dense tables, virtualized lists, text-heavy layout). It is
a canary that finds core gaps fast - which is why it lives in-repo and
builds against HEAD rather than in `~/solidrt/projects/`.

## State

Done so far:

- Scaffolded from the `components` template at `apps/inspector/`; `apps/*`
  added to root workspaces, deps are `workspace:*`. Per-app `AGENTS.md`
  points at the scaffold reference. The launcher moved to `apps/launcher/`
  alongside it (2026-08-14).
- `src/servers.ts` - reads the `~/.solidrt/servers` registry, probes
  `/__control__/clients` per target. Targets are `host:port`, not port
  alone: manually added ones sit beside the registry's and persist to
  `manual-servers.json` in the app data dir. A silent registry port drops
  out of the list (stale folder); a silent manual target stays, marked.
- `src/capture.ts` - client selection plus screen capture via tree +
  snapshot (window-root node, base64 PNG).
- `src/index.tsx` - server/client list, manual-add form, capture pane.

Open:

- [ ] Stats panel (sparklines over polled `/stats`)
- [ ] Log tail panel
- [ ] Tree-over-snapshot overlay and clock transport
- [ ] Debug-command forms
- [ ] Remote targets: tunnel/ticket path (manual host:port entry is in)
- [ ] `SERVERS_DIR` is hardcoded to an absolute home path in `servers.ts` -
      flux exposes no environment and the client cwd is the app data dir, so
      the app cannot derive it; needs a real answer before anyone else runs
      this
- [ ] `solidrt.displayName` in `package.json` before it appears in a
      launcher list
- [ ] Pack-and-release CI step for the distributable binaries
- [ ] Streaming: control is request/response only; live stats means polling.
      Acceptable now; a proper version wants a `/__control__/subscribe`
      stream (server work, separate item when it hurts)

Deliberately elsewhere: the open-vs-closed and monetization thread is
product material and stays out of the public repo (the decision that binds
this plan: the inspector is open and free, the API is the moat).

## Findings

- The dev server broadcasts and latches `load`/`reload` for late-joining
  clients, so any tool that must survive an app switch can never be a
  websocket client of the server it observes.
