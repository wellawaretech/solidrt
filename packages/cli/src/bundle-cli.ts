// Standalone bundler entry, spawned by the dev server (a flux process) as a
// Bun subprocess to rebuild the app on an MCP-triggered reload. flux cannot call
// Bun.build, so the server shells out to this. Params arrive as one JSON
// argument; the bundled code goes to stdout and diagnostics to stderr, so the
// caller reads stdout verbatim. On a build failure it exits non-zero with an
// empty stdout.

import { bundleWith, codeFromOutputs, type BundleOptions } from "./bundler"

let params = JSON.parse(process.argv[2] ?? "{}") as BundleOptions
let result = await bundleWith(params)
if (!result) process.exit(1)
process.stdout.write(await codeFromOutputs(result.outputs))
