import { values, source } from "../args"
import { bundleFlux, bundleSolid, compileToBytecode, findFluxIsolates } from "../bundler"
import { resolvePackFonts } from "../fonts"
import { loadAppIdentity } from "../project"
import { resolveMode } from "../mode"
import { packFlux, packSolid } from "../packer"
import { buildPackFolder, writePackFolder } from "../pack-folder"
import { requireBinary } from "../util"
import { dirname, join, resolve } from "node:path"

// Windows executables need the suffix; a user-given --output may already
// carry it.
function exeName(outfile: string): string {
  return process.platform === "win32" && !outfile.toLowerCase().endsWith(".exe") ? outfile + ".exe" : outfile
}

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
    let outfile = exeName(values.output ?? source!.replace(/\.[jt]s$/, ""))
    // The entry's isolate modules ride along as isolates/<id>.bin sections
    // (module name = id, for stack attribution).
    let isolates = []
    for (let module of findFluxIsolates(dirname(resolve(source!)))) {
      isolates.push({ id: module.id, bytecode: await compileToBytecode(await bundleFlux(module.path), module.id) })
    }
    if (isolates.length) console.log(`>> isolates: ${isolates.map((i) => i.id).join(", ")}`)
    await writeExecutable(packFlux(await compileToBytecode(await bundleFlux(source!)), isolates), outfile)
    process.exit()
  }

  // Both solidrt outputs are the same canonical pack: manifest + bundle.bin +
  // assets (fonts included). --folder writes it as a flat folder next to a
  // bare runner; the default single-file exe carries it as trailer sections.
  let mode = resolveMode()
  let identity = loadAppIdentity(mode.entry, mode.projectDir)
  console.log(`>> app: ${identity.appId} (${identity.org} / ${identity.displayName})`)
  if (identity.defaulted) {
    console.warn('>> warning: no "solidrt.appId" in package.json; set a stable reverse-DNS id before distributing')
  }
  let fonts = resolvePackFonts(mode.projectDir)
  console.log(`>> fonts: ${fonts.length ? fonts.map((f) => f.alias).join(", ") : "none"}`)

  let bundled = await bundleSolid(mode)
  let bytecode = await compileToBytecode(bundled.code)
  let isolates = []
  for (let i of bundled.isolates) isolates.push({ id: i.id, bytecode: await compileToBytecode(i.code, i.id) })
  if (isolates.length) console.log(`>> isolates: ${isolates.map((i) => i.id).join(", ")}`)
  let folder = buildPackFolder(mode, bytecode, isolates)

  if (values.folder) {
    let outDir = values.output ?? join("dist", "pack")
    writePackFolder(outDir, requireBinary("solidrt"), bytecode, folder)
    console.log(`>> wrote pack folder to ${resolve(outDir)}`)
    process.exit()
  }

  let outfile = values.output ?? mode.entry.replace(/\.[jt]sx?$/, "")
  // On Windows the packed image is a PE executable; it needs a .exe name to run.
  if (process.platform === "win32" && !outfile.toLowerCase().endsWith(".exe")) {
    outfile += ".exe"
  }
  await writeExecutable(packSolid(folder, bytecode), outfile)
  process.exit()
}
