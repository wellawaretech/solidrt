// Test discovery: which folders `sol test` walks and what a test file is
// called (okf/done/test-harness.md). One tests/ folder per package or
// project, never beside the sources; a fixture project under a tests/
// folder (tests/fixtures/<app>/, with a tests/ folder of its own) is a
// project like any other, so the walk goes on below a tests/ folder.
//
// Another project is not entered. A folder with a package.json of its own
// that is neither the walk's project nor one of its workspace members is
// another project (a vendored checkout beside a custom runtime, a scratch
// app), whose suites belong to its own `sol test`; under a tests/ folder the
// rule is off, since fixtures are projects on purpose. The walk's project is
// the nearest package.json at or above the target, the usual workspace root,
// so `sol test packages/x` inside a checkout still knows the checkout's
// members. Dependencies, build output and dot folders are never entered.
//
// Pure over `Tree`, the few reads it makes, so the rule is tested on the
// flux binary like the rest of the cli's tests (tests/discover.test.ts);
// main.ts binds it to the real filesystem.

export const TESTS_DIR = "tests"
export const TEST_FILE = /\.test\.tsx?$/
// Folders never entered: dependencies and build output.
const SKIPPED_DIRS = ["node_modules", "dist", "target"]

export type Tree = {
  /** The subfolders of `dir` by name, sorted. */
  dirs(dir: string): string[]
  /** Whether `dir` holds a package.json. */
  isProject(dir: string): boolean
  /** Whether `dir`, inside the project at `root`, is one of its workspace members. */
  isWorkspaceMember(root: string, dir: string): boolean
  /** The test files in `dir` (`TEST_FILE` names), sorted, as paths. */
  testFiles(dir: string): string[]
}

/**
 * The test files under `target`, in walk order, and the other projects the
 * walk did not enter, so a run that finds nothing can say what it passed
 * over.
 */
export function discover(target: string, tree: Tree): { files: string[]; skipped: string[] } {
  let found = { files: [] as string[], skipped: [] as string[] }
  walk(target, projectRootOf(target, tree), segmentsOf(target).includes(TESTS_DIR), tree, found)
  return found
}

function walk(dir: string, root: string | null, inTests: boolean, tree: Tree, found: { files: string[]; skipped: string[] }) {
  if (segmentsOf(dir).at(-1) === TESTS_DIR) found.files.push(...tree.testFiles(dir))
  for (let name of tree.dirs(dir)) {
    if (name.startsWith(".") || SKIPPED_DIRS.includes(name)) continue
    let child = `${dir}/${name}`
    if (!inTests && tree.isProject(child) && !(root !== null && tree.isWorkspaceMember(root, child))) {
      found.skipped.push(child)
      continue
    }
    walk(child, root, inTests || name === TESTS_DIR, tree, found)
  }
}

// The nearest folder at or above `dir` with a package.json, or null.
function projectRootOf(dir: string, tree: Tree): string | null {
  for (let at: string | null = dir; at !== null; at = parentOf(at)) {
    if (tree.isProject(at)) return at
  }
  return null
}

// The parent of a path, null at the root ("/" or a drive); no node:path here.
function parentOf(path: string): string | null {
  let i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"))
  return i <= 0 ? null : path.slice(0, i)
}

function segmentsOf(path: string): string[] {
  return path.split(/[\\/]/).filter((segment) => segment !== "")
}
