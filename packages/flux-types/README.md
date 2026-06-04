# @solidrt/flux-types

TypeScript type definitions for the `Flux` runtime global in [SolidRT](https://github.com/wellawaretech/solidrt) apps.

## Installation

```sh
bun add -d @solidrt/flux-types
```

Then **you must add it to the `types` array** in your `tsconfig.json`. This is
required: the `flux:*` modules are ambient declarations, so they are only
visible to TypeScript when the package is listed in `types`. Without this you
will get `Cannot find module 'flux:fs'` (and the same for `flux:http`,
`flux:process`, etc.).

```json
{
  "compilerOptions": {
    "types": ["@solidrt/flux-types"]
  }
}
```

If you already have a `types` array (for example with `@types/bun`), **add to
it** rather than replacing it — listing `types` disables TypeScript's automatic
type inclusion, so every package you rely on must be named:

```json
{
  "compilerOptions": {
    "types": ["bun", "@solidrt/flux-types"]
  }
}
```

## License

MIT. Copyright (c) 2026 Antoine van Wel.