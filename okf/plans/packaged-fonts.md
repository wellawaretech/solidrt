---
title: Fonts as pack-time payload
description: The solidrt runtime goes font-free and srt pack appends fonts as trailer sections, with the three Noto role defaults declared through the package.json solidrt fonts key.
created: 2026-07-20
---

# Fonts as pack-time payload

Decouple the default fonts from the runtime binary so a packed app can swap,
add, or drop fonts without rebuilding: fonts are appended by `srt pack` the
same way the JS bytecode is. Draft under discussion 2026-07-20. Sequencing
decided: fonts ship first, ahead of the client-storage/update work, with an
interim in-memory registration path and a recorded migration.

## Decisions

- The `solidrt` runtime binary ships no font data. The `include_bytes!`
  Notos leave alloy; the binary shrinks ~3.7 MB (38.5 -> ~34.8 MB).
- `srt pack` appends fonts as trailer sections. By default it appends
  three Noto files (Sans, Serif, Sans Mono), so a default packed app
  renders identically on every platform (consistency is preserved by
  default, now via packaging).
- Fonts are optional: an app can override them per role, add more, or
  drop the defaults if it does not care about cross-platform consistency
  or renders no text.
- Font declaration lives in project config (the `solidrt` key in
  package.json, per the update-mechanism design), NOT in CLI flags. Three
  consumers need the same font set - dev server push, future `srt
  publish`, and `srt pack` - and flags can only feed pack. One source of
  truth; no `--font`/`--no-default-fonts` flags.
- With no font registered, Impeller falls back to the platform font
  manager (fontconfig/DirectWrite/CoreText). Verified on Linux via
  alloy/examples/system_font_probe.rs: an empty TypographyContext renders
  real glyphs for every family name, known or unknown; exact system
  family names match; generic names ("sans-serif", "monospace") do NOT
  resolve and fall back to the proportional default.
- `solidrt-go` (dev client) keeps the Notos compiled in - all three,
  including the new Noto Serif (+1.9 MB): it is never packed, and the dev
  loop (default screen, BSOD, HUD, `srt render` golden frames) needs
  deterministic text matching a default packed app. The embed moves out
  of alloy into the go-client build. `make download-fonts` gains the Noto
  Serif file. Once fonts ride the dev push as assets (see Alignment),
  the embedded set becomes the no-app/no-config fallback.
- Family vocabulary: the role names `"sans"`, `"serif"`, `"mono"` become
  registered alias names themselves (Impeller's register_font alias
  parameter). The hardcoded "Noto Sans"/"Noto Sans Mono" strings and the
  sans/mono alias maps disappear; whatever font is bound to the sans role
  IS "sans". An unbound role falls through to the system default
  (graceful, verified). Uniform rule, no role is special-cased: bind a
  role or accept the platform default.
- Role set is `sans | serif | mono`, mirroring the three real typographic
  roles; the other CSS generic families are rejected: cursive/fantasy are
  decorative grab-bags (pack a specific font under its real name),
  system-ui is the opposite of our consistency default (unset family
  already approximates it via system fallback; possible named future for
  native-feeling apps), math/emoji are glyph-coverage concerns, not roles.
- All three roles get default payloads: Noto Sans, Noto Serif, Noto Sans
  Mono. A default packed app carries ~5.6 MB of fonts.
- Multiple fonts beyond the roles are first-class: every declared font is
  selectable in the app via fontFamily. Two DIFFERENT typefaces cannot
  share one alias: Impeller merges same-family registrations into one
  family and style-matches across its faces (that is the intended use:
  registering bold/italic FACES of one family). Distinct fonts are
  selected by distinct names - their intrinsic family name, or a custom
  alias. Role names are just conventional aliases; the alias namespace is
  free-form.
- fluxrt is untouched: headless, no gui feature, no fonts. Its single
  payload trailer format stays as is.

## Alignment with update mechanism / client storage

Per okf/notes/update-mechanism.md: assets are loose content-addressed
files on disk under `apps/<app-id>/versions/<v>/`, the version manifest
enumerates them, and single-file artifacts (a packed binary) are
immutable install wrappers at the edges, never storage. Fonts are assets
under that model, with one twist: a native consumer at startup plus role
metadata. Alignment decisions:

- The version manifest gains a `fonts` array: asset entries annotated
  with an optional alias (`{ path, hash, size, alias? }`). That is the
  authoritative home of the alias metadata; the trailer's per-section
  alias field is the packed serialization of the same annotation.
- End state: a packed binary's first run seeds the version store on disk
  (the install-wrapper role), fonts register from files in the version
  dir, and the dev server pushes font files like any asset - which
  closes the dev/prod font gap (custom fonts visible in dev) for free.
