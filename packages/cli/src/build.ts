import { solidPlugin } from "./bun-plugin-solid"
import { values, source, isPrebuilt } from "./args"
import { requireBinary, state } from "./util"
import { resolve } from "path"

export async function bundle(entry = source) {
  let result = null

  let devBase = state.serverUrl ?? undefined
  let define: Record<string, string> = { "process.env.NODE_ENV": "production" }
  if (devBase) define.__SRT_DEV_BASE__ = JSON.stringify(devBase)

  try {
    result = await Bun.build({
      entrypoints: [entry!],
      target: "browser",
      format: "esm",
      minify: values.minify,
      external: ["qjs:*"],
      define,
      plugins: [solidPlugin({ devBase })],
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

async function compileJs(jsCode: string, outfile: string) {
  let compiler = requireBinary("flux")
  let proc = Bun.spawn([compiler], {
    stdin: new Blob([jsCode]),
    stdout: "pipe",
    stderr: "inherit",
  })
  let [bytecode, code] = await Promise.all([new Response(proc.stdout).arrayBuffer(), proc.exited])
  if (code !== 0) process.exit(code)
  await Bun.write(outfile, bytecode)
  return outfile
}

async function compileToBytecode(jsFile: string, outFile?: string) {
  let jsCode = await Bun.file(jsFile).text()
  let dest = outFile ?? jsFile.replace(/\.srt\.js$/, ".srt.bin").replace(/\.js$/, ".bin")
  return compileJs(jsCode, dest)
}

async function compileFromStdin(jsCode: string, outfile: string) {
  return compileJs(jsCode, outfile)
}

export async function runBuildCommand() {
  if (isPrebuilt) {
    if (!source!.endsWith(".srt.js")) {
      console.error("Can only compile .srt.js files. .srt.bin is already compiled.")
      process.exit(1)
    }
    await compileToBytecode(resolve(source!))
    process.exit()
  }

  let baseName = values.output ?? source!.replace(/\.tsx$/, "")

  if (values.stdout) {
    let result = await bundle()
    if (!result) {
      console.error("Build failed")
      process.exit(1)
    }
    for (let output of result.outputs) {
      process.stdout.write(await output.text())
    }
    process.exit()
  }

  if (values.compile) {
    let result = await bundle()
    if (!result) {
      console.error("Build failed")
      process.exit(1)
    }
    let jsCode = ""
    for (let output of result.outputs) {
      jsCode += await output.text()
    }
    let binOutfile = baseName + ".srt.bin"
    await compileFromStdin(jsCode, binOutfile)
    process.exit()
  }

  let jsOutfile = baseName + ".srt.js"
  let result = await bundleTo(jsOutfile)
  for (let output of result.outputs) {
    console.log(`>> wrote ${output.size} bytes to ${jsOutfile}`)
  }
  process.exit()
}