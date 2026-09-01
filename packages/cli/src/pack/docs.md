# srt pack

{{ usage pack }}

Bundles, compiles to bytecode and appends the result to the runner as one
standalone executable; `--folder` writes the flat runner + manifest +
bundle + assets folder to `dist/pack/` instead. `--flux` packs a script for
the bare [Flux runtime](/runtime/). Experimental.

One output rule: every deliverable defaults into the gitignored `dist/`
build root - the executable, `.srtapp` and `.apk` as `dist/<name>` files
named by the appId's last segment, flow folders (`pack/`, `render/`,
`bundle/`) below it - never next to the sources. `--output` overrides.

`--app` writes the app alone, without a runner: one `<name>.srtapp` holding
the manifest, the bytecode and the assets, which any `solidrt` runner of the
same version runs as `solidrt <file>.srtapp`. The runner is used in place,
so a signed runner stays signed; the file is platform-independent. This is
how the CLI ships the [console](../console/docs.md).

`--apk` writes an installable Android APK, with no Android SDK on the
machine: a copy of the runner APK is patched in place - application id
(from `appId`) and label rewritten, the `.srtapp` payload added - then
re-aligned and re-signed with a fixed development key, so the result
sideloads out of the box (publishing will need a real key). The base is
the production runner APK (in a checkout, `make android-runtime` stages
it), which boots the payload directly - no launcher, no dev server. With
no runner staged, the solidrt-go dev client stands in: that APK installs
and launches, but boots the launcher instead of the payload.

Set a stable `appId` in the `solidrt` key of package.json before
distributing (it keys the app's storage folder); `pack` warns while it is
defaulted from the package name.
