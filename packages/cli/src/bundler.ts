import { solidPlugin } from "./bun-plugin-solid"
import { values, source } from "./args"
import { state, print } from "./util"

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
      external: ["flux:*"],
      define,
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