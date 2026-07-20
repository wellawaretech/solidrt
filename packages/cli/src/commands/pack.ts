import { values, source } from "../args"
import { bundleFlux, bundleSolid, compileToBytecode } from "../bundler"
import { loadPackFonts, resolvePackFonts } from "../fonts"
import { loadAppIdentity } from "../project"
import { packRunner } from "../packer"
import { buildPackFolder, writePackFolder } from "../pack-folder"
import { requireBinary } from "../util"
import { resolve } from "node:path"

// Write the packed executable, mark it runnable, and report its size.
async function writeExecutable(packed: Buffer, outfile: string) {
  await Bun.write(outfile, packed)
  if (process.platform !== "win32") {
    Bun.spawnSync(["chmod", "+x", outfile])
  }
  console.log(`>> wrote ${packed.length} bytes to ${outfile}`)
}

function printIdentity() {
  let identity = loadAppIdentity(source!)
  console.log(`>> app: ${identity.appId} (${identity.org} / ${identity.displayName})`)
  if (identity.defaulted) {
    console.warn('>> warning: no "solidrt.appId" in package.json; set a stable reverse-DNS id before distributing')
  }
}

export async function runPackCommand() {
  if (values.folder) {
    // The canonical flat folder (see pack-folder.ts). The single-file exe
    // remains the default output; it becomes a wrapper over this folder later.
    if (values.flux) {
      console.error("--folder is for app packs; flux scripts have no folder output")
      process.exit(1)
    }
    printIdentity()
    let fonts = resolvePackFonts(source!)
    console.log(`>> fonts: ${fonts.length ? fonts.map((f) => f.alias).join(", ") : "none"}`)
    let bytecode = await compileToBytecode(await bundleSolid())
    let folder = buildPackFolder(source!, bytecode)
    let outDir = values.output ?? "dist"
    writePackFolder(outDir, requireBinary("solidrt"), bytecode, folder)
    console.log(`>> wrote pack folder to ${resolve(outDir)}`)
    process.exit()
  }

  let outfile = values.output ?? source!.replace(/\.[jt]sx?$/, "")
  // On Windows the packed image is a PE executable; it needs a .exe name to run.
  if (process.platform === "win32" && !outfile.toLowerCase().endsWith(".exe")) {
    outfile += ".exe"
  }
  let packed: Buffer
  if (values.flux) {
    packed = await packRunner("fluxrt", await bundleFlux(source!))
  } else {
    printIdentity()
    let fonts = loadPackFonts(source!)
    console.log(`>> fonts: ${fonts.length ? fonts.map((f) => f.alias).join(", ") : "none"}`)
    packed = await packRunner("solidrt", await bundleSolid(), fonts, loadAppIdentity(source!))
  }
  await writeExecutable(packed, outfile)
  process.exit()
}