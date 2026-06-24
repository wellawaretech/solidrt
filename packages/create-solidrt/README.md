# create-solidrt

Scaffold a new [SolidRT](https://github.com/antoinevanwel/solidrt) project:

```sh
bun create solidrt my-app
cd my-app
bunx srt run src/index.tsx
```

This forwards to `srt init` from `@solidrt/cli`, which writes a starter project
(package.json, tsconfig.json, AGENTS.md, src/index.tsx) into a new, empty folder
and installs the dependencies.