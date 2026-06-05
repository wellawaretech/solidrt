import { values, source } from "../args"
import { bundleFlux, bundleSolid } from "../bundler"
import { packRunner } from "../packer"

// Write the packed executable, mark it runnable, and report its size.
async function writeExecutable(packed: Buffer, outfile: string) {
  await Bun.write(outfile, packed)
  if (process.platform !== "win32") {
    Bun.spawnSync(["chmod", "+x", outfile])
  }
  console.log(`>> wrote ${packed.length} bytes to ${outfile}`)
}

export async function runPackCommand() {
  let outfile = values.output ?? source!.replace(/\.[jt]sx?$/, "")
  let packed = values.flux
    ? await packRunner("fluxrt", await bundleFlux(source!))
    : await packRunner("solidrt-runner", await bundleSolid())
  await writeExecutable(packed, outfile)
  process.exit()
}