import { values } from "../args"
import { requireBinary, run } from "../util"

// Standalone solidrt-go client (no dev server). The `run` command instead uses
// spawnClient() to launch a client tied to the dev-server lifecycle.
export async function runClientCommand() {
  let runner = requireBinary("solidrt-go")
  let args: string[] = []
  if (values.size) args.push("--size", values.size)
  //TODO add dev server connection
  // if (source) args.push("--dev-server", source)
  let exit = await run(runner, args)
  process.exit(exit)
}