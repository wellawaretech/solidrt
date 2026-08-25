import { resolveBinary } from "./artifacts"

// A fatal usage or configuration error: the message, then exit. The
// throw-in-dev policy (CLAUDE.md, "API design"): a bad value fails the
// command instead of being papered over.
export function fail(message: string): never {
  console.error(message)
  process.exit(1)
}

// Build target per binary, for the "not found" hint. Run from the repo root.
let BUILD_HINTS: Record<string, string> = {
  "solidrt-go": "make client",
  solidrt: "make runtime",
  flux: "make -C flux flux",
  fluxc: "make -C flux fluxc",
  fluxrt: "make -C flux fluxrt PROFILE=release-opt",
}

export function requireBinary(name: string) {
  let path = resolveBinary(name)
  if (path) return path
  let hint = BUILD_HINTS[name]
  console.error(`Could not find ${name} binary.`)
  if (hint) {
    console.error(`Build it from source: run ${hint}, with SRT_HOME pointing at your SolidRT checkout.`)
  } else {
    console.error("Build it from source, with SRT_HOME pointing at your SolidRT checkout.")
  }
  process.exit(1)
}

export async function run(binary: string, args: string[]) {
  let proc = Bun.spawn([binary, ...args], { stdio: ["inherit", "inherit", "inherit"] })
  return proc.exited
}
