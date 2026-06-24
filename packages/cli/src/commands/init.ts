import { mkdir, readdir } from "node:fs/promises"
import { basename, join, resolve } from "node:path"
import { source } from "../args"

// Write `contents` to `path` only if nothing is already there, so init never
// clobbers a file the user (or a previous run) created.
async function writeIfAbsent(path: string, contents: string): Promise<boolean> {
  if (await Bun.file(path).exists()) {
    console.log(`   skip  ${path} (exists)`)
    return false
  }
  await Bun.write(path, contents)
  console.log(`   write ${path}`)
  return true
}

// A valid npm package name derived from the target directory.
function packageName(dir: string): string {
  let name = basename(resolve(dir))
    .toLowerCase()
    .replace(/[^a-z0-9-_.]/g, "-")
  return name || "solidrt-app"
}

const TSCONFIG = `{
  "compilerOptions": {
    "jsx": "preserve",
    "jsxImportSource": "@solidrt/core",
    "moduleResolution": "bundler",
    "strict": true,
    "types": ["@solidrt/flux-types"]
  },
  "include": ["src"]
}
`

const GITIGNORE = `node_modules/
*.srt.js
*.srt.bin
.srt-cache.db
`

const INDEX_TSX = `import { render } from "@solidrt/core"
import { createSignal } from "@solidjs/signals"

function App() {
  let [count, setCount] = createSignal(0)
  return (
    <window flexDirection="column" alignItems="center" justifyContent="center" gap={24}>
      <d-rect color="#0b0f17" />
      <text color="#1f6feb" fontSize={48} fontWeight={800}>{count()}</text>
      <view
        onPointerDown={() => setCount((c) => c + 1)}
        padding={16}
        alignItems="center"
        justifyContent="center"
      >
        <d-rect color="#1f6feb" radius={12} />
        <text color="#ffffff" fontSize={20}>increment</text>
      </view>
    </window>
  )
}

render(() => <App />)
`

const AGENTS_MD = `# SolidRT app - agent notes

This project uses SolidRT: a custom SolidJS renderer that paints through a Rust
runtime. No DOM, no HTML, no CSS cascade. If you are an AI assistant, read this
before writing or editing code here.

Authoritative references ship inside the installed packages - read them:
- node_modules/@solidrt/core/AGENTS.md   - element/prop/reactivity model
- node_modules/@solidrt/cli/AGENTS.md    - running, bundling, headless verify
- node_modules/@solidrt/core/src/types.d.ts and jsx-runtime.d.ts - source of truth

<!-- Claude Code auto-imports these; other tools read the paths above. -->
@./node_modules/@solidrt/core/AGENTS.md
@./node_modules/@solidrt/cli/AGENTS.md

## The things assistants get wrong (this is not React/DOM)

1. SolidJS, not React: createSignal/createEffect from @solidjs/signals. No hooks,
   no virtual DOM, the component function does not re-run.
2. Host elements are lowercase intrinsics: window, view, text, rect, oval, path,
   texture, audio (+ d- variants). No div/span/img/button.
3. render(() => <App/>) once, top level. The root MUST be <window> or it throws.
4. Containers (window, view) DO NOT PAINT. Background = a draw primitive child
   placed behind the content, e.g. <d-rect color="..." />.
5. Paint color is the color prop (a CSS color string). No fill/stroke/background
   prop. Outline: drawStyle="stroke" + strokeWidth.
6. No onClick/onPress. A button is a <view>/<rect> with onPointerDown.
7. d- prefix = detached from layout: a plain element is laid out by Taffy, a
   d-element you position with x/y (omit to fill the parent = how backgrounds
   work).
8. Per-frame work: onFrame((tick, frame) => {}). Also onResize, onLayout.
9. Device/GPU via subpath imports: @solidrt/core/camera, /microphone, /gpu.
10. tsconfig needs jsx:"preserve" + jsxImportSource:"@solidrt/core". Solid peer
    deps are pinned betas - do not bump them casually.

## Run / verify

- bunx srt run src/index.tsx     - dev server + window (needs a display)
- bunx srt bundle src/index.tsx  - exit 0 means it compiles
- bunx srt record src/index.tsx --size 480x640 --duration 1 --fps 2 - headless
  render to PNG frames (proves it renders; see the cli AGENTS.md for where the
  frames land)
`

export async function runInitCommand() {
  // The target folder is required (validateArgs enforces it) and must be empty
  // or absent, so init can never overwrite files in an existing project.
  let dir = source!
  let existing = await readdir(dir).catch(() => null)
  if (existing && existing.length > 0) {
    console.error(`!! ${resolve(dir)} already exists and is not empty; choose a new folder name`)
    process.exit(1)
  }

  let name = packageName(dir)
  console.log(`>> scaffolding SolidRT project in ${resolve(dir)}`)

  await mkdir(join(dir, "src"), { recursive: true })

  let pkg = {
    name,
    type: "module",
    private: true,
    scripts: {
      dev: "srt run src/index.tsx",
      bundle: "srt bundle src/index.tsx",
    },
  }
  await writeIfAbsent(join(dir, "package.json"), JSON.stringify(pkg, null, 2) + "\n")
  await writeIfAbsent(join(dir, "tsconfig.json"), TSCONFIG)
  await writeIfAbsent(join(dir, ".gitignore"), GITIGNORE)
  await writeIfAbsent(join(dir, "AGENTS.md"), AGENTS_MD)
  await writeIfAbsent(join(dir, "src", "index.tsx"), INDEX_TSX)

  // Solid peer deps (@solidjs/signals etc., pinned betas) come in via bun's
  // peerDependencies resolution off @solidrt/core, so we do not pin them here.
  console.log("\n>> installing dependencies")
  let runtime = Bun.spawnSync(["bun", "add", "@solidrt/core"], {
    cwd: dir,
    stdout: "inherit",
    stderr: "inherit",
  })
  let dev = Bun.spawnSync(["bun", "add", "-d", "@solidrt/cli", "@solidrt/flux-types", "typescript@^6"], {
    cwd: dir,
    stdout: "inherit",
    stderr: "inherit",
  })
  if (runtime.exitCode !== 0 || dev.exitCode !== 0) {
    console.error("\n!! dependency install failed; retry with `bun add @solidrt/core` in the project")
    process.exit(1)
  }

  let prefix = dir === "." ? "" : `cd ${dir} && `
  console.log(`\n>> done. next:\n   ${prefix}bunx srt run src/index.tsx\n`)
  process.exit()
}