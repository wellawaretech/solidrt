---
type: backlog-item
title: Engine-side HTTP disk cache
status: planned
timestamp: 2026-07-17T00:00:00Z
---

# Engine-side HTTP disk cache

Design decided 2026-07-17; decisions and staging live in
[the plan](../plans/fetch-cache.md). Summary: explicit opt-in per fetch call
(`cache: "force-cache" | "reload"`, standard names with documented
app-runtime semantics); the default stays uncached like Node/Bun/Deno;
server cache headers are ignored entirely. This supersedes the
cache-control-honoring framing this note originally had: solidrt apps target
known resources, so caching policy belongs at the call site, not in headers.

Motivation:
- kaas postmortem (2.2 related note, 2026-07-16): every hot reload drops the
  JS-side image cache, so dev iteration re-hammers remote hosts. `--proxy-http`
  covers this in dev, but only when enabled.
- earcraft (2026-07-16): fetches many images in normal operation and wants
  them cached across sessions - a production need `--proxy-http` cannot serve.
- image review (2026-07-17): core's `createImage` accumulated image-only
  loading policy (fetch gate, failure cache) that generalizes to all fetches
  and belongs in the engine fetch layer.