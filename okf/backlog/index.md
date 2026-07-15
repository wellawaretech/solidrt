---
type: bundle-index
title: Backlog
description: Deferred features and ideas, one file per item, picked up here when someone has time.
timestamp: 2026-07-13T00:00:00Z
---

# Backlog

- [GPU pipeline extensions](gpu-pipeline-extensions.md) - typed (vec/mat4)
  uniforms, index buffers, float data textures, blending, multi-pass targets
  on top of the minimal createPipeline. Status: deferred.
- [stdin/tty support in flux](stdin-tty-support.md) - raw-mode terminal
  input as a flux capability, useful standalone (not just for the CLI).
  Status: deferred.
- [CLI/flux migration](cli-flux-migration.md) - move the srt CLI (REPL + dev
  server) fully into flux, keep Bun only as an external bundler subprocess.
  Depends on stdin/tty support for the REPL piece specifically. Status:
  deferred.
- [Runtime-side sourcemap remapping](runtime-sourcemap-remap.md) - remap
  stack frames in the runtime itself so the local terminal and logcat show
  .tsx positions too. Explicitly NOT to be implemented unless the current
  server-only remapping proves insufficient in practice. Status: deferred.
- [Snapshots depend on a frame happening](snapshot-requires-next-render.md) -
  captureSnapshot / get_snapshot latch a frame request but do not wake the
  render loop; works today only because the idle Tick still fires. A truly
  idle client (true JS idle, paused/backgrounded window) never services the
  capture and the query times out. Fix: make request_frame actively wake the
  loop. Status: deferred.
- [MCP input injection](mcp-input-injection.md) - synthetic key/pointer
  events to clients (plus a snapshot-diff helper), so an agent can navigate,
  capture, and verify visuals without a human ferrying the app around.
  Status: deferred.
- [App-registered debug commands](mcp-debug-commands.md) - `srt:dev`
  registerDebug + MCP list_debug/call_debug, replacing the debug-keys +
  get_logs pattern for querying/poking a running app. Status: done
  (2026-07-15; async commands still unsupported).
- [GPU resource inspection](mcp-gpu-resource-inspection.md) - MCP readback of
  textures (as PNG), buffer ranges, and pipeline state (draw counts, last
  uniforms), because a one-pipeline app hides everything from the render
  tree. Status: done (2026-07-15; depth-attachment readback still deferred).
- [onFrame tick reset on reload](onframe-tick-reset-on-reload.md) - the tick
  timebase resets across hot reload after the new instance's first frame,
  handing apps one enormous negative dt (teleports/frozen accumulators).
  Status: deferred (apps clamp dt to [0, cap] as a workaround).
- [Dev-state KV across reloads](dev-state-across-reloads.md) - host-owned
  per-client store (`flux:dev` devState) so apps can restore pose/UI state
  after hot reload instead of resetting to start. Status: deferred.
- [Client build info in list_clients](client-build-info.md) - git hash /
  version / profile per connected client, so "does this binary have my
  engine fix" is checkable. Status: done (2026-07-15; build timestamp +
  HEAD-staleness comparison still deferred).
- [Android APK packaging for flux:ffi libraries](ffi-android-apk-packaging.md) -
  ship an app's ffi libraries in an asset folder, packaged into the APK's
  native-lib dir and opened by path automatically (byte-loading from the dev
  server is blocked by Android W^X policy). Part of the future APK packaging
  work. Status: deferred.