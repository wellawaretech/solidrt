import { fileURLToPath } from "node:url"

// The dev server (packages/cli/server/) is a flux script; the flux binary
// runs one plain-JS file, so the server's TypeScript modules are bundled
// into one. Bun is already the bundler; the browser target keeps node
// builtins out, and the flux: capability modules stay external (the runtime
// provides them). A published CLI ships this bundle prebuilt as
// dist/server.js (scripts/build-server.ts, run at release time); a checkout
// builds it per launch (commands/server.ts).
export async function buildServerBundle(outfile: string): Promise<void> {
  let entry = fileURLToPath(new URL("../server/main.ts", import.meta.url))
  let result = await Bun.build({
    entrypoints: [entry],
    target: "browser",
    format: "esm",
    external: ["flux:*"],
  })
  if (!result.success) {
    console.error("[cli] Failed to bundle the dev server:")
    for (let log of result.logs) console.error(String(log))
    process.exit(1)
  }
  await Bun.write(outfile, result.outputs[0]!)
}