- Interim (fonts ship first, no version store exists): the packed runner
  registers trailer fonts straight from memory, no disk writes.
  register_font wants whole bytes in memory anyway (no seek/stream
  need), so the observable behavior is identical; only the resting place
  changes later. Migration = replace "slice from trailer" with "read
  from version dir" when client storage tier 1 lands; the alias
  plumbing, registration code, and config surface all carry over.

## Trailer format (solidrt only)

Replace the single-payload trailer with sections. After the runner image:
each section's bytes, then a table of section entries, then entry count,
then magic. The magic stays `SOLIDRT\x88\x44` unchanged: CLI and runners
ship pinned together so no format versioning is needed, and the parser
bounds-checks every table offset/length so a stale runner meeting a
new-format trailer (contributor SRT_HOME mismatch) degrades to "no
payload" instead of misparsing.

Two section kinds, with per-section alias metadata instead of one kind
per role (adding a role later costs nothing, arbitrary aliases fall out
free):
- `bytecode` - compiled JS (required for a packed app)
- `font` - repeatable; carries an optional alias string, mirroring the
  future manifest `fonts` annotation. With an alias
  ("sans"/"serif"/"mono" by convention) the font registers under that
  alias; without one it registers under the family name in the font file.

Table entry shape: kind u32, offset u64, len u64, alias len u8 + alias
bytes (len 0 = no alias). Exact encoding settled at implementation time.

## Config surface

The `solidrt` package.json key gains a `fonts` map: alias -> font file
path, nothing else. `srt pack` reads it (dev push and publish read the
same key later).

```json
"solidrt": {
  "fonts": {
    "sans": "./fonts/Inter.ttf",
    "mono": false,
    "display": "./fonts/Fancy.ttf"
  }
}
```

The map merges over the defaults. Each key is the registration alias;
role names are just conventional keys:

- no `fonts` key: the three Noto defaults
- a role key with a path replaces that role's default (`"sans"` above:
  Inter instead of Noto Sans)
- a role key with `false` drops that role's default (`"mono"` above: no
  mono font packed); setting all three roles to `false` packs no
  defaults at all
- any other key adds a font under that alias
- unmentioned roles keep their Noto defaults (`"serif"` above)

Anything finer (intrinsic-name registration without an alias, etc.) is
deliberately not designed now; revisit when the client-storage plan
executes and the manifest `fonts` array becomes the authoritative home.

Default font resolution at pack time: `SRT_HOME/alloy/assets/fonts/` for
contributors; the published CLI package carries its own copy of the three
Noto files (platform-independent, so the CLI package is the channel, not
the per-platform binary packages; ~5.6 MB on the CLI package).

## Runtime side

- Interim unpacking is in-memory (see Alignment): the runner fs::reads
  its own executable image at startup (it already does this today for
  bytecode), slices each font section out as an owned Vec<u8>, and hands
  the bytes straight to Impeller's register_font (Cow<'static, [u8]>,
  parses from memory). Cost is the font bytes living on the heap
  (~5.6 MB default) instead of in file-backed executable pages as with
  include_bytes - acceptable, and the whole-image read is transient.
  End state after client storage lands: first run writes the payload
  into the version store, fonts register from disk files.
- main.rs parses the section table, hands lattice the bytecode plus a
  list of (alias, bytes) font payloads.
- PlatformContext::new takes font payloads and registers each: aliased
  fonts under their alias, plain fonts under their intrinsic name.
- Defaults switch: kinds/text.rs default family "sans"; gui/tree.rs and
  gui/properties/text.rs alias maps removed; overlay HUD "mono";
  examples/terminal "mono"; packages/core types.d.ts fontFamily union
  gains "serif".

## Caveats (documented, not blocking)

- Variable fonts: the Notos carry wdth/wght axes; a static replacement
  TTF snaps weights or fake-bolds (see alloy/examples/weight_axis.rs).
  Warn in pack docs.
- Dropping the defaults: "mono" has no reliable system fallback (generic
  names do not resolve), so the stats HUD and mono text render
  proportional. Cosmetic. Detecting the system mono IS possible, just
  not through Impeller (TypographyContext exposes only register_font: no
  enumeration, no generic-family resolution; generics are a browser
  convention resolved above the font manager). Named future, two viable
  routes if a defaults-off app ever wants real mono: (1) resolve the
  default mono family name natively per platform (fontconfig "monospace"
  match, DirectWrite IsMonospacedFont, CoreText monospace trait, Android
  Roboto Mono) and use it as a literal family; (2) probe well-known
  candidate names and detect a match via Impeller metrics alone
  (candidate metrics differ from default fallback = matched; "iiii" vs
  "WWWW" equal width = fixed pitch). Too disproportionate to build now
  for the opt-out corner case. SDL is no help here (checked 2026-07-20):
  core SDL3 has no font API (only the 8x8 debug text), and SDL_ttf opens
  fonts by path only - no system enumeration or family matching - while
  dragging in a second FreeType/HarfBuzz text stack beside Impeller's
  typographer. Rejected.
