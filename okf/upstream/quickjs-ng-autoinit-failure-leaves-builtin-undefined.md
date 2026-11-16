---
title: quickjs-ng leaves a built-in method undefined when its first use fails
description: QuickJS builds a built-in's methods lazily at their first read, and when that build fails at the memory limit it has already cleared the lazy entry, so the method reads as undefined for the rest of the runtime's life.
project: quickjs-ng (github.com/quickjs-ng/quickjs)
versions: quickjs-ng 0.16.2 plus 26 commits (commit 0fdea21f, as vendored by rquickjs-sys 0.14.0); Bellard's QuickJS has the same code (read 2026-11-16)
status: unfiled
link:
created: 2026-11-16
---

# quickjs-ng: a failed lazy init leaves a built-in method undefined

Not filed, and not searched for upstream yet: check quickjs-ng for an
existing report and the code on its master before writing one.

## The bug

Built-in methods declared with `JS_DEF_CFUNC` are lazy ("autoinit")
properties. The first read of the property builds the function object.
`JS_AutoInitProperty` (`quickjs.c:9078` in the vendored copy) calls the
build function, then frees the lazy entry, clears the property's type flags
and stores `undefined`, and only after that checks whether the build
failed:

```c
val = func(realm, p, prop, pr->u.init.opaque);
js_autoinit_free(ctx->rt, pr);
prs->flags &= ~JS_PROP_TMASK;
pr->u.value = JS_UNDEFINED;
if (JS_IsException(val))
    return -1;
pr->u.value = val;
```

A build that fails (out of memory at `JS_SetMemoryLimit`) leaves a plain
data property holding `undefined`. The method is gone for the rest of the
runtime's life, also after the whole heap is freed.

The other users of the lazy entry carry the same risk: a function's
`prototype` object and a module namespace's exports.

## Reproduction

An engine with a memory limit. The script fills the heap until an
allocation throws, then frees a few bytes per step and runs
`let g = function () { return 1 }; g.call(null);` in a try/catch. After the
first `TypeError: not a function`, free everything:
`typeof Function.prototype.call` is `"undefined"`. Direct calls, arrows,
`apply` and `Reflect.apply` still work, because they were used before the
fill or need no lazy method.

## What it does to us

Only an engine with a memory limit can meet it, which is a `flux:isolate`
child with `memoryLimit`. A child that touches a built-in method for the
first time at its limit loses that method. The embedded `streams.js` then
fails wherever it uses the method (seen at `WeakMap.prototype.set` and at
`Function.prototype.call`), so a stream cannot be built in that child any
more.

`flux/tests/memory_limit.rs` works around it: a walk step makes no closure
and calls no method on a built-in.

Our answer is the same as for the dropped promise reactions
([quickjs-ng-promise-reactions-dropped-at-memory-limit](quickjs-ng-promise-reactions-dropped-at-memory-limit.md)):
a child at its limit is documented as not reliable, and
[isolate-memory-limit-ends-child](../backlog/isolate-memory-limit-ends-child.md)
ends the child at the limit, which makes this harmless. Building every
lazy property during startup, before the limit applies, would avoid it too,
at the cost of startup memory in every engine.

## Proposal for upstream

Keep the lazy entry when the build fails: check `JS_IsException(val)`
first and return -1 with the property untouched, so that a later read
builds the method again.
