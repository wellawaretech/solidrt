import { values, clientStorageArgs } from "../args"
import { requireBinary, run } from "../util"
import { spawnAndroidClient } from "../dev-android"
import { DEV_PORT } from "../dev-server"

// Standalone solidrt-go client (no dev server). The `run` command instead uses
// spawnClient() to launch a client tied to the dev-server lifecycle. --server
// auto-connects to a dev server at the given address (otherwise the client
// starts on the connect screen); with --android it is installed and launched
// on a connected Android device instead of run locally.
export async function runClientCommand() {
  if (values.android) {
    await spawnAndroidClient()
    return
  }

  let runner = requireBinary("solidrt-go")
  let args: string[] = [...clientStorageArgs()]
  if (values.size) args.push("--size", values.size)
  if (values.server) {
    let address = values.server.includes(":") ? values.server : `${values.server}:${DEV_PORT}`
    args.push("--dev-server", address)
  }
  let exit = await run(runner, args)
  process.exit(exit)
}