import { values, port, clientStorageArgs } from "../lib/args"
import { requireBinary, run } from "../lib/util"
import { resolveFromCwd } from "../lib/registry"

// Standalone solidrt-go client (no dev server of its own). Without flags it
// attaches to the dev server of the project (or file) in the current
// directory, resolved from the registry, and starts on its own (into the
// launcher) when there is none; --port picks a local server by port and
// --server names any address, and those must exist. A device is `srt android`.
export async function main() {
  let runner = requireBinary("solidrt-go")
  let args: string[] = [...clientStorageArgs()]
  if (values.size) args.push("--size", values.size)
  let address: string | null
  if (values.server) {
    if (!values.server.includes(":")) {
      console.error(`--server needs host:port (got "${values.server}"); dev servers have no fixed port`)
      process.exit(1)
    }
    address = values.server
  } else if (port !== undefined) {
    address = `127.0.0.1:${port}`
  } else {
    let resolved = await resolveFromCwd(process.cwd())
    address = resolved.ok ? `127.0.0.1:${resolved.record.port}` : null
  }
  if (address) args.push("--dev-server", address)
  let exit = await run(runner, args)
  process.exit(exit)
}
