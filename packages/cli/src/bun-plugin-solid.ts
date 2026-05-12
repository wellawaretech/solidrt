import { transformAsync } from "@babel/core"
import ts from "@babel/preset-typescript"
import solid from "babel-preset-solid"
import { type BunPlugin } from "bun"
import { resolve } from "path"

// let ioDevPath = resolve(import.meta.dir, "io-dev.ts")

export function solidPlugin(opts: { devBase?: string } = {}): BunPlugin {
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

      // // -- dev-mode qjs:io rewrite ----------------------------------
      // // In dev, user code's `import * as io from "qjs:io"` is redirected
      // // to a wrapper that proxies non-http targets through the dev
      // // server. The wrapper itself imports `qjs:io` -- that one import
      // // is short-circuited to `external` to break the cycle.
      // if (opts.devBase) {
      //   build.onResolve({ filter: /^qjs:io$/ }, (args) => {
      //     if (args.importer === ioDevPath) {
      //       return { path: "qjs:io", external: true }
      //     }
      //     return { path: ioDevPath }
      //   })
      // }
    },
  }
}
