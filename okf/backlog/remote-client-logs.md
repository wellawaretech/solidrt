---
title: Carry a remote client's engine log into /logs
description: An Android client's own log lines (the engine logger, slow-frame warnings, GL and shader diagnostics) never reach the dev server, so /logs shows nothing from the device and every device problem starts with adb logcat; the client should forward its log the way the local one's stderr is captured.
created: 2026-10-07
---

# Carry a remote client's engine log into /logs

## Symptom

`/logs?client=<android id>` returns no entries for a connected tablet
client, including its startup lines (`SolidRT version`, `GPU ready`), its
slow-frame warnings and anything a shader compile or GL validation would
print. The engine log exists, under the SDL/APP tag in logcat, found only
with `adb logcat --pid=<pid>`. The local client's stderr is captured by
the server (entries with `client: -1`); a remote one has no route at all,
as the debugging reference notes. Every device investigation therefore
starts outside the tool, and an agent driving the device through the
control API is blind to the client's own diagnostics.

## Done looks like

A remote client forwards its engine log lines over its dev connection and
they appear in `/logs` under that client's id, at their level, with the
same cursor semantics as the rest. The local client keeps its stderr
capture. Volume is bounded: the slow-frame summary collapse in tiny.md
applies here too, and a client that cannot keep up drops lines and says
so once.

## Involves

The client's logger (lattice's log sink on Android writes to logcat) gains
a second sink over the dev-server connection (`lattice/src/go/connection.rs`,
the same channel the capabilities report uses); the server's log store
takes entries tagged by client id; the control API already filters by
client.
