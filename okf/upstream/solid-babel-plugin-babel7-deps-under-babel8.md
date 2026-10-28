---
title: Solid babel plugin peers Babel 8 but depends on Babel 7 helpers
description: "@solidjs/babel-plugin rc.19 widened its @babel/core peer to ^7 || ^8 but still depends on @babel/plugin-syntax-jsx ^7, @babel/types ^7 and @babel/helper-module-imports 7.18.6, so a Babel 8 install carries a Babel 7 helper set plus an unused nested @babel/core 7."
project: "@solidjs/babel-plugin" (github.com/solidjs/solid, packages/babel-plugin)
versions: "@solidjs/babel-plugin 2.0.0-rc.19 (next HEAD as of 2026-10-28)"
status: unfiled
link:
created: 2026-10-28
---

# Solid babel plugin peers Babel 8 but depends on Babel 7 helpers

Found 2026-10-28 while moving `@solidrt/cli` to Babel 8, which rc.19 made
possible. The plugin's `package.json` on `next`:

```json
"dependencies": {
  "@babel/helper-module-imports": "7.18.6",
  "@babel/plugin-syntax-jsx": "^7.18.6",
  "@babel/types": "^7.20.7",
  ...
},
"peerDependencies": {
  "@babel/core": "^7.20.12 || ^8.0.0",
  ...
}
```

The peer says Babel 8 is supported. The dependencies still pin the Babel 7
majors, and `index.js` requires all three at run time (`SyntaxJSX` is the
plugin's `inherits`, `@babel/types` builds every node it emits). So under
`@babel/core` 8 the installed tree is:

- `@babel/core` 8.0.9 (ours, the engine that runs);
- `@babel/plugin-syntax-jsx` 7.29.7, `@babel/types` 7.29.8 and
  `@babel/helper-module-imports` 7.18.6, nested under the plugin;
- `@babel/core` 7.29.7 nested under that `plugin-syntax-jsx`, installed only
  to satisfy its `@babel/core ^7.0.0-0` peer. Nothing loads it
  (`plugin-syntax-jsx` imports only `@babel/helper-plugin-utils`), so it is
  dead weight with its whole dependency tree.

It works: `syntax-jsx` 7's `assertVersion` accepts `^8`, and Babel 8's
generator prints the nodes `@babel/types` 7 builds. The rc.19 changelog
("both Babel majors print them as before") shows upstream tests exactly this
mixed setup. Nothing breaks. The cost is a second Babel helper set in every
Babel 8 install, nodes from one `@babel/types` major going through another
major's traversal and generator, and a lockfile that reads as a Babel 7
dependency to anyone auditing it.

## Draft report

**@solidjs/babel-plugin: Babel 8 peer, but Babel 7 helper dependencies pull a
second Babel into Babel 8 installs**

rc.19 widened the `@babel/core` peer to `^7.20.12 || ^8.0.0`. The regular
dependencies were not widened: `@babel/plugin-syntax-jsx ^7.18.6`,
`@babel/types ^7.20.7` and `@babel/helper-module-imports 7.18.6` (exact).
With `@babel/core` 8 installed, the package manager adds the Babel 7
versions of all three under the plugin, plus a nested `@babel/core` 7 to
satisfy `plugin-syntax-jsx` 7's own `@babel/core ^7` peer. That `@babel/core`
7 is never loaded.

Repro: a project with `@babel/core ^8`, `@babel/preset-typescript ^8` and
`@solidjs/babel-plugin 2.0.0-rc.19`. After install, the lockfile holds
`@solidjs/babel-plugin/@babel/plugin-syntax-jsx/@babel/core` at 7.x.

Suggested fix, either of:

1. Widen the three ranges to `^7.x || ^8.0.0` (8.0.0 is out for all three:
   `plugin-syntax-jsx` 8.0.1, `types` 8.x, `helper-module-imports` 8.0.0).
   The package manager then dedupes against the user's Babel.
2. Better for `@babel/types`: take it from the running core instead of a
   dependency (`api.types`, the `types` argument every plugin already
   receives), so the nodes always match the engine's major. That is the
   usual Babel-plugin pattern and removes the dependency.

Either way, the exact pin on `helper-module-imports` 7.18.6 has to go;
an exact pin defeats deduplication even on Babel 7.

## Upstream status

Checked 2026-10-28 before writing: not known. `packages/babel-plugin/package.json`
on `next` (version 2.0.0-rc.19) has the ranges above. No issue on
solidjs/solid mentions Babel 8 dependencies (searched "babel 8",
"plugin-syntax-jsx").

## Our side

`@solidrt/cli` stays on Babel 7 until this is fixed (reverted 2026-10-28), so
only one Babel is installed. The Babel 8 move itself was done and verified
once; two Babel 8 behaviour changes need config in
`packages/cli/src/bundle/bundler.ts` when we take it again:

- `@babel/plugin-syntax-typescript` 8 no longer removes the `jsx` parser
  plugin for `.ts` files, and the Solid plugin inherits `syntax-jsx`, so a
  `.ts` file parses JSX and `<T>(x) => x` fails. Run only the
  import-attribute plugin on `.ts` files (no JSX there to compile).
- `@babel/preset-typescript` 8 defaults `onlyRemoveTypeImports: true`, which
  keeps unused and type-only plain imports. The scaffold tsconfig has no
  `verbatimModuleSyntax`, so pass `onlyRemoveTypeImports: false`.
- Babel 8 ships its own types: drop `@types/babel__core`, and declare the
  Solid plugin as `PluginTarget` (not `PluginItem`) in
  `untyped-deps.d.ts`.

With those, the player and BSOD bundles were byte-identical to the Babel 7
output. On `resolved`, bump the three `@babel/*` deps in
`packages/cli/package.json` to `^8` and apply the three items above.
