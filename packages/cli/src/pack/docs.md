# srt pack

{{ usage pack }}

Bundles, compiles to bytecode and appends the result to the runner as one
standalone executable; `--folder` writes the flat runner + manifest +
bundle + assets folder to `dist/pack/` instead. `--flux` packs a script for
the bare [Flux runtime](/runtime/). Experimental.

Set a stable `appId` in the `solidrt` key of package.json before
distributing (it keys the app's storage folder); `pack` warns while it is
defaulted from the package name.
