import { fileURLToPath } from "node:url"
import { buildFluxScript } from "./flux-script"

// The dev server (src/server/) is a flux script (lib/flux-script.ts on
// why that means a bundle). A published CLI ships this bundle prebuilt as
// dist/server.js (the Makefile target, run at release time); a checkout
// builds it per launch (main.ts).
export async function buildServerBundle(outfile: string): Promise<void> {
  let entry = fileURLToPath(new URL("../server/main.ts", import.meta.url))
  await buildFluxScript(entry, outfile, "the dev server")
}
