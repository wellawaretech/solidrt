# srt pack

{{ usage pack }}

Bundles, compiles to bytecode and appends the result to the runner as one
standalone executable; `--folder` writes the flat runner + manifest +
bundle + assets folder to `dist/pack/` instead. `--flux` packs a script for
the bare [Flux runtime](/runtime/). Experimental.

`--app` writes the app alone, without a runner: one `<entry>.srtapp` holding
the manifest, the bytecode and the assets, which any `solidrt` runner of the
same version runs as `solidrt <file>.srtapp`. The runner is used in place,
so a signed runner stays signed; the file is platform-independent. This is
how the CLI ships the [console](../console/docs.md).

Set a stable `appId` in the `solidrt` key of package.json before
distributing (it keys the app's storage folder); `pack` warns while it is
defaulted from the package name.
