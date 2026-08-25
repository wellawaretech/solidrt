// Standalone bundler entry, spawned by the dev server (a flux process) as a
// bun subprocess: flux cannot call Bun.build, so every rebuild shells out to
// this. Params arrive as one JSON argument; one JSON object { code, map,
// manifest, isolates } goes to stdout and diagnostics to stderr. On a build
// failure it exits non-zero with an empty stdout. A prebuilt .srt.js entry
// is read as-is, with its sibling isolate bundles.

import { bundleWith, readPrebuiltIsolates, isolateManifestAssets, type BundleOptions } from "../bundler"
import { buildManifest } from "../project"
import type { BundleOutput } from "../../shared/bundle"

let params = JSON.parse(process.argv[2] ?? "{}") as BundleOptions
let result: BundleOutput | null
if (params.entry.endsWith(".srt.js")) {
  let code = await Bun.file(params.entry).text()
  let isolates = readPrebuiltIsolates(params.entry).map((i) => ({ ...i, map: null }))
  result = { code, map: null, manifest: buildManifest(code, params.entry, isolateManifestAssets(isolates), params.project), isolates }
} else {
  result = await bundleWith(params)
}
if (!result) process.exit(1)
process.stdout.write(JSON.stringify(result))
