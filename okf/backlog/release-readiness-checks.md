---
type: backlog-item
title: Release readiness checklist and pre-publish artifact checks
status: deferred
timestamp: 2026-07-17T00:00:00Z
---

# Release readiness checklist and pre-publish artifact checks

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
  in flux-types and docs/flux.md; today this is a manual convention with no
  check.
- `srt check` clean on the scaffold app; components strict-clean (postmortem
  4.3 second half) so dependency errors are not hiding app-relevant ones.
- Rust test suites and JS tests green across the workspace.
- No product names or private notes in anything that ships (okf/product is
  gitignored, but artifacts should be scanned too).
- Versioning preconditions: all versions still `0.0.0` placeholders as
  release.yml expects (do not hand-bump), changelog/release notes exist.

## Candidate pre-publish checks

- `npm pack` (or registry dry-run) each package and inspect contents: files
  present, no strays, exports resolve.
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