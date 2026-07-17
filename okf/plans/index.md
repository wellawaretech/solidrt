---
type: bundle-index
title: Plans
description: Concrete implementation plans for decided work, one file per item; staging, decisions made, and current status.
timestamp: 2026-07-16T00:00:00Z
---

# Plans

- [Documentation website](website.md) - monorepo `website/`, static
  generation by a flux script (converter ported from the standalone
  `~/solidrt/docs` experiment), nav Start-Core-Frameworks-Tools-Runtime-
  Architecture (News deferred), Core-first Start page (no landing switcher),
  per-section Examples + Reference sub-shape, generate-what-we-can content
  (API ref from types, examples from repo, CLI ref); staged: skeleton +
  hand-written pages + examples generator first. Status: active.
- [Fetch disk cache](fetch-cache.md) - explicit opt-in caching in the forge
  fetch core (`cache: "force-cache" | "reload"`, default stays uncached like
  Node/Bun/Deno, server cache headers ignored); per-app data root store, LRU
  size cap; stage 2 adds GET coalescing + per-host limits and shrinks
  core's image.ts back to decode + texture refcounting. Status: stage 1
  done (2026-07-17), stage 2 open.
