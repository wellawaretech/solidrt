# create-solidrt

Scaffold a new [SolidRT](https://github.com/antoinevanwel/solidrt) project:

```sh
bun create solidrt my-app
cd my-app
bun run dev
```

This forwards to `srt init` from `@solidrt/cli`, which writes a starter project
(package.json, tsconfig.json, AGENTS.md, src/index.tsx) into a new, empty folder
and installs the dependencies.

Bun is only used for development: it runs the scaffolder, the `srt` CLI and the
dev server. A SolidRT app itself runs on the SolidRT runtime, which embeds its
own JavaScript engine, so Bun is not needed to run or ship the app.
