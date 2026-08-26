import { existsSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { appArgs } from "../lib/args"
import { requireBinary, run } from "../lib/util"

// srt console: the dev console (apps/console) as a packed app on the plain
// runner. The console is pre-compiled: dist/console.srtapp (manifest +
// bytecode + assets, `srt pack --app`), built by `make -C packages/cli
// dist/console.srtapp` - the release workflow before publishing, a checkout
// after editing the console. This command builds nothing: it points the
// runner at the file, and the runner is used in place, so a signed runner
// stays signed.
export async function main() {
  let app = fileURLToPath(new URL("../../dist/console.srtapp", import.meta.url))
  if (!existsSync(app)) {
    console.error("Console not built: run make -C packages/cli dist/console.srtapp")
    process.exit(1)
  }
  let exit = await run(requireBinary("solidrt"), [app, ...appArgs])
  process.exit(exit)
}
