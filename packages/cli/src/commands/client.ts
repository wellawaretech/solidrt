import { values, port, clientStorageArgs } from "../args"
import { requireBinary, run } from "../util"
import { spawnAndroidClient } from "../android"
import { resolveFromCwd } from "../registry"

// Standalone solidrt-go client (no dev server of its own). Without flags it
// attaches to the dev server of the project (or file) in the current
// directory, resolved from the registry; --port picks a local server by port
// and --server names any address. With --android it is installed and
// launched on a connected Android device instead of run locally.
export async function runClientCommand() {
  if (values.android) {
    await spawnAndroidClient()
    return
  }

  let runner = requireBinary("solidrt-go")
  let args: string[] = [...clientStorageArgs()]
  if (values.size) args.push("--size", values.size)
  let address: string
  if (values.server) {
    if (!values.server.includes(":")) {
      console.error(`--server needs host:port (got "${values.server}"); dev servers have no fixed port`)
      process.exit(1)
    }
    address = values.server
  } else if (port !== undefined) {
    address = `127.0.0.1:${port}`
  } else {
    let resolved = resolveFromCwd(process.cwd())
    if (!resolved.ok) {
      console.error(resolved.message)
      process.exit(1)
    }
    address = `127.0.0.1:${resolved.record.port}`
  }
  args.push("--dev-server", address)
  let exit = await run(runner, args)
  process.exit(exit)
}
