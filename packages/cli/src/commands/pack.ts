import { values, source } from "../args"
import { bundleFlux, bundleSolid, compileToBytecode } from "../bundler"
import { resolvePackFonts } from "../fonts"
import { loadAppIdentity } from "../project"
import { packFlux, packSolid } from "../packer"
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

export async function runPackCommand() {
  if (values.flux) {
    if (values.folder) {
      console.error("--folder is for app packs; flux scripts have no folder output")
      process.exit(1)
    }
    let outfile = values.output ?? source!.replace(/\.[jt]sx?$/, "")
    if (process.platform === "win32" && !outfile.toLowerCase().endsWith(".exe")) {
      outfile += ".exe"
    }
    await writeExecutable(await packFlux(await bundleFlux(source!)), outfile)
    process.exit()
  }

  // Both solidrt outputs are the same canonical pack: manifest + bundle.bin +
  // assets (fonts included). --folder writes it as a flat folder next to a
  // bare runner; the default single-file exe carries it as trailer sections.
  let identity = loadAppIdentity(source!)
  console.log(`>> app: ${identity.appId} (${identity.org} / ${identity.displayName})`)
  if (identity.defaulted) {
    console.warn('>> warning: no "solidrt.appId" in package.json; set a stable reverse-DNS id before distributing')
  }
  let fonts = resolvePackFonts(source!)
  console.log(`>> fonts: ${fonts.length ? fonts.map((f) => f.alias).join(", ") : "none"}`)

  let bundled = await bundleSolid()
  let bytecode = await compileToBytecode(bundled.code)
  let isolates = []
  for (let i of bundled.isolates) isolates.push({ id: i.id, bytecode: await compileToBytecode(i.code, i.id) })
  if (isolates.length) console.log(`>> isolates: ${isolates.map((i) => i.id).join(", ")}`)
  let folder = buildPackFolder(source!, bytecode, isolates)

  if (values.folder) {
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
  await writeExecutable(packSolid(folder, bytecode), outfile)
  process.exit()
}