- Glyph coverage is a separate open topic: the Notos cover neither emoji
  nor CJK. Whether Impeller does per-glyph fallback to system fonts is
  unprobed; if not, those are tofu today regardless of this plan.
  Deserves its own probe (extend system_font_probe.rs) before any
  coverage promise.
- Dev/prod gap (interim only): dev shows the embedded Notos; a custom
  font only shows in the packed app. Closes automatically when fonts
  ride the dev push as manifest assets (see Alignment).
- Android: not affected today (`srt pack` is desktop-only; the APK uses
  the dev-client model). When Android production packaging happens,
  fonts ride as APK assets, no current_exe trick.
- Size (measured 2026-07-20): the Notos are big because they are
  variable - gvar (axis deltas) is 61-74% of each file, glyf only 16%.
  General-purpose compression halves them (zstd -19 49%, brotli -11 45%
  of original); WOFF2 reaches only 43% and Impeller cannot read it, so
  it is rejected (decoder + sfnt reconstruction for 1.5 points over
  plain brotli). Named future: zstd compression of trailer sections as
  a generic knob (would squeeze bytecode too; one decoder dep, ~15 ms
  startup) - build when packed-size pressure is real; transit is
  already compressed (npm tarballs, release archives), so this only
  shrinks the resting packed binary. The bigger lever - instancing the
  unreachable wdth axis out of the default fonts (2.0 -> 1.4 MB per
  font, zero runtime cost) - is deliberately NOT decided here: it is
  mutually exclusive with ever exposing fontStretch on the defaults,
  see okf/backlog/font-stretch-axis.md; whichever lands second decides.

## Verification checklist (run 2026-07-20, Linux)

- [x] Packed app with default fonts renders the three Noto roles (same
      font files as the old embeds, so shaping is unchanged)
- [x] Packed app with a `fonts.sans` override renders the custom font
- [x] "serif" renders Noto Serif by default in packed app and dev client
      (frames pixel-equivalent between the two, as designed)
- [x] Added fonts (non-role keys) selectable via fontFamily by alias;
      verified with a CFF .otf file - OTF parses fine (Skia sfnt/CFF)
- [x] App with all three roles set to `false` renders via system
      default, HUD legible (proportional, as documented)
- [x] Bare unpacked solidrt (no trailer) still runs the default screen
      via system fallback
- [x] Trailer bytes verified against the spec with an independent
      parser (section offsets/lengths/aliases, table consumed exactly)
- [x] Config errors are loud: missing font file and non-string/false
      values exit 1 with the offending key
- [ ] `srt render` golden diff against a pre-change frame (go client
      embeds are the same files; default screen re-rendered fine, no
      stored golden to byte-diff)
- [ ] System-font probe on Windows (winbox) before documenting the
      defaults-off path as a promise

## Status

Approved and implemented 2026-07-20 (interim in-memory stage; the
version-store migration under Alignment stays future). Update
2026-07-20: client-storage stage 3a landed the dev half of the
Alignment migration - the version manifest carries font annotations
({path, alias} into assets/), dev pushes install custom fonts into
the version store, and a store boot registers them over the embedded
defaults (restart-visible; live registration still out). Custom
fonts now must live under assets/. Stages 3b + 3c (same day)
complete the Alignment migration: `srt pack --folder` materializes
the full font set (defaults included) into the folder's
assets/fonts/, the single-file trailer carries the same files as
manifest-named sections (the font section kind and its alias field
are GONE - the manifest fonts array is now the only alias home, as
this plan forecast), and the runner registers fonts from the
manifest annotations in both forms. The interim in-memory
registration path is fully retired; solidrt-go's embedded Notos
remain the dev fallback. Sequencing:
fonts first, ahead of client storage. Landed: font-free solidrt
(embeds moved out of alloy; PlatformContext::new takes FontPayloads),
sectioned trailer (writer packages/cli/src/packer.ts, parser
lattice/src/main.rs), `solidrt.fonts` config map
(packages/cli/src/fonts.ts), role aliases everywhere (alias maps and
"Noto Sans" literals deleted), Noto Serif added (download-fonts +
go-client embed + CLI default), fonts staged into the published CLI
package by release.yml. Verification per the checklist below; Windows
probe still pending on the winbox.
