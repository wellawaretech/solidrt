---
title: Honor binary and text import attributes in flux's module loader
description: Make `with { type: "binary" | "text" }` a runtime feature of flux, so bare flux runs asset imports and the bundler's inlining becomes ahead-of-time evaluation of the same contract.
created: 2026-10-14
---

# Honor binary and text import attributes in flux's module loader

## Problem

`import data from "./pic.png" with { type: "binary" }` works only through
the sol bundler, which rewrites it into an inline Uint8Array
(packages/cli/src/bundle/bundler.ts, `inlineImport`). Bare flux
(`flux script.ts`, the tty repl) fails on the import, so the asset-import
contract is the bundler's, and its type declarations ship with the cli
(`packages/cli/assets.d.ts`), not with `@solidrt/flux-types`. That split is
correct today but it is a seam: the same source means one thing bundled and
another thing on the runtime it is written for.

## Done looks like

- flux's module loader resolves an import carrying `with { type: "binary" }`
  to a module whose default export is the file's bytes (Uint8Array), and
  `with { type: "text" }` to its UTF-8 contents, relative to the importing
  module, the way the bundler resolves them. Any extension, as in the
  bundler; `.svg` with no attribute stays a bundler convenience unless the
  loader can carry it too.
- The bundler keeps inlining (zero I/O in a shipped app) and its output is
  observably the same as the runtime's loading, so a file runs the same bare
  and bundled.
- The asset declarations move from `packages/cli/assets.d.ts` to
  `@solidrt/flux-types`, where a runtime feature's types belong, and the
  `"@solidrt/cli"` entry leaves the scaffold tsconfig `types`.

## Involves

- Whether rquickjs/QuickJS-NG hands import attributes to the module loader
  (QuickJS gained import attributes in 2024; the rquickjs loader API has to
  expose them). If it does not, this waits on upstream.
- The loader side in flux (`flux/src`, the module resolver/loader): read the
  file, build a synthetic module with one default export. Keep it in the
  standards layer: import attributes are a web standard, `"binary"`/`"text"`
  are the Bun vocabulary the bundler already speaks.
- A flux module test per attribute, and a check that `sol check` and the
  editor agree once the declarations move.
