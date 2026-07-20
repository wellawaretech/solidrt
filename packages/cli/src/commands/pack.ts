import { values, source } from "../args"
import { bundleFlux, bundleSolid } from "../bundler"
import { loadPackFonts } from "../fonts"
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
  // On Windows the packed image is a PE executable; it needs a .exe name to run.
  if (process.platform === "win32" && !outfile.toLowerCase().endsWith(".exe")) {
    outfile += ".exe"
  }
  let packed: Buffer
  if (values.flux) {
    packed = await packRunner("fluxrt", await bundleFlux(source!))
  } else {
    let fonts = loadPackFonts(source!)
    console.log(`>> fonts: ${fonts.length ? fonts.map((f) => f.alias).join(", ") : "none"}`)
    packed = await packRunner("solidrt", await bundleSolid(), fonts)
  }
  await writeExecutable(packed, outfile)
  process.exit()
}