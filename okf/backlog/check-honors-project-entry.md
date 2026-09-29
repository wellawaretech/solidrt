---
title: Make srt check honor solidrt.entry
description: Folder-mode srt check discovers the app entry from a hardcoded src/index.tsx glob and never reads solidrt.entry, so a project with a declared entry is either not found or checked against the wrong file.
created: 2026-09-29
---

# Make srt check honor solidrt.entry

## Symptom

A project that declares its entry in package.json:

```json
"solidrt": { "entry": "app/main.tsx" }
```

runs, bundles and packs from that entry, but a bare `srt check` (or
`srt check <folder>`) does not look at it:

- With no `src/index.tsx` in the project it fails with "No entries found
  under ...", although the project has a valid entry.
- With a stale `src/index.tsx` next to the declared entry it checks that
  file instead and can print "Check passed" without ever having built or
  typechecked the real app. This is the worse shape: the gate reports
  green on code that was not checked.

`srt check <file>` is not affected, and neither is the dev server's
startup check, which spawns `srt check <entry>` with the resolved entry.

## Cause

Folder mode takes its entries from `CHECK_ALL_GLOBS` in
`packages/cli/src/check/main.ts`, whose app entry is the literal
`src/index.tsx`, and whose example apps are the literal
`examples/*/src/index.tsx`. `checkEntry` does load the project
(`findProject`), but uses only its `dir`.

Every other command resolves the entry through `resolveMode` in
`packages/cli/src/lib/mode.ts`: `solidrt.entry`, defaulting to
`src/index.tsx`. Check is the one command that walks up from each entry
to its project instead of taking the cwd, which is why it does not go
through `resolveMode`, and how the entry lookup got left behind.

## What done looks like

- `srt check` in a project checks the entry `srt run` would run:
  `solidrt.entry` when declared, `src/index.tsx` otherwise.
- The same holds per example app: an `examples/*/` directory with its own
  package.json is checked at its declared entry.
- A declared entry that does not exist fails the check with the same
  "Entry not found" message the other commands give, instead of being
  skipped.
- The single-file globs (`examples/*.tsx`, `packages/*/examples/*.tsx`,
  `packages/*/demos/src/*.tsx`) are unchanged.
- The default entry path is defined once (`DEFAULT_ENTRY` in mode.ts,
  exported) rather than repeated in the glob list.
- `packages/cli/src/check/docs.md` and the comment above
  `CHECK_ALL_GLOBS` say "the project's entry" instead of `src/index.tsx`.

## Open question

A folder that relies on the default and has no `src/index.tsx` is skipped
silently today, which is right for a monorepo root (it usually has no app
of its own) and wrong for an app project whose entry went missing. One
way to tell them apart: a package.json with a `solidrt` key is an app and
a missing entry is an error; one without is skipped.
