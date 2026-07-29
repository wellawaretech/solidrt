---
type: backlog-item
title: Guard that every referenced example ships
description: A committed examples README can name an example file that is untracked, so the doc ships and the file does not; a release-time parity check would catch it.
status: open
timestamp: 2026-07-29T00:00:00Z
---

# Guard that every referenced example ships

Raised by the shadertoy field report
(projects/shadertoy/SOLIDRT-FEEDBACK.md #5). `packages/core/examples/README.md`
in the installed 0.0.39 package listed:

> `gpu-raw-program.tsx` - the raw shading layer: compileShader/linkProgram/
> createShaderTarget, one vertex stage shared by two programs, with and
> without the standard header.

That was the single example most relevant to what they were building, so they
went looking for it on the strength of the README. It was not in the tarball.

## Mechanism

Not a packaging bug. `packages/core/package.json` lists `examples/` in `files`,
so the directory does ship - but `gpu-raw-program.tsx` was never `git add`ed.
The release workflow builds from a fresh checkout, where an untracked file
simply does not exist, while the README naming it was committed long before.
The result is a published doc pointing at a file that cannot be there.

Fixed for this instance on 2026-07-29 (the file is now tracked). Diffing every
`.tsx` named in `packages/core/examples/README.md` against `git ls-files`
showed it was the only mismatch at the time, so nothing else is currently
broken - but nothing prevents the next one either, and the failure is silent on
the publishing side and only visible to someone who already trusted the doc.

## Proposed shape

A check over each package that ships an `examples/` directory: every example
filename referenced in that package's `examples/README.md` must exist in the
packed output (or, more cheaply, in `git ls-files`). Fail the release on a
mismatch.

Cheapest useful placement is `.github/workflows/release.yml`, alongside the
existing guard that verifies `bun publish` baked the resolved workspace
specifiers into each tarball - same shape of problem (the published artifact
differing from what the source implies) and the same place to catch it. Running
it in `ci.yml` instead would surface it earlier, at the cost of not inspecting
the real tarball.

Worth extending to the reverse direction too if it is free: an example file
that no README mentions is not a shipping bug, but it is usually an oversight.

Related: [[examples-rescope]] in okf/plans for the wider examples-corpus work.
