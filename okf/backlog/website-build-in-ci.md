---
title: Nothing builds the website in CI
description: A broken site build goes unnoticed until someone runs make build locally; the components theme.ts headless-import break sat undetected from the colord removal until the next manual rebuild.
created: 2026-08-19
---

# Nothing builds the website in CI

`make -C website build` is the only thing that exercises the site generator,
its pull directives, and the headless-import path of everything the build
script touches (`packages/components/src/theme.ts` via `website/src/tokens.ts`,
the docs/ trees, the declaration splitter). None of it runs in CI, so a break
surfaces on the next manual rebuild, not on the change that caused it.

Evidence: the colord removal made theme.ts call `flux:rendertree` at module
init, which no plain flux binary resolves. The site build was broken from that
commit until the next local `make build` (2026-08-19), which happened to be
unrelated work.

Done looks like: a CI job that catches this class on the PR that introduces
it. Two tiers, cheapest first:

1. Bundle + import smoke, no runtime binary needed: run the `srt bundle`
   step (parse/resolve errors) and `bun -e 'import("./website/src/tokens.ts")'`
   (headless-import regressions of the components theme). Cheap enough for the
   existing js job.
2. Full `make -C website build`, which needs the host flux binary - only
   worth it as part of a job that already builds the runtime, and only useful
   as a gate once flux exits non-zero on error (flux-script-exit-code.md);
   today the directive and coverage guards are advisory prints.

Involves: one CI job/step, no product code. The components docs coverage gate
(`scripts/build-components-docs.ts --check`) already runs in CI and is
separate from this.
