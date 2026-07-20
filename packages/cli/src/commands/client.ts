import { values, clientStorageArgs } from "../args"
import { requireBinary, run } from "../util"
import { spawnAndroidClient } from "../dev-android"

// Standalone solidrt-go client (no dev server). The `run` command instead uses
// spawnClient() to launch a client tied to the dev-server lifecycle. Either way
// the client discovers a dev server over the LAN; with --android it is installed
// and launched on a connected Android device instead of run locally.
export async function runClientCommand() {
  if (values.android) {
    await spawnAndroidClient()
    return
  }

  let runner = requireBinary("solidrt-go")
  let args: string[] = [...clientStorageArgs()]
  if (values.size) args.push("--size", values.size)
  //TODO add dev server connection
  // if (source) args.push("--dev-server", source)
  let exit = await run(runner, args)
  process.exit(exit)
}