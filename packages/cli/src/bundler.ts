import { transformAsync } from "@babel/core"
import jsx from "@babel/plugin-syntax-jsx"
import ts from "@babel/preset-typescript"
import solid from "babel-preset-solid"
import { type BunPlugin, type BuildArtifact } from "bun"
import { readFileSync } from "node:fs"
import { dirname, resolve as resolvePath } from "node:path"
import { values, source } from "./args"
import { state, print, requireBinary } from "./util"

// Babel plugin: rewrite `import data from "./x" with { type: "binary" }` into an
// inline Uint8Array of the file's bytes. The import attribute is invisible to
// Bun's bundler and its plugins in this Bun version, so we handle it here in the
// transform where the AST still carries it. Inlining (rather than emitting a
// separate asset) keeps a single bundle output and hands JS a Uint8Array, which
// is what createImage and friends expect. Decoded at runtime via the global
// atob; for ASCII-extension files (.jpg/.png/...) we may add an attribute-free
// path later.
function binaryImport({ types: t }: { types: any }) {
  return {
    visitor: {
      ImportDeclaration(path: any, pluginState: any) {
        let attrs = path.node.attributes ?? path.node.assertions
        let isBinary = attrs?.some((a: any) => a.key.name === "type" && a.value.value === "binary")
        if (!isBinary) return

        let def = path.node.specifiers.find((s: any) => s.type === "ImportDefaultSpecifier")
        if (!def) {
          throw path.buildCodeFrameError(
            'A binary import needs a default import: import data from "./file" with { type: "binary" }',
          )
        }

        let importer = pluginState.file.opts.filename as string
        let abs = resolvePath(dirname(importer), path.node.source.value)
        let b64 = readFileSync(abs).toString("base64")

        // var <local> = Uint8Array.from(atob("<b64>"), c => c.charCodeAt(0))
        let expr = t.callExpression(t.memberExpression(t.identifier("Uint8Array"), t.identifier("from")), [
          t.callExpression(t.identifier("atob"), [t.stringLiteral(b64)]),
          t.arrowFunctionExpression(
            [t.identifier("c")],
            t.callExpression(t.memberExpression(t.identifier("c"), t.identifier("charCodeAt")), [t.numericLiteral(0)]),
          ),
        ])
        path.replaceWith(t.variableDeclaration("var", [t.variableDeclarator(t.identifier(def.local.name), expr)]))
      },
    },
  }
}

// Concatenate only the JS outputs (entry point plus any code-split chunks) of a
// build, skipping emitted asset outputs. Bun's file loader emits binary assets
// as extra outputs; the callers that flatten outputs into a single code string
// must not glue those raw bytes onto the program.
export async function codeFromOutputs(outputs: BuildArtifact[]): Promise<string> {
  let code = ""
  for (let o of outputs) {
    if (o.kind === "entry-point" || o.kind === "chunk") code += await o.text()
  }
  return code
}

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
          plugins: [jsx, binaryImport],
        })
        return { contents: transforms?.code ?? "", loader: "js" }
      })
    },
  }
}

export type BundleOptions = { entry: string; devBase?: string; dev: boolean; minify: boolean }

// The pure bundle: every input is explicit, so it runs identically in the srt
// (Bun) process and in the standalone bundle-cli subprocess the dev server
// spawns. It never touches the ambient args/state singletons and never prints
// progress (callers own that), so its stdout stays clean for subprocess use.
export async function bundleWith(opts: BundleOptions) {
  let define: Record<string, string> = {
    "process.env.NODE_ENV": opts.dev ? "development" : "production",
  }
  if (opts.devBase) define.__SRT_DEV_BASE__ = opts.devBase

  let result = null
  try {
    result = await Bun.build({
      entrypoints: [opts.entry],
      target: "browser",
      format: "esm",
      minify: opts.minify,
      external: ["flux:*", "srt:*"],
      define,
      loader: { ".svg": "text" },
      plugins: [solidPlugin()],
    })
  } catch (e) {
    console.error("[cli] compile error:\n", e)
    return null
  }

  if (result.success) return result
  for (let msg of result.logs) console.error(msg)
  return null
}

export async function bundle(entry = source) {
  let devBase = state.serverUrl ?? undefined
  let dev = !!devBase || values.dev
  // Keep stdout clean when the bundle itself is written to stdout.
  if (!values.stdout) print(`[cli] Bundling (${dev ? "development" : "production"})`)
  return bundleWith({ entry: entry!, devBase, dev, minify: values.minify })
}

export async function bundleTo(outfile: string) {
  let result = await bundle()
  if (!result) {
    console.error("Build failed")
    process.exit(1)
  }
  await Bun.write(outfile, await codeFromOutputs(result.outputs))
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
  return codeFromOutputs(result.outputs)
}

// Bundle for the SolidRT runtime via the standard Solid-aware bundler.
export async function bundleSolid(): Promise<string> {
  let result = await bundle()
  if (!result) {
    console.error("Build failed")
    process.exit(1)
  }
  return codeFromOutputs(result.outputs)
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