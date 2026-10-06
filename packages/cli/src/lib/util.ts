import { resolveBinary } from "./artifacts"

// Run `cmd` in `cwd` with its output held back: the exit code and the
// combined output, for a caller that shows the output only on failure.
export async function runQuiet(cmd: string[], cwd: string): Promise<{ code: number; output: string }> {
  let proc = Bun.spawn(cmd, { cwd, stdout: "pipe", stderr: "pipe" })
  let [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited])
  return { code, output: out + err }
}

// Build target per binary, for the "not found" hint. Run from the repo root.
let BUILD_HINTS: Record<string, string> = {
  "solidrt-go": "make client",
  solidrt: "make runtime",
  flux: "make -C flux flux",
  fluxc: "make -C flux fluxc",
  fluxrt: "make -C flux fluxrt",
}

export function requireBinary(name: string) {
  let path = resolveBinary(name)
  if (path) return path
  let hint = BUILD_HINTS[name]
  console.error(`Could not find ${name} binary.`)
  if (hint) {
    console.error(`Build it from source: run ${hint}, with SOLIDRT_HOME pointing at your SolidRT checkout.`)
  } else {
    console.error("Build it from source, with SOLIDRT_HOME pointing at your SolidRT checkout.")
  }
  process.exit(1)
}

export async function run(binary: string, args: string[]) {
  let proc = Bun.spawn([binary, ...args], { stdio: ["inherit", "inherit", "inherit"] })
  return proc.exited
}
