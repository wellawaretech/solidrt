---
title: flux exits 0 on an uncaught top-level error
description: The flux binary always exits 0, so a script that throws still reports success; any flux script used in a Makefile, a build step or CI cannot fail, and callers have to grep output to detect an error.
created: 2026-08-18
---

# flux exits 0 on an uncaught top-level error

`flux script.js` reports success no matter what the script does. Verified for
all three shapes:

```
$ flux sync.js     # throw new Error("boom")
Error: boom
exit: 0
$ flux async.js    # await something, then throw
Error: boom
exit: 0
$ flux rej.js      # Promise.reject(...)
Uncaught (in promise) Error: unhandled
exit: 0
```

The error is reported, so nothing is silent, but the exit code says the run
succeeded. Any consumer that composes flux scripts - a Makefile, a build step,
a CI job, a script calling another script - therefore cannot tell a completed
run from a failed one without parsing stdout.

## Where it comes from

`FluxEngine::eval_source` returns `()` (`flux/src/engine.rs`): a module
evaluation error goes to `report_error`, and a rejected top-level promise to
`report_rejection`, both of which log and return. The `flux` binary
(`flux/src/bin/flux.rs`) awaits `eval_source` and falls off the end of `main`,
so there is nothing for it to check even if it wanted to.

## Shape

The engine already knows an uncaught error happened at the moment it reports
one. It needs to remember that and let the embedder ask:

- `report_error`/`report_rejection` set a flag on the engine.
- `eval_source` (and `eval_module`) surface it - a returned `bool`, or an
  `errored()` accessor on the engine, whichever reads better against the
  existing signatures.
- The `flux` binary exits 1 when it is set.

Deliberately an embedder decision, not an engine one: the engine keeps
reporting and returning, and each consumer chooses what an uncaught error
means. `flux` is a CLI, so for it the answer is a non-zero exit; lattice runs
an app event loop and will want to keep running.

Worth deciding at the same time: whether an unhandled rejection with no
top-level throw should count the same way (the third case above), and whether
a rejection reported after the entry module has finished - from a timer or an
event callback, long into the event loop - should affect the exit code of a
run that otherwise completes.

## Why it matters beyond tidiness

The website build (`website/src/build.ts`) is the current example. It
validates that every documentation pull directive resolves and throws when one
does not, which is the guard that keeps a renamed declaration from silently
blanking a reference page. The error prints, `make build` succeeds, and a
broken site ships. Every future flux-scripted build step inherits the same
hole.
