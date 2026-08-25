// Prebuild the dev server bundle the published CLI ships (dist/server.js,
// listed in package.json "files"; the release workflow runs this before
// publishing). A checkout does not need it: srt bundles the server per
// launch when the file is absent.
import { resolve } from "node:path"
import { buildServerBundle } from "../src/server-bundle"

let outfile = resolve(import.meta.dir, "../dist/server.js")
await buildServerBundle(outfile)
console.log(`>> wrote ${outfile}`)
