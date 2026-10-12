---
title: Default fonts in the runtime, packs ship only what it lacks
description: Every pack carries the three Noto role defaults (5.6 MB) because the production runtime registers no fonts; the runtime should carry them, droppable and replaceable in a custom runtime, and a pack should ship only the fonts its runtime does not have.
created: 2026-10-12
---

# Default fonts in the runtime, packs ship only what it lacks

`sol pack` writes the three Noto role defaults into every pack: Noto Sans
(2.0 MB), Noto Serif (1.9 MB) and Noto Sans Mono (1.7 MB), materialized
under `assets/fonts/` by `packages/cli/src/pack/layout.ts` and bound in the
manifest's `fonts` array. The production runtime `solidrt` registers no
fonts of its own (`lattice/src/entry.rs`: the payload's fonts, else an
empty set), so the pack is the only place they can come from. Every output
carries them: the single-file exe, `--folder`, the `.solapp` of `--app`,
and the `app.solapp` inside an APK.

That was decided when the runtime was one stock binary shipped inside each
app ([packaged-fonts](../done/packaged-fonts.md)): the pack was the only
place an app could change its fonts without a rebuild. Two things have
moved since:

- A `.solapp` runs on a runtime it does not contain, and under the
  [update mechanism](../notes/update-mechanism.md) every installed version
  is one. Each version re-ships 5.6 MB of fonts that do not change between
  versions.
- A project can own its runtime
  ([custom-runtime-authoring](../done/custom-runtime-authoring.md)).
  Changing the defaults no longer needs the pack: a project with its own
  runtime can put its fonts there, and a runtime whose apps bring their
  own faces should not carry the Notos at all.

The dev client already has the target shape. `solidrt-go` embeds the Notos
as its base set (`lattice::embedded_fonts()`), a dev push carries only the
project's own fonts and its `false` annotations, and `lattice::merge_fonts`
layers the manifest over the base. The production runtime is the odd one
out: its base set is empty.

For an exe or an APK the bytes move rather than vanish (runner +5.6 MB,
payload -5.6 MB). The saving is per `.solapp` and per installed version,
and in a custom runtime that drops a role.

## Shape

- **The runtime carries the defaults.** `lattice::Program` gains the base
  font set beside `modules`, `bsod` and `player`. lattice offers the stock
  set (the three Notos it embeds today; the files are tracked in git, so a
  tag dependency builds), which the stock program (`runtime/src/lib.rs`)
  and the scaffold's `lib.rs` use. Both binaries register the program's
  set, and `embedded_fonts()` as a go-only helper goes.
- **A custom runtime edits that set in its `lib.rs`**: drop a role,
  replace one with its own file, add faces. Roughly
  `lattice::Fonts::stock().without("mono").with("sans", include_bytes!("../fonts/Brand.ttf"))`;
  the exact API is decided when it is built.
- **Run time is one rule.** The production runtime merges the payload's
  manifest fonts over the program's set with `merge_fonts`, as the dev
  client merges a dev push: an alias the manifest names (a file, or
  `false`) replaces or drops the base font under it, every other base font
  stays.
- **Pack ships only what the runtime lacks.** The app's `solidrt.fonts`
  map keeps its shape and is resolved against the runtime the pack is for:
  - a role left out, or `true`: the runtime's font for that role. Nothing
    ships when the runtime binds the role; when the runtime dropped it,
    pack ships the stock Noto, so no app loses a role to the runtime it is
    packed for. A runtime that replaced a role thereby changes what the
    default is for its apps.
  - a file path: the file ships, and takes the alias over at run time.
  - `false`: an annotation only; the runtime's font for that role is
    dropped.

  A default app packed for the stock runtime then carries no font files.
  The `>> fonts:` line says which fonts ship and which come from the
  runtime.
- **The dev client shows what the pack will.** A go build registers the
  program's set plus the stock Noto for every role the program does not
  bind, which is exactly what a pack adds for those roles. Dev pushes stay
  as they are.

## Open questions

- **How pack learns the runtime's set.** For the stock runtime the CLI
  knows it: the two ship together. For a custom runtime, pack has to read
  it off the runtime it resolved (`packages/cli/src/lib/runtime.ts`), and
  packing is cross-target (`--apk` packs an Android runner on a desktop
  host), so running the runner cannot be the only channel. Lean: a listing
  beside the binaries in the runtime's dist dir (the aliases it binds),
  which the stock platform packages ship too. Still open is who writes it:
  `runtime.mk` from the host build answering `--fonts` (`lib.rs` stays the
  one declaration, and the set is per program, not per target), or
  `build.rs` from a font table the runtime declares as data, with the
  embeds generated from that table.
- **A role the runtime dropped.** The shape above ships the stock Noto for
  it unless the app says `false`, so dropping a role from the runtime only
  saves bytes when the app opts out too, or brings its own face. The
  alternative (left out means whatever the runtime has, `true` means
  ensure the stock default) avoids that double declaration, but gives
  `true` and a missing key different meanings while the scaffold writes
  explicit `true`s.

## Done looks like

- Stock `solidrt` registers the three Notos; a pack of an app without a
  `solidrt.fonts` map writes no font files in any output (exe, `--folder`,
  `--app`, `--apk`) and renders as before.
- A custom runtime can drop, replace and add fonts in its program; a pack
  for it ships the stock Noto only for a role the runtime lacks and the
  app keeps.
- The dev client renders a project's text as its pack does, for the stock
  runtime and for a custom one with a dropped and a replaced role.
- Tests: the production runtime's merge of a payload over the program's
  set (lattice), pack font resolution against a runtime's set (CLI).
- Docs: `packages/cli/agents/assets.md` (where the defaults come from),
  the custom-runtime docs (the font set in `lib.rs`).
