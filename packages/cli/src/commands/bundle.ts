import { values, source, isPrebuilt } from "../args"
import { bundle, bundleTo, bundleFlux, compileToBytecode } from "../bundler"
import { resolve } from "path"

// Write to stdout and resolve only once the whole payload is flushed.
// process.stdout.write to a pipe is async and applies backpressure; the
// callback fires after every byte is drained, so it is safe to exit after.
function writeStdout(data: string): Promise<void> {
  return new Promise((resolve, reject) => {
    process.stdout.write(data, (err) => (err ? reject(err) : resolve()))
  })
}

// Compile JS to a .srt.bin file and report its size.
async function writeBytecode(jsCode: string, outfile: string) {
  let bytecode = await compileToBytecode(jsCode)
  await Bun.write(outfile, bytecode)
  let binSize = (await Bun.file(outfile).stat()).size
  console.log(`>> wrote ${binSize} bytes to ${outfile}`)
}

export async function runBundleCommand() {
  if (values.flux) {
    let baseName = values.output ?? source!.replace(/\.[jt]s$/, "")
    let jsCode = await bundleFlux(source!)

    if (values.stdout) {
      await writeStdout(jsCode)
    } else if (values.compile) {
      await writeBytecode(jsCode, baseName + ".flux.bin")
    } else {
      let outfile = baseName + ".flux.js"
      await Bun.write(outfile, jsCode)
      console.log(`>> wrote ${jsCode.length} bytes to ${outfile}`)
    }
    process.exit()
  }

  if (isPrebuilt) {
    if (!source!.endsWith(".srt.js")) {
      console.error("Can only compile .srt.js files. .srt.bin is already compiled.")
      process.exit(1)
    }
    let jsFile = resolve(source!)
    let binOut = jsFile.replace(/\.srt\.js$/, ".srt.bin").replace(/\.js$/, ".bin")
    await writeBytecode(await Bun.file(jsFile).text(), binOut)
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
      await writeStdout(await output.text())
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
    await writeBytecode(jsCode, baseName + ".srt.bin")
    process.exit()
  }

  let jsOutfile = baseName + ".srt.js"
  let result = await bundleTo(jsOutfile)
  for (let output of result.outputs) {
    console.log(`>> wrote ${output.size} bytes to ${jsOutfile}`)
  }
  process.exit()
}