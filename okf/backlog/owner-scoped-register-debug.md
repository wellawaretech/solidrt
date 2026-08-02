---
type: backlog-item
title: Owner-scoped registerDebug
description: Registrations reset on hot reload, so commands must register at module init - which forces any app state a command touches up to module scope; an owner-scoped variant auto-cleaned like onFrame lets both live in the component they belong to.
status: open
timestamp: 2026-08-02T00:00:00Z
---

# Owner-scoped registerDebug

Source: the animated-explainer demo feedback (2026-08-02). The demo's clock
signal is a module-level createSignal purely so its seek/pause/play debug
commands can reach it from module init.

srt:dev's registerDebug documents "registrations reset on hot reload, so
register at module init" - correct, but the constraint leaks into app
structure. An owner-scoped registration (auto-cleaned on owner disposal
like onFrame, re-registered naturally when the new instance mounts) would
let a debug command close over component state where it belongs. Parent
surface: [[mcp-debug-commands]].
