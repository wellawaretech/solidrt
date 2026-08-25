// Standalone typecheck entry, spawned by the dev server (a flux process) as
// a bun subprocess for its startup typecheck: tsc lives in the project's
// node_modules and the check code is bun's, like bundle-cli. Takes the entry
// path as its one argument, prints the report (diagnostics, then the
// verdict) and exits 1 on type errors in app code. No project or no tsc
// means nothing to check, silently: the server never gates on this, srt
// check is the hard gate.

import { findProjectRoot, typecheck, reportTypes } from "./commands/check"

let entry = process.argv[2]
if (!entry) {
  console.error("Usage: typecheck-cli <entry>")
  process.exit(2)
}
let root = findProjectRoot(entry)
if (!root) process.exit(0)
let types = await typecheck(root, entry)
process.exit(types && reportTypes(types) ? 1 : 0)
