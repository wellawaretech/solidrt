// A flux script is one plain-JS file: the flux binary runs no TypeScript
// and loads no module from disk. So a script written as TypeScript modules
// (the dev server, a package's flux tool) is bundled into one file first.
// Bun is already the bundler; the browser target keeps node builtins out,
// and the flux: capability modules stay external (the runtime provides
// them).
export async function buildFluxScript(entry: string, outfile: string, what: string): Promise<void> {
  let result = await Bun.build({
    entrypoints: [entry],
    target: "browser",
    format: "esm",
    external: ["flux:*"],
  })
  if (!result.success) {
    console.error(`[cli] Failed to bundle ${what}:`)
    for (let log of result.logs) console.error(String(log))
    process.exit(1)
  }
  await Bun.write(outfile, result.outputs[0]!)
}
