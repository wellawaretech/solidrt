import { dir } from "flux:fs"
import { state } from "./state"
import { dirname } from "./mode"
import { rebuildAndBroadcast, showBuildFailure } from "./rebuild"

// Reload-on-save. Not a directory watch: the watch set is the bundle's own
// input list (BundleOutput.inputs: the app's modules, the dependency modules
// bundled in, inlined files, package.json, tsconfig.json), so a source file
// the running app does not import, or a dependency it does not use, never
// triggers anything, while an edit to a workspace package it does import
// does. Each input's parent directory is watched (never the file itself: an
// editor's atomic save replaces the inode, which a file watch would lose)
// and events are matched by name within that directory. On top of that, in
// project mode, the whole assets/ tree: the manifest lists the tree, so any
// change to it is a new version. While the last build failed the input list
// is gone, so the source directory is watched as a whole until a build
// succeeds again (the missing-import case: the file the build waits for is
// not in any list yet). Every hit debounces into the one
// rebuildAndBroadcast() path, which re-arms the watch from the new inputs.
// Paused by the MCP pause_watch tool while an agent edits (state.watchPaused):
// changes made meanwhile are not pushed; the agent's reload is.

const DEBOUNCE_MS = 100
const SOURCE_EXT = /\.(tsx?|jsx?)$/

let offs: (() => void)[] = []
let timer: ReturnType<typeof setTimeout> | null = null
let changed = new Set<string>()
let building = false
let dirty = false

function basename(path: string): string {
  return path.slice(Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\")) + 1)
}

// A path with a dot component (below `root` when given) is never a source:
// .srt-data in particular is this server's own output, and rebuilding on it
// would rebuild forever. node_modules is the same in file mode, where the
// source directory may be a project root.
function isToolingPath(path: string, root: string): boolean {
  let rel = path.startsWith(root) ? path.slice(root.length) : path
  return rel.split(/[\\/]/).some((part) => part.startsWith(".") || part === "node_modules")
}

/** Arm the watch from a build's inputs; null (a failed build) watches the source tree as a whole. */
export function armWatcher(inputs: string[] | null) {
  stopWatcher()
  let config = state.config
  let hit = (path: string) => onChange(path)
  try {
    if (inputs) {
      let byDir = new Map<string, Set<string>>()
      for (let input of inputs) {
        let parent = dirname(input)
        let names = byDir.get(parent)
        if (!names) byDir.set(parent, (names = new Set()))
        names.add(basename(input))
      }
      for (let [parent, names] of byDir) {
        offs.push(dir(parent).watch((e) => names.has(basename(e.path)) && hit(e.path)))
      }
    } else {
      let root = config.sourceDir
      offs.push(
        dir(root).watch((e) => SOURCE_EXT.test(e.path) && !isToolingPath(e.path, root) && hit(e.path), { recursive: true }),
      )
    }
    if (config.projectDir) {
      let assets = `${config.projectDir}/assets`
      dir(assets)
        .exists()
        .then((exists) => {
          if (exists && offs.length) offs.push(dir(assets).watch((e) => !isToolingPath(e.path, assets) && hit(e.path), { recursive: true }))
        })
    }
  } catch (e) {
    console.error(`[cli] Watch failed: ${e instanceof Error ? e.message : e}`)
    stopWatcher()
  }
}

/** Drop every watch; the engine loop can go idle. */
export function stopWatcher() {
  for (let off of offs) off()
  offs = []
  if (timer !== null) clearTimeout(timer)
  timer = null
  changed.clear()
}

function onChange(path: string) {
  if (state.watchPaused) return
  changed.add(path)
  if (building) {
    dirty = true
    return
  }
  if (timer !== null) clearTimeout(timer)
  timer = setTimeout(() => void rebuild(), DEBOUNCE_MS)
}

// One rebuild per burst; a change that lands while the bundle runs queues
// exactly one more, since the running build cannot have seen it.
async function rebuild() {
  timer = null
  building = true
  let root = state.config.projectDir ?? state.config.sourceDir
  let names = [...changed].map((p) => (p.startsWith(root) ? p.slice(root.length + 1) : p))
  changed.clear()
  console.log(`[cli] Change detected: ${names.slice(0, 3).join(", ")}${names.length > 3 ? ` (+${names.length - 3})` : ""}`)
  let error = await rebuildAndBroadcast()
  if (error) {
    console.error(error)
    showBuildFailure()
  }
  building = false
  if (dirty) {
    dirty = false
    onChange(names[0] ?? root)
  }
}
