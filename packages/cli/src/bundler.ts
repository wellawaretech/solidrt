import { transformAsync } from "@babel/core"
import jsx from "@babel/plugin-syntax-jsx"
import ts from "@babel/preset-typescript"
import solid from "babel-preset-solid"
import { type BunPlugin } from "bun"
import { values, source } from "./args"
import { state, print, requireBinary } from "./util"

// Bun build plugin that runs JSX/TSX through babel-preset-solid (universal
// generate, targeting @solidrt/core) plus the TS preset.
function solidPlugin(): BunPlugin {
  return {
    name: "bun-plugin-solid",
    setup: (build) => {
      build.onLoad({ filter: /\.(js|ts)x$/ }, async (args) => {
        let file = Bun.file(args.path)
        let code = await file.text()
        let transforms = await transformAsync(code, {
          filename: args.path,
          presets: [[solid, { moduleName: "@solidrt/core", generate: "universal" }], [ts]],
          plugins: [jsx],
        })
        return { contents: transforms?.code ?? "", loader: "js" }
      })
    },
  }
}

export async function bundle(entry = source) {
  let result = null

  let devBase = state.serverUrl ?? undefined
  let dev = !!devBase || values.dev
  print(`[cli] Bundling (${dev ? "development" : "production"})`)
  let define: Record<string, string> = {
    "process.env.NODE_ENV": dev ? "development" : "production",
  }
  if (devBase) define.__SRT_DEV_BASE__ = devBase

  try {
    result = await Bun.build({
      entrypoints: [entry!],
      target: "browser",
      format: "esm",
      minify: values.minify,
      external: ["flux:*", "srt:*"],
      define,
      loader: { ".svg": "text" },
      plugins: [solidPlugin()],
    })
  } catch (e) {
    console.error("[cli] compile error:\n", e)
    return null
  }

  if (result?.success) {
    return result
  }

  if (result) {
    for (let msg of result?.logs) console.error(msg)
  }
  return null
}

export async function bundleTo(outfile: string) {
  let result = await bundle()
  if (!result) {
    console.error("Build failed")
    process.exit(1)
  }
  for (let output of result.outputs) {
    await Bun.write(outfile, output)
  }
  return result
}

// Bundle for the bare Flux runtime: no Solid plugin, flux: modules stay external.
export async function bundleFlux(entry: string): Promise<string> {
  let result = await Bun.build({
    entrypoints: [entry],
    target: "browser",
    format: "esm",
    minify: values.minify,
    external: ["flux:*"],
  })
  if (!result.success) {
    for (let msg of result.logs) console.error(msg)
    console.error("Build failed")
    process.exit(1)
  }
  let jsCode = ""
  for (let output of result.outputs) jsCode += await output.text()
  return jsCode
}

// Bundle for the SolidRT runtime via the standard Solid-aware bundler.
export async function bundleSolid(): Promise<string> {
  let result = await bundle()
  if (!result) {
    console.error("Build failed")
    process.exit(1)
  }
  let jsCode = ""
  for (let output of result.outputs) jsCode += await output.text()
  return jsCode
}

// Compile JS source to QuickJS bytecode via the fluxc binary.
export async function compileToBytecode(jsCode: string): Promise<Buffer> {
  let compiler = requireBinary("fluxc")
  let proc = Bun.spawn([compiler], {
    stdin: new Blob([jsCode]),
    stdout: "pipe",
    stderr: "inherit",
  })
  let [bytecode, code] = await Promise.all([new Response(proc.stdout).arrayBuffer(), proc.exited])
  if (code !== 0) process.exit(code)
  return Buffer.from(bytecode)
}