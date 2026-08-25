import { transformAsync } from "@babel/core"
import jsx from "@babel/plugin-syntax-jsx"
import ts from "@babel/preset-typescript"
import remapping from "@jridgewell/remapping"
import solid from "babel-preset-solid"
import { type BunPlugin, type BuildArtifact } from "bun"
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs"
import { dirname, join, relative, resolve as resolvePath, sep } from "node:path"
import { values } from "./args"
import type { Mode } from "./mode"
import { print, requireBinary } from "./util"
import { buildManifest, manifestAssetFor, type ManifestAsset } from "./project"

// Babel plugin: rewrite `import data from "./x" with { type: "binary" }` into an
// inline Uint8Array of the file's bytes, and `with { type: "text" }` into an
// inline string of its UTF-8 contents. The import attribute is invisible to
// Bun's bundler and its plugins in this Bun version, so we handle it here in the
// transform where the AST still carries it. Inlining (rather than emitting a
// separate asset) keeps a single bundle output and hands JS a Uint8Array, which
// is what createImage and friends expect. Binary is decoded at runtime via the
// global atob; for ASCII-extension files (.jpg/.png/...) we may add an
// attribute-free path later. Both attributes work on any extension, so shader
// and other text sources inline by attribute the same way bytes do; `.svg` is
// additionally text-loaded without an attribute (see Bun's `loader` below).
function inlineImport({ types: t }: { types: any }) {
  return {
    visitor: {
      ImportDeclaration(path: any, pluginState: any) {
        let attrs = path.node.attributes ?? path.node.assertions
        let kind = attrs?.find((a: any) => a.key.name === "type")?.value.value
        if (kind !== "binary" && kind !== "text") return

        let def = path.node.specifiers.find((s: any) => s.type === "ImportDefaultSpecifier")
        if (!def) {
          throw path.buildCodeFrameError(
            `A ${kind} import needs a default import: import data from "./file" with { type: "${kind}" }`,
          )
        }

        let importer = pluginState.file.opts.filename as string
        let abs = resolvePath(dirname(importer), path.node.source.value)

        // text: var <local> = "<contents>"
        // binary: var <local> = Uint8Array.from(atob("<b64>"), c => c.charCodeAt(0))
        let expr =
          kind === "text"
            ? t.stringLiteral(readFileSync(abs, "utf8"))
            : t.callExpression(t.memberExpression(t.identifier("Uint8Array"), t.identifier("from")), [
                t.callExpression(t.identifier("atob"), [t.stringLiteral(readFileSync(abs).toString("base64"))]),
                t.arrowFunctionExpression(
                  [t.identifier("c")],
                  t.callExpression(t.memberExpression(t.identifier("c"), t.identifier("charCodeAt")), [
                    t.numericLiteral(0),
                  ]),
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
async function codeFromOutputs(outputs: BuildArtifact[]): Promise<string> {
  let code = ""
  for (let o of outputs) {
    if (o.kind === "entry-point" || o.kind === "chunk") code += await o.text()
  }
  return code
}

// Bun build plugin that runs JSX/TSX through babel-preset-solid (universal
// generate, targeting @solidrt/core) plus the TS preset. `moduleName` only ends
// up as an import specifier in the emitted code, resolved from the app's tree at
// bundle time; the CLI itself never loads core, so core is deliberately neither
// a dependency nor a peer of this package (a second copy under the CLI could be
// hoisted over the app's and bundle two runtime instances). Plain .js/.ts app
// modules take the same path (solid is a no-op without JSX) so inlineImport
// can rewrite their `with { type: "binary" }` imports too; dependency code
// (node_modules) skips the babel detour and keeps Bun's native loaders.
// With `babelMaps`, each file's transform map (original -> babel output) is
// collected there, keyed by absolute path, for sourcemap composition later.
// `isolateEntry` is the one "use isolate" module this build may load (its own
// entry); loading any other one means a by-value import of an isolate module,
// which is a build error (see isolate modules below).
function solidPlugin(babelMaps?: Map<string, object>, isolateEntry?: string): BunPlugin {
  return {
    name: "bun-plugin-solid",
    setup: (build) => {
      build.onLoad({ filter: /\.(js|ts)x?$/ }, async (args) => {
        if (!/\.(js|ts)x$/.test(args.path) && args.path.includes("node_modules")) return
        let file = Bun.file(args.path)
        let code = await file.text()
        if (args.path !== isolateEntry && hasIsolateDirective(code)) {
          throw new Error(
            `${args.path} is a "use isolate" module: import its types only (import type * as W from "./...") and call it through isolate() from flux:isolate`,
          )
        }
        let transforms = await transformAsync(code, {
          filename: args.path,
          sourceMaps: !!babelMaps,
          presets: [[solid, { moduleName: "@solidrt/core", generate: "universal" }], [ts]],
          plugins: [jsx, inlineImport],
        })
        if (babelMaps && transforms?.map) babelMaps.set(args.path, transforms.map)
        return { contents: transforms?.code ?? "", loader: "js" }
      })
    },
  }
}

// Isolate modules (okf/done/isolates-and-ports.md): a source file whose first
// statement is the "use isolate" directive is the entry of its own bundle,
// run by flux:isolate in a second runtime. Its id is its path relative to
// the source root (the entry's directory) without extension; the bundle
// travels as the manifest asset isolates/<id>.js (dev) or .bin (pack). The
// main build never loads such a module (only `import type` reaches it), so
// the set is found by scanning the tree rather than by following imports.

// The directive is the first statement: leading whitespace, comments and a
// shebang may precede it, nothing else.
let ISOLATE_DIRECTIVE = /^(?:#![^\n]*\n)?(?:\s|\/\/[^\n]*|\/\*[\s\S]*?\*\/)*(?:"use isolate"|'use isolate')\s*(?:;|\n|$)/

export function hasIsolateDirective(code: string): boolean {
  return ISOLATE_DIRECTIVE.test(code)
}

let SKIP_DIRS = new Set(["node_modules", "dist"])

/**
 * Depth-first files under `root`, visited as (absolute path, forward-slash
 * path relative to root). Dotfiles and `skipDirs` are skipped; a missing
 * root visits nothing.
 */
export function walkFiles(root: string, visit: (abs: string, rel: string) => void, skipDirs?: Set<string>) {
  if (!existsSync(root)) return
  let walk = (dir: string) => {
    for (let entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name.startsWith(".") || skipDirs?.has(entry.name)) continue
      let abs = join(dir, entry.name)
      if (entry.isDirectory()) walk(abs)
      else if (entry.isFile()) visit(abs, relative(root, abs).split(sep).join("/"))
    }
  }
  walk(root)
}

export type IsolateModule = { id: string; path: string }

/** Every "use isolate" module under `root`, in id order. */
export function findIsolateModules(root: string): IsolateModule[] {
  let out: IsolateModule[] = []
  walkFiles(
    root,
    (abs, rel) => {
      if (!/\.(js|ts)x?$/.test(rel) || rel.endsWith(".d.ts")) return
      if (hasIsolateDirective(readFileSync(abs, "utf8"))) {
        out.push({ id: rel.replace(/\.(js|ts)x?$/, ""), path: abs })
      }
    },
    SKIP_DIRS,
  )
  out.sort((a, b) => (a.id < b.id ? -1 : 1))
  return out
}

/** The manifest asset path of an isolate bundle. */
export function isolateAssetPath(id: string, ext: "js" | "bin"): string {
  return `isolates/${id}.${ext}`
}

// `project` is the project root (mode.ts decides it, never a search), or
// null for a file on its own: no assets in the manifest and no isolate
// modules (isolates are a project feature).
export type BundleOptions = { entry: string; devBase?: string; dev: boolean; minify: boolean; project: string | null }

export type BundleResult = {
  code: string
  /** Composed sourcemap JSON (bundle -> original .tsx sources), dev builds only. */
  map: string | null
  /** Version manifest JSON for this bundle; clients install pushes under its hash. */
  manifest: string
  /** The app's isolate bundles, one per "use isolate" module, in id order; maps dev builds only. */
  isolates: { id: string; code: string; map: string | null }[]
}

/**
 * The bundle's sourcemaps keyed by the module name stack frames cite ("main"
 * for the app, the isolate id for each isolate), for the server's log remap.
 * Null when the build carries no maps (production builds).
 */
export function bundleMaps(result: BundleResult): Record<string, string> | null {
  let maps: Record<string, string> = {}
  if (result.map) maps.main = result.map
  for (let i of result.isolates) if (i.map) maps[i.id] = i.map
  return Object.keys(maps).length ? maps : null
}

// The pure bundle: every input is explicit, so it runs identically in the srt
// (Bun) process and in the standalone bundle-cli subprocess the dev server
// spawns. It never touches the ambient args/state singletons and never prints
// progress (callers own that), so its stdout stays clean for subprocess use.
export async function bundleWith(opts: BundleOptions): Promise<BundleResult | null> {
  // Define values are parsed as expressions, so string values need embedded
  // quotes - a bare word substitutes as an identifier and crashes at runtime.
  // import.meta.env.DEV is the solidrt build-mode constant (used by core's
  // leak sentinel; typed in core's types.d.ts). NODE_ENV stays defined as
  // ecosystem compat only: third-party libraries bundled into apps commonly
  // read it, and an unresolved `process` crashes at import time.
  let define: Record<string, string> = {
    "import.meta.env.DEV": opts.dev ? "true" : "false",
    "process.env.NODE_ENV": opts.dev ? '"development"' : '"production"',
  }
  if (opts.devBase) define.__SRT_DEV_BASE__ = opts.devBase

  // One Bun.build per entry: the app, then each isolate module as its own
  // self-contained bundle (splitting is off, so a helper both import gets
  // duplicated rather than shared). In dev every build gets a composed
  // sourcemap, keyed downstream by its module name.
  let build = async (entry: string, babelMaps?: Map<string, object>, isolateEntry?: string) => {
    let result = null
    try {
      result = await Bun.build({
        entrypoints: [entry],
        target: "browser",
        format: "esm",
        minify: opts.minify,
        external: ["flux:*", "srt:*"],
        define,
        loader: { ".svg": "text" },
        sourcemap: babelMaps ? "external" : "none",
        plugins: [solidPlugin(babelMaps, isolateEntry)],
      })
    } catch (e) {
      console.error("[cli] compile error:\n", e)
      return null
    }
    if (!result.success) {
      for (let msg of result.logs) console.error(msg)
      return null
    }
    return result
  }

  let babelMaps = opts.dev ? new Map<string, object>() : undefined
  let main = await build(opts.entry, babelMaps)
  if (!main) return null
  let code = await codeFromOutputs(main.outputs)

  let isolates: { id: string; code: string; map: string | null }[] = []
  let modules = opts.project === null ? [] : findIsolateModules(dirname(resolvePath(opts.entry)))
  for (let module of modules) {
    let moduleMaps = opts.dev ? new Map<string, object>() : undefined
    let result = await build(module.path, moduleMaps, module.path)
    if (!result) return null
    isolates.push({
      id: module.id,
      code: await codeFromOutputs(result.outputs),
      map: await composeMap(result.outputs, moduleMaps),
    })
  }

  return {
    code,
    map: await composeMap(main.outputs, babelMaps),
    manifest: buildManifest(code, opts.entry, isolateManifestAssets(isolates), opts.project),
    isolates,
  }
}

/** The manifest assets for a set of isolate bundles (dev form: isolates/<id>.js). */
export function isolateManifestAssets(isolates: { id: string; code: string }[]): ManifestAsset[] {
  return isolates.map((i) => manifestAssetFor(isolateAssetPath(i.id, "js"), Buffer.from(i.code, "utf8")))
}

// Write isolate bundles as `<dir>/<id>.js`, the shape the manifest lists and
// a prebuilt bundle's sibling isolates/ dir carries.
export function writeIsolates(dir: string, isolates: { id: string; code: string }[]) {
  for (let i of isolates) {
    let file = join(dir, `${i.id}.js`)
    mkdirSync(dirname(file), { recursive: true })
    writeFileSync(file, i.code)
  }
}

// Compose Bun's bundle map (babel output -> bundle) with the per-file Babel
// maps (original source -> babel output) so positions point at the .tsx
// sources. Bun ignores sourcemaps in plugin onLoad contents, so this second
// hop has to happen here. Only a single-artifact build gets a map: code
// splitting is off, and concatenated artifacts would invalidate offsets.
async function composeMap(outputs: BuildArtifact[], babelMaps?: Map<string, object>): Promise<string | null> {
  if (!babelMaps) return null
  let js = outputs.filter((o) => o.kind === "entry-point" || o.kind === "chunk")
  if (js.length !== 1 || !js[0]!.sourcemap) return null
  let bunMap = JSON.parse(await js[0]!.sourcemap.text())
  let composed = remapping(bunMap, (file: string) => {
    // Bun writes cwd-relative source paths; the babel maps are keyed by the
    // absolute path. Serve each map exactly once: remapping asks again for a
    // served map's own original source, and that lookup must return null.
    let abs = resolvePath(file)
    let map = babelMaps.get(abs)
    babelMaps.delete(abs)
    return (map as any) ?? null
  })
  return composed.toString()
}

// The one-shot commands' bundle (bundle, pack, render): production unless
// --dev, on what the cwd and the argument decide (mode.ts).
export async function bundle(mode: Mode) {
  let dev = values.dev
  // Keep stdout clean when the bundle itself is written to stdout.
  if (!values.stdout) print(`[cli] Bundling (${dev ? "development" : "production"})`)
  return bundleWith({ entry: mode.entry, dev, minify: values.minify, project: mode.projectDir })
}

// A flux entry's isolate modules: everything under its isolates/ dir, id =
// the path relative to that dir without extension. Standalone flux resolves
// isolates by location, not directive - module <id> is
// <entry dir>/isolates/<id>.bin or .js - so this is the discovery for
// bundling and packing flux scripts (which also lets a worker be .ts, unlike
// running from source).
export function findFluxIsolates(entryDir: string): IsolateModule[] {
  let out: IsolateModule[] = []
  walkFiles(join(entryDir, "isolates"), (abs, rel) => {
    if (/\.[jt]s$/.test(rel) && !rel.endsWith(".d.ts")) {
      out.push({ id: rel.replace(/\.[jt]s$/, ""), path: abs })
    }
  })
  out.sort((a, b) => (a.id < b.id ? -1 : 1))
  return out
}

// A bundle cannot carry its isolate bundles inside itself, so they travel in
// the isolates/ dir next to it: `<dir>/isolates/<id>.js` (or `.bin`,
// compiled) beside the bundle file - the shape the flux runtime and an
// installed version dir resolve. Writes are confined to bundle-owned output
// dirs (the ensureOutDir rule in the bundle command); loads read the dir
// from wherever the bundle sits.
export function bundleIsolatesDir(bundlePath: string): string {
  return join(dirname(bundlePath), "isolates")
}

/** A prebuilt bundle's isolate bundles, read back from its sibling dir; no dir means none. */
export function readPrebuiltIsolates(bundlePath: string): { id: string; code: string }[] {
  let out: { id: string; code: string }[] = []
  walkFiles(bundleIsolatesDir(resolvePath(bundlePath)), (abs, rel) => {
    if (rel.endsWith(".js")) {
      out.push({ id: rel.replace(/\.js$/, ""), code: readFileSync(abs, "utf8") })
    }
  })
  out.sort((a, b) => (a.id < b.id ? -1 : 1))
  return out
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

// Bundle for the SolidRT runtime via the standard Solid-aware bundler, or
// exit the command on a failed build.
export async function bundleSolid(mode: Mode): Promise<BundleResult> {
  let result = await bundle(mode)
  if (!result) {
    console.error("Build failed")
    process.exit(1)
  }
  return result
}

// Compile JS source to QuickJS bytecode via the fluxc binary. `moduleName` is
// what stack frames cite at runtime: "main" for the entry, the isolate id for
// an isolate bundle.
export async function compileToBytecode(jsCode: string, moduleName = "main"): Promise<Buffer> {
  let compiler = requireBinary("fluxc")
  let proc = Bun.spawn([compiler, moduleName], {
    stdin: new Blob([jsCode]),
    stdout: "pipe",
    stderr: "inherit",
  })
  let [bytecode, code] = await Promise.all([new Response(proc.stdout).arrayBuffer(), proc.exited])
  if (code !== 0) process.exit(code)
  return Buffer.from(bytecode)
}