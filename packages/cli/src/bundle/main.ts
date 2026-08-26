import { values, source, isPrebuilt } from "../lib/args"
import {
  bundleFlux,
  bundleSolid,
  bundleWith,
  compileToBytecode,
  findFluxIsolates,
  isolateManifestAssets,
  readPrebuiltIsolates,
  walkFiles,
  writeIsolates,
} from "./bundler"
import { resolveMode } from "../lib/mode"
import { buildManifest } from "../lib/project"
import type { BundleOutput } from "../types/bundle"
import { existsSync, mkdirSync, readdirSync, rmSync } from "node:fs"
import { basename, dirname, join, resolve } from "node:path"

// Write to stdout and resolve only once the whole payload is flushed.
// process.stdout.write to a pipe is async and applies backpressure; the
// callback fires after every byte is drained, so it is safe to exit after.
function writeStdout(data: string): Promise<void> {
  return new Promise((resolve, reject) => {
    process.stdout.write(data, (err) => (err ? reject(err) : resolve()))
  })
}

// The bundle output dir (okf/backlog/build-output-dirs.md): the bundle flow's
// subdir of the build root, or an explicit --output dir. The build root is
// the cwd: a project command runs in its root (mode.ts), and a file on its
// own builds where it is run from. Only reused when it is empty or already a
// bundle output (a *.srt.* or *.flux.* bundle at top level) - the
// writePackFolder rule - so it never writes into an unrelated directory.
function ensureOutDir(defaultDir = join("dist", "bundle")): string {
  let outDir = values.output ?? defaultDir
  let existing = existsSync(outDir) ? readdirSync(outDir) : null
  if (existing && existing.length > 0 && !existing.some((name) => /\.(srt|flux)\.(js|bin)$/.test(name))) {
    console.error(`${resolve(outDir)} exists and is not a bundle output; choose another --output`)
    process.exit(1)
  }
  mkdirSync(outDir, { recursive: true })
  return outDir
}

// Clear one form's files from the output's isolates/ dir before rewriting it,
// so removed modules cannot go stale. The dir is shared by a bundle's .js and
// .bin forms, so only the form being rewritten is cleared - a --compile must
// not delete the .js set the .js bundle pairs with, nor the reverse.
function clearIsolates(dir: string, ext: ".js" | ".bin") {
  walkFiles(dir, (abs) => {
    if (abs.endsWith(ext)) rmSync(abs)
  })
}

// Compile one isolate bundle to `<dir>/<id>.bin` (module name = its id, for
// stack attribution).
async function writeIsolateBytecode(dir: string, isolate: { id: string; code: string }) {
  let outfile = join(dir, isolate.id + ".bin")
  mkdirSync(dirname(outfile), { recursive: true })
  await Bun.write(outfile, await compileToBytecode(isolate.code, isolate.id))
}

// Compile JS to a bytecode file and report its size.
async function writeBytecode(jsCode: string, outfile: string) {
  let bytecode = await compileToBytecode(jsCode)
  await Bun.write(outfile, bytecode)
  let binSize = (await Bun.file(outfile).stat()).size
  console.log(`>> wrote ${binSize} bytes to ${outfile}`)
}

