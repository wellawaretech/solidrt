import { values, source, isPrebuilt } from "../args"
import { requireBinary } from "../util"
import { bundle, bundleTo } from "../bundler"
import { resolve } from "path"

async function compileJs(jsCode: string, outfile: string) {
  let compiler = requireBinary("fluxc")
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

export async function runBundleCommand() {
  if (isPrebuilt) {
    if (!source!.endsWith(".srt.js")) {
      console.error("Can only compile .srt.js files. .srt.bin is already compiled.")
      process.exit(1)
    }
    let binOut = await compileToBytecode(resolve(source!))
    let binSize = (await Bun.file(binOut).stat()).size
    console.log(`>> wrote ${binSize} bytes to ${binOut}`)
    process.exit()
  }

  let baseName = values.output ?? source!.replace(/\.[jt]sx?$/, "")

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
    let binSize = (await Bun.file(binOutfile).stat()).size
    console.log(`>> wrote ${binSize} bytes to ${binOutfile}`)
    process.exit()
  }

  let jsOutfile = baseName + ".srt.js"
  let result = await bundleTo(jsOutfile)
  for (let output of result.outputs) {
    console.log(`>> wrote ${output.size} bytes to ${jsOutfile}`)
  }
  process.exit()
}