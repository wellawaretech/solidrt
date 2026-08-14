---
title: Bindings reject an explicitly passed undefined for an optional argument
description: An omitted option object arrives at the binding as an explicit undefined, which Opt<Object> refuses - so createTexture without opts throws and every <Image> load fails.
created: 2026-08-13
completed: 2026-08-13
---

# Bindings reject an explicitly passed undefined for an optional argument

Symptom (apps/inspector, 2026-08-13): an `<Image>` renders as an empty box of
its layout size, with nothing in the log. `Image` contains load failures
unless the caller passes `onError`, so the app looks like a layout bug. With
`onError` wired up:

```
TypeError: Error converting from js 'undefined' into type 'object'
    at createTexture2 (packages/core/src/gpu.ts:218)
    at <anonymous>   (packages/core/src/image.ts:140)
```

Reduced, in app code, away from any component:

```js
createTexture(data, w, h)      // throws: converting from js 'undefined' into type 'object'
createTexture(data, w, h, {})  // ok, returns a texture id
```

## Mechanism

`packages/core/src/gpu.ts:218` always forwards a fourth argument:

```ts
let id = gpu.createTexture(data, width, height, opts)   // opts may be undefined
```

The binding declares `opts: Opt<Object<'_>>`
(`flux/src/plugins/gui/gpu.rs:560`). rquickjs `Opt` means "the argument was
ABSENT", not "the argument was undefined": an explicit `undefined` is still
converted, `Object::from_js(undefined)` fails, and the call throws. Calling the
same binding with three arguments would be fine - but no JS caller does,
because the wrapper always passes four.

## Reach

- `createImage` / `<Image>` fail for EVERY source: the URL path
  (`packages/core/src/image.ts:41`) and the byte path (`:140`) both call the
  three-argument `createTexture`. Image loading is broken wholesale in this
  build.
- Six more core wrappers forward a possibly-undefined trailing `opts` the same
  way: `createMutableTexture`, `createShaderTexture`, `createShaderTarget`,
  `createDrawTarget`, `createPipelineTexture`, `createBuffer`
  (`packages/core/src/gpu.ts`).
- ~44 `Opt<Object` argument sites across `flux/src/plugins/` (gpu, camera,
  audio, microphone, tree, svg, image, net, p2p, serve, mdns, subprocess,
  fetch, request, response, text) share the shape, so any JS caller that
  spells out `undefined` for an optional trailing object hits it. Only the
  seven wrappers above are known to do so today.

Not local dirt: the `Opt<Object<'_>>` signature is committed, from 3c1190a
(2026-08-06, "GPU draw list"), and the files are clean.

## Fix candidates, in preference order

1. **Binding side.** Make an optional trailing object tolerate an explicit
   `undefined` - `opts: Opt<Option<Object<'js>>>` and `opts.0.flatten()` at the
   use sites, or a small newtype that does it once. Correct layer: passing
   `undefined` for "no options" is ordinary JS, so the binding should accept
   it, and this closes the whole class rather than the one caller that tripped
   over it. Costs a client rebuild.
2. **Wrapper side.** Have the core wrappers omit the argument when `opts` is
   undefined. Pure JS, so it takes effect on the next reload with no rebuild,
   but it patches callers rather than the contract, and every future wrapper
   has to remember.

Done looks like: `createTexture(data, w, h)` returns an id, `<Image>` shows a
picture from both a URL and a Uint8Array, and a test in `flux/src/tests/`
covers an optional-object binding called with an explicit `undefined`.

## Resolution

Fixed via candidate 1, generalized past objects: `marshal::OptArg<T>` is a
`FromParam` newtype that treats absent, `undefined`, and `null` alike as
`None`, and every optional binding argument in `flux/src/plugins/` now uses
it - rquickjs `Opt` no longer appears in the plugin layer at all. That also
closed the same flaw at scalar sites the original report only counted in
passing (`WebSocket.close(code?, reason?)`, `TextEncoder.encode(input?)`,
`flux:subprocess` `spawn(cmd, args?, opts?)`'s middle argument, upload
offsets, sqlite params, multicast iface), and folded in the hand-rolled
`Opt<Value>` undefined checks (`modules/image.rs`, `modules/svg.rs`,
`net_icmp_echo`). The rule for new bindings lives on `OptArg`'s doc comment;
tests in `flux/src/tests/marshal.rs`. Verified on a rebuilt client via
apps/inspector's `<Image src={Uint8Array}>` path. Upstream angle tracked in
okf/upstream/rquickjs-opt-explicit-undefined.md.
