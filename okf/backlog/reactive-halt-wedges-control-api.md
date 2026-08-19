---
title: A reactive halt wedges the control API queries
description: After REACTIVITY_HALTED (an uncaught error in the reactive system) the client keeps running but tree/snapshot control queries time out, so the tooling reports "JS thread busy or app wedged" instead of showing the error; queries should keep answering from the last good tree.
created: 2026-08-19
---

# A reactive halt wedges the control API queries

## Symptom

An app error halts the reactive system (REACTIVITY_HALTED, e.g. a throwing
validation in a render expression). The client stays alive and the error is
in /logs, but `/tree` and `/snapshot` (and the MCP tools over them) time
out: "the client is connected but did not answer". An agent or developer
then debugs a phantom hang instead of reading the actual error, and only a
client restart recovers the tooling.

## Why

The debug queries are answered on the JS thread in step with the frame
loop; after a halt no frame callback runs anymore, so queries queue
forever. The render tree itself is intact (the last committed frame keeps
presenting).

## Done looks like

- `/tree`, `/snapshot`, `/stats` answer after a halt, serving the last
  committed tree.
- Bonus: the query response carries a `halted: true` marker (or /clients
  reports it) so tools can say "the app reactive system halted, see /logs"
  instead of timing out.
