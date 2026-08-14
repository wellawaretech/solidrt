---
title: Release readiness and pre-publish checks
description: A pre-build readiness gate (types and runtime in lockstep, srt check, tests, version placeholders) plus post-build artifact checks before the irreversible npm publish.
created: 2026-07-27
---

# Release readiness and pre-publish checks

There is no defined procedure between "the tree looks good" and "packages are
on npm". Two gates are missing, and both need to exist before the first real
release is attempted:

1. **Pre-build readiness** - a checklist (ideally automated, `srt`-style
   command or CI job) that must pass before release artifacts are even built.
2. **Post-build, pre-publish sanity checks** - once artifacts exist, verify
   them as a consumer would, and only then publish to npm. Publishing is the
   irreversible step; everything before it is cheap.

## Candidate pre-build checks

- Types/runtime lockstep (postmortem 5.2): every flux surface change mirrored
  in flux-types and docs/flux.md; the flux-types/docs mirroring is still a
  manual convention with no check. Partly covered since 2026-07-25: CI
  (.github/workflows/ci.yml) typechecks core/components/cli/cli-server/
  launcher and runs `srt check` on every examples/* app per PR/push.
- `srt check` clean on the scaffold app; components strict-clean (postmortem
  4.3 second half) so dependency errors are not hiding app-relevant ones.
  CI covers the examples apps but not the scaffold itself.
- Rust test suites and JS tests green across the workspace.
- No product names or private notes in anything that ships (the private notes
  live outside the repository entirely now, but artifacts should be scanned
  anyway).
- Versioning preconditions: all versions still `0.0.0` placeholders as
  release.yml expects (do not hand-bump), changelog/release notes exist.

## Candidate pre-publish checks

- `npm pack` (or registry dry-run) each package and inspect contents: files
  present, no strays, exports resolve. Partly covered: release.yml now
  packs every publish dir (`bun pm pack`), extracts, and fails the release
  if any `@solidrt/*` specifier is off the release version, before publish.
  Contents/exports inspection is still missing.
- Scaffold a project against the packed tarballs (not the monorepo links) and
  run `bun install` + `srt check` + a smoke run; this exercises the pinned
  intra-monorepo deps that release.yml rewrites, which by design cannot work
  in-repo.
- Client binaries: launch each shipped triple against the dev server, confirm
  version/build info reports the release (client-build-info backlog item
  relates), render one frame.
- Only after all of the above: publish, then immediately verify the published
  versions install and run from the registry.

Open questions for when this is picked up: where the automation lives (CI
only, or an `srt release-check` runnable locally), what is blocking vs
advisory, and how the checklist stays in sync with new surfaces (same
lockstep problem as 5.2 itself, one level up).

## Harden the publish job against the dependency graph

Separate from "are the artifacts good": `NPM_CONFIG_TOKEN` is live in the
publish job, and that job also runs `bun install`. A malicious or compromised
package anywhere in the dependency graph runs with the token in the
environment and can exfiltrate it. A publish token is worth more than
anything else in this repo - it is the one credential that lets someone ship
code to every consumer.

Options, in rough order of value per effort:

- isolate publish from build: build and pack in a job with no token, hand the
  tarballs to a second job that installs nothing and only publishes
- scope and time-limit the token (granular, per-package, short-lived) so a
  leak has a small blast radius
- audit the graph harder before publish - lockfile diff review, or a
  provenance/trusted-publisher setup if npm's supports what we need

The first one is the real fix; the others reduce the damage rather than the
exposure. Worth deciding before the first real publish, since the flow is
being defined now anyway.

Source: root TODO.md, migrated 2026-08-14.