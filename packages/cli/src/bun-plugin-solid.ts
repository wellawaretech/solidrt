import { transformAsync } from "@babel/core"
import ts from "@babel/preset-typescript"
import solid from "babel-preset-solid"
import { type BunPlugin } from "bun"

export function solidPlugin(): BunPlugin {
  return {
    name: "bun-plugin-solid",
    setup: (build) => {
      build.onLoad({ filter: /\.(js|ts)x$/ }, async (args) => {
        let file = Bun.file(args.path)
        let code = await file.text()
        let transforms = await transformAsync(code, {
          filename: args.path,
          presets: [[solid, { moduleName: "@solidrt/core", generate: "universal" }], [ts]],
        })
        return { contents: transforms?.code ?? "", loader: "js" }
      })
    },
  }
}