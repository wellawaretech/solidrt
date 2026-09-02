---
title: Devtools introspection survey
description: What browser devtools have that the control API lacks, and candidate extensions (pick mode, highlight, live prop edit, node-to-source, composer REPL, streaming) shaped as control endpoints plus console blocks.
created: 2026-09-02
---

# Devtools introspection survey

A research note, to be continued. Question: browsers let you introspect the
live page via devtools; in solidrt the only path is the control channel.
What would "doing better" look like, given the console app is being built?

## Framing

Browser devtools are four capabilities stacked:

1. see the tree live
2. point at a pixel and get its element (pick)
3. edit the element in place
4. type code at the page (console REPL)

The control API today gives capability 1, request/response only, and none of
the other three.

The positioning from `okf/plans/inspector.md` is the organizing principle:
one agent-shaped API, two peer front-ends (MCP bridge for agents, console for
humans). So every idea below is phrased as "one `/__control__` endpoint plus
one console block or header widget" - the agent gets each capability for free
the same day the human does. This also rules out the main architectural
alternative: an in-process inspector drawn by the runtime itself (Flutter
widget-inspector style). It would help on a lone device, but it splits the
surface in two and undercuts the one-API story. The overlay post-pass
participating as display hands for the external tool (highlight, pick
affordance) gets most of that benefit without a second UI.

## Candidate extensions, in value order

### 1. Pick mode

The single biggest ergonomic gap. `POST /pick?client=` puts the client in a
mode where hover draws the hovered node's bounds via the overlay post-pass
(the stats overlay proves that layer exists and stays out of snapshots), and
a click resolves the request with the node id instead of dispatching.
Hit-testing on the input path already exists. In the console chat this is a
one-shot ask: press Pick, tap the device, the reply block is the node with
its props and a tight snapshot. For an agent it collapses the
snapshot/guess-coordinates/query-tree dance into one call whenever a human
is available to point.

### 2. Highlight from the tool

The reverse direction: hover a row in the console's tree block, the node's
box lights up on the actual device. Cheap - bounds already come from
`/tree`; needs only `/highlight?node=` writing into the same post-pass
overlay. Tree-over-snapshot (inspector panel 3) is the offline version;
highlighting on the live screen is what makes the tree feel connected to the
app, especially with a phone or TV in hand.

Pick and highlight are two directions of the same small overlay-and-endpoint
mechanism, and together they make the console's tree block feel like
devtools rather than a listing. Likely the pair to shape first.

### 3. Live prop editing

`/tree?props=true` is read-only. A dev-only `POST /set?node=&prop=` that
pokes the renderer node directly makes the tree an editor. Semantics land
exactly where browser style-editing does: the write is below the framework,
so the next reactive write clobbers it - fine, browsers have the same
contract with re-renders. This is the "nudge the padding until it looks
right" loop that currently costs an edit plus reload round trip.

### 4. Node-to-source

Browser devtools without framework extensions show DOM; React/Solid
devtools earn their keep by answering "what code made this". Dev bundles
already carry TSX sourcemaps, so if dev-mode createNode stamps the creation
site (a couple of fields, dev only), the tree block gains "open in editor" -
and the agent gains "which file do I edit for this node", which is genuinely
hard for it today.

### 5. The composer as the REPL

The console SPEC notes Load is not offered because the composer has nothing
to type into yet. Flip that: the composer is where the browser-console
analog belongs. Typed expression in, result block out, in the app's JS
context, dev-only. The chat shape is literally a REPL transcript. Open
design question (ask before building): `registerDebug` is the sanctioned
"commands, not eval" pattern, and an eval endpoint is a philosophical step
past it - though the control API already lets any caller `/load` arbitrary
entries, so it is not a new trust boundary, and `srt run` already has a flux
repl to route.

### 6. Streaming

Already on the inspector plan's open list as `/__control__/subscribe`. The
enabler for the "live views" genre the console SPEC assigns to header/nav:
stats sparklines, a tree that updates as the app mutates, log follow without
per-card polling. Still after 1-3: polling is acceptable at dev-tool rates,
and the picker/editor loop changes what the tool is rather than how fresh
it is.

## Further out

- Breakpoints, heap, profiling: the acknowledged QuickJS gaps (inspector
  plan). If it ever matters, the leverage move is a small CDP-subset shim
  (QuickJS debugger-protocol precedents exist) so existing editor debuggers
  attach, rather than building a debugger UI.
- A Solid reactivity panel (which signal woke which effect) is the
  framework-devtools layer on top; needs hooks from Solid 2 itself, so a
  different project.