export async function main() {
  if (values.flux) {
    let entry = resolve(source!)
    let name = basename(entry).replace(/\.[jt]s$/, "")
    let jsCode = await bundleFlux(entry)
    // Standalone flux resolves isolates by location, not directive: module
    // <id> is <entry dir>/isolates/<id>.js. Bundling keeps that shape - every
    // module under the entry's isolates/ dir is built bare like the entry
    // (which also lets a worker be .ts, unlike running from source) and lands
    // as isolates/<id>.js next to the bundle.
    let isolateModules = findFluxIsolates(dirname(entry))

    if (values.stdout) {
      if (isolateModules.length) {
        console.error("[cli] Warning: this script has isolate modules; --stdout carries only the main bundle")
      }
      await writeStdout(jsCode)
      process.exit()
    }
    let outDir = ensureOutDir()
    if (values.compile) {
      await writeBytecode(jsCode, join(outDir, name + ".flux.bin"))
    } else {
      let outfile = join(outDir, name + ".flux.js")
      await Bun.write(outfile, jsCode)
      console.log(`>> wrote ${jsCode.length} bytes to ${outfile}`)
    }
    // Isolates follow the main bundle's form: source beside a .flux.js,
    // bytecode beside a .flux.bin (the flux resolver reads .bin first).
    let isolatesDir = join(outDir, "isolates")
    if (values.compile) {
      clearIsolates(isolatesDir, ".bin")
      for (let module of isolateModules) {
        await writeIsolateBytecode(isolatesDir, { id: module.id, code: await bundleFlux(module.path) })
      }
    } else {
      clearIsolates(isolatesDir, ".js")
      for (let module of isolateModules) {
        let file = join(isolatesDir, module.id + ".js")
        mkdirSync(dirname(file), { recursive: true })
        await Bun.write(file, await bundleFlux(module.path))
      }
    }
    process.exit()
  }

  // --json: the dev server's rebuild (server/rebuild.ts spawns it in the
  // project root or the entry's directory with the entry and --project or
  // --file, so the mode resolves the same). One BundleOutput object on
  // stdout (types/bundle.d.ts), diagnostics on stderr, exit 1 with an empty
  // stdout on a build failure. A prebuilt .srt.js is read as-is with its
  // sibling isolate bundles.
  if (values.json) {
    let mode = resolveMode()
    let result: BundleOutput | null
    if (mode.entry.endsWith(".srt.js")) {
      let code = await Bun.file(mode.entry).text()
      let isolates = readPrebuiltIsolates(mode.entry).map((i) => ({ ...i, map: null }))
      result = {
        code,
        map: null,
        manifest: buildManifest(code, mode.entry, isolateManifestAssets(isolates), mode.projectDir),
        isolates,
        inputs: [mode.entry],
      }
    } else {
      result = await bundleWith({
        entry: mode.entry,
        project: mode.projectDir,
        dev: values.dev,
        minify: values.minify,
      })
    }
    if (!result) process.exit(1)
    await writeStdout(JSON.stringify(result))
    process.exit()
  }

  // A prebuilt .srt.js (validateArgs admits no other prebuilt form) is
  // compiled to bytecode: the only step left, so --compile is implied. The
  // output lands in the bundle's own dir unless --output says otherwise; its
  // isolate bundles compile along into the output's isolates/ (the ids match,
  // the extension picks the form).
  if (isPrebuilt) {
    let jsFile = resolve(source!)
    if (!existsSync(jsFile)) {
      console.error(`Entry not found: ${source}`)
      process.exit(1)
    }
    let outDir = ensureOutDir(dirname(jsFile))
    await writeBytecode(await Bun.file(jsFile).text(), join(outDir, basename(jsFile).replace(/\.js$/, ".bin")))
    let isolatesDir = join(outDir, "isolates")
    clearIsolates(isolatesDir, ".bin")
    for (let isolate of readPrebuiltIsolates(jsFile)) {
      await writeIsolateBytecode(isolatesDir, isolate)
    }
    process.exit()
  }

  let mode = resolveMode()
  let entry = mode.entry
  let name = basename(entry).replace(/\.[jt]sx?$/, "")

  if (values.stdout) {
    let result = await bundleSolid(mode)
    if (result.isolates.length) {
      console.error("[cli] Warning: this app has isolate modules; --stdout carries only the main bundle")
    }
    await writeStdout(result.code)
    process.exit()
  }

  let outDir = ensureOutDir()
  let isolatesDir = join(outDir, "isolates")

  if (values.compile) {
    let result = await bundleSolid(mode)
    await writeBytecode(result.code, join(outDir, name + ".srt.bin"))
    clearIsolates(isolatesDir, ".bin")
    for (let isolate of result.isolates) {
      await writeIsolateBytecode(isolatesDir, isolate)
    }
    process.exit()
  }

  let result = await bundleSolid(mode)
  let jsOutfile = join(outDir, name + ".srt.js")
  await Bun.write(jsOutfile, result.code)
  clearIsolates(isolatesDir, ".js")
  writeIsolates(isolatesDir, result.isolates)
  console.log(`>> wrote ${result.code.length} bytes to ${jsOutfile}`)
  process.exit()
}
