# srt bundle

{{ usage bundle }}

Writes `dist/bundle/` (or `--output <dir>`): `<name>.srt.js` plus the app's
isolate modules as `isolates/<id>.js`, or bytecode with `--compile`. Move
the dir, not the bare file: a bundle loaded without its isolates/ dir loses
them.

`--flux` targets the bare [Flux runtime](/runtime/) instead of a SolidRT
app, for scripts and servers with no UI. `--json` is the dev server's
rebuild contract, one JSON object on stdout.
