# solidrt

[SolidRT](https://github.com/wellawaretech/solidrt) runs SolidJS apps on a
native runtime. This package holds the name; the code lives under the
`@solidrt` scope.

Start a project:

```sh
bun create solidrt my-app
cd my-app
bun run dev
```

The packages:

- `@solidrt/cli` - the `srt` command-line tool: scaffold, develop, test, ship
- `@solidrt/core` - the runtime surface an app is written against
- `@solidrt/components`, `@solidrt/2d`, `@solidrt/3d`, `@solidrt/router` -
  extensions on top of core
- `@solidrt/test` - tests for an app, run with `bun run srt test`
