---
title: Fonts as pack-time payload
description: The production runtime went font-free and every pack carries its fonts as manifest assets, the three Noto role defaults included, bound to role aliases by the package.json solidrt.fonts map.
created: 2026-07-20
completed: 2026-07-20
---

# Fonts as pack-time payload

The default fonts left the production runtime so a packed app can swap,
add or drop fonts without a runtime rebuild: `sol pack` carries them the
way it carries the app's bytecode. Implemented 2026-07-20, first as font
sections in the pack trailer registered from memory; the storage stages of
[client-storage-updates](../plans/client-storage-updates.md) turned them
into manifest assets the same day.

Superseded in part by
[runtime-default-fonts](../backlog/runtime-default-fonts.md): with custom
runtimes, the defaults move back into the runtime and a pack ships only
the fonts its runtime lacks.

## What landed

- The production runtime `solidrt` embeds no fonts; its font set is the
  payload's. `solidrt-go` (the dev client) embeds the three Notos as its
  base set (`lattice::embedded_fonts()`, `include_bytes!` from
  `alloy/assets/fonts/`): it is never packed, and the dev loop (player,
  BSOD, stats HUD, `sol render` frames) needs text that matches a default
  pack.
- Fonts are configured by the `solidrt.fonts` map in package.json, never
  by CLI flags: dev pushes, `sol render` and every pack read the same key.
  Each key is a registration alias:

  ```json
  "solidrt": {
    "fonts": {
      "sans": "./assets/fonts/Inter.ttf",
      "serif": true,
      "mono": false,
      "display": "./assets/fonts/Fancy.ttf"
    }
  }
  ```

  A role with a path replaces that role's default, `false` drops it,
  `true` or leaving the role out keeps it, any other key adds a face under
  that alias. Font files live under `assets/`. The scaffold writes the
  three roles as explicit `true`s so dropping one is a one-word edit.
  Resolution in `packages/cli/src/lib/fonts.ts`, shape checks in
  `lib/project.ts`; a missing file or a wrong value type fails naming the
  key.
- Fonts are manifest assets. The manifest's `fonts` array is the only home
  of the alias binding: `{ path, alias }` for a file, `{ alias }` for a
  dropped role. Every pack output (the single-file exe, `--folder`, the
  `.solapp` of `--app`, the `app.solapp` inside an `--apk`) materializes
  the defaults under `assets/fonts/` (`pack/layout.ts`) and lists the full
  set; the exe trailer carries manifest-named files, with no font section
  kind. A dev push carries only the project's own font files and its
  `false` annotations, and `lattice::merge_fonts` layers them over the
  client's base set on every app switch: an alias the manifest names
  replaces or drops the base font under it.
- The default font files: `alloy/assets/fonts/` in a checkout (tracked),
  `fonts/` in the published CLI package (staged by release.yml), looked up
  through `SOLIDRT_HOME` first.
- fluxrt is untouched: headless, no fonts.

## Decisions

- The role names are aliases: `"sans"`, `"serif"`, `"mono"`. Whatever is
  bound to the sans role is "sans"; the "Noto Sans" literals and the flux
  alias maps were deleted, and text's default family is "sans".
- The role set mirrors the three real typographic roles. The other CSS
  generic families were rejected: cursive and fantasy are decorative
  grab-bags (pack a specific font under its own name), system-ui is the
  opposite of the consistency default, math and emoji are glyph coverage,
  not roles.
- All three roles default to Noto (Sans, Serif, Sans Mono), so a default
  app renders identically on every platform.
- Every face beyond the roles is selectable via fontFamily, by its alias
  or by the family names in its file. One face per name: the first
  registration of an alias or family name wins (`FontSet::register`), so
  two typefaces need two aliases. Weights and widths come from a face's
  variable axes; a static face synthesizes bold and slants for italic.
- An unknown family, or a role the app dropped, resolves to the face
  registered as sans, else to the first face. The text engine never
  consults the platform's fonts (since
  [text-own-rasterizer](text-own-rasterizer.md)): an app that drops every
  role and registers nothing draws no text, and a character no registered
  face covers draws as notdef. Under Impeller in July, an empty or
  incomplete set fell back to fontconfig/DirectWrite/CoreText; the routes
  to detect a system mono considered then are moot.

## Caveats

- Coverage: the Notos cover neither emoji nor CJK. An app that needs them
  registers a face that does; per-character fallback walks every
  registered face.
- Size (measured 2026-07-20): the Notos are large because they are
  variable. gvar (axis deltas) is 61-74% of each file, glyf 16%.
  zstd -19 brings a file to 49%, brotli -11 to 45%; WOFF2 reaches 43% and
  needs a decoder plus sfnt reconstruction for 1.5 points over brotli, so
  it was rejected. zstd over pack sections stays a named future for when
  packed size presses (transit is already compressed). Instancing the
  wdth axis out of the defaults (2.0 -> 1.4 MB a font) is off the table
  since `fontStretch` drives that axis
  ([font-stretch-axis](font-stretch-axis.md)).

## Verification (2026-07-20, Linux)

Packed apps rendered the three Noto roles by default (the same files as
the old embeds), a `sans` override, `serif` by default in both the pack
and the dev client (frames pixel-equivalent), added aliases including a
CFF `.otf`, and an all-`false` pack (drawn from the system default under
Impeller then). Config errors exited 1 naming the key, and the trailer
bytes were checked with an independent parser. Two checks were dropped,
not run: a byte-diff against a pre-change golden (none was stored), and
the Windows system-font probe (moot once the text engine stopped
consulting platform fonts).
