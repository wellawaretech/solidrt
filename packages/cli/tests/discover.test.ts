// The test discovery rule (src/test/discover.ts) over in-memory trees:
// which folders the walk enters, and which it leaves to their own `sol
// test`.
import { test, expect } from "flux:test"
import { discover, type Tree } from "../src/test/discover.ts"

// A tree from its paths: a folder per key, its subfolders, whether it is a
// project (holds a package.json), and the test file names in it. Workspace
// members are the project's `members` list, relative to it.
type Folder = { project?: boolean; members?: string[]; tests?: string[] }

function treeOf(folders: Record<string, Folder>): Tree {
  let childrenOf = (dir: string) =>
    Object.keys(folders)
      .filter((path) => path.startsWith(dir + "/") && !path.slice(dir.length + 1).includes("/"))
      .map((path) => path.slice(dir.length + 1))
      .sort()
  return {
    dirs: childrenOf,
    isProject: (dir) => folders[dir]?.project === true,
    isWorkspaceMember: (root, dir) => (folders[root]?.members ?? []).some((member) => `${root}/${member}` === dir),
    testFiles: (dir) => (folders[dir]?.tests ?? []).map((name) => `${dir}/${name}`),
  }
}

test("a tests/ folder's files, the project's own and its workspace members'", () => {
  let tree = treeOf({
    "/repo": { project: true, members: ["packages/a", "packages/b"] },
    "/repo/tests": { tests: ["root.test.ts"] },
    "/repo/packages": {},
    "/repo/packages/a": { project: true },
    "/repo/packages/a/tests": { tests: ["a.test.ts", "a2.test.tsx"] },
    "/repo/packages/b": { project: true },
    "/repo/packages/b/src": {},
    "/repo/packages/b/tests": { tests: ["b.test.ts"] },
  })
  expect(discover("/repo", tree)).toEqual({
    files: ["/repo/packages/a/tests/a.test.ts", "/repo/packages/a/tests/a2.test.tsx", "/repo/packages/b/tests/b.test.ts", "/repo/tests/root.test.ts"],
    skipped: [],
  })
})

test("another project beside the walk is not entered", () => {
  let tree = treeOf({
    "/app": { project: true },
    "/app/tests": { tests: ["app.test.ts"] },
    "/app/runtime": {},
    "/app/runtime/vendor": {},
    "/app/runtime/vendor/solidrt": { project: true, members: ["packages/core"] },
    "/app/runtime/vendor/solidrt/tests": { tests: ["vendored.test.ts"] },
    "/app/runtime/vendor/solidrt/packages": {},
    "/app/runtime/vendor/solidrt/packages/core": { project: true },
    "/app/runtime/vendor/solidrt/packages/core/tests": { tests: ["core.test.ts"] },
    "/app/scratch": { project: true },
    "/app/scratch/tests": { tests: ["scratch.test.ts"] },
  })
  expect(discover("/app", tree)).toEqual({ files: ["/app/tests/app.test.ts"], skipped: ["/app/runtime/vendor/solidrt", "/app/scratch"] })
})

test("a fixture project under tests/ is walked, nested projects and all", () => {
  let tree = treeOf({
    "/pkg": { project: true },
    "/pkg/tests": { tests: ["pkg.test.ts"] },
    "/pkg/tests/fixtures": {},
    "/pkg/tests/fixtures/app": { project: true },
    "/pkg/tests/fixtures/app/tests": { tests: ["app.test.tsx"] },
    "/pkg/tests/fixtures/app/vendor": { project: true },
    "/pkg/tests/fixtures/app/vendor/tests": { tests: ["vendor.test.ts"] },
  })
  expect(discover("/pkg", tree).files).toEqual([
    "/pkg/tests/pkg.test.ts",
    "/pkg/tests/fixtures/app/tests/app.test.tsx",
    "/pkg/tests/fixtures/app/vendor/tests/vendor.test.ts",
  ])
})

test("a target inside a checkout takes the checkout as its project", () => {
  let tree = treeOf({
    "/repo": { project: true, members: ["packages/a", "packages/b"] },
    "/repo/packages": {},
    "/repo/packages/a": { project: true },
    "/repo/packages/a/tests": { tests: ["a.test.ts"] },
    "/repo/packages/b": { project: true },
    "/repo/packages/b/tests": { tests: ["b.test.ts"] },
    "/repo/packages/c": { project: true },
    "/repo/packages/c/tests": { tests: ["c.test.ts"] },
  })
  // The folder of the members: a, b are members, c is another project.
  expect(discover("/repo/packages", tree)).toEqual({
    files: ["/repo/packages/a/tests/a.test.ts", "/repo/packages/b/tests/b.test.ts"],
    skipped: ["/repo/packages/c"],
  })
  // The target itself is always walked.
  expect(discover("/repo/packages/c", tree).files).toEqual(["/repo/packages/c/tests/c.test.ts"])
})

test("a target under a tests/ folder is a fixture: the rule is off below it", () => {
  let tree = treeOf({
    "/pkg": { project: true },
    "/pkg/tests": {},
    "/pkg/tests/fixtures": {},
    "/pkg/tests/fixtures/app": { project: true },
    "/pkg/tests/fixtures/app/tests": { tests: ["app.test.ts"] },
    "/pkg/tests/fixtures/app/other": { project: true },
    "/pkg/tests/fixtures/app/other/tests": { tests: ["other.test.ts"] },
  })
  expect(discover("/pkg/tests/fixtures", tree)).toEqual({
    files: ["/pkg/tests/fixtures/app/other/tests/other.test.ts", "/pkg/tests/fixtures/app/tests/app.test.ts"],
    skipped: [],
  })
})

test("dependencies, build output and dot folders are never entered", () => {
  let tree = treeOf({
    "/pkg": { project: true },
    "/pkg/node_modules": {},
    "/pkg/node_modules/tests": { tests: ["dep.test.ts"] },
    "/pkg/dist": {},
    "/pkg/dist/tests": { tests: ["out.test.ts"] },
    "/pkg/target": {},
    "/pkg/target/tests": { tests: ["cargo.test.ts"] },
    "/pkg/.hidden": {},
    "/pkg/.hidden/tests": { tests: ["hidden.test.ts"] },
    "/pkg/tests": { tests: ["pkg.test.ts"] },
  })
  expect(discover("/pkg", tree)).toEqual({ files: ["/pkg/tests/pkg.test.ts"], skipped: [] })
})

test("no project above the target: every project below is another one", () => {
  let tree = treeOf({
    "/home": {},
    "/home/apps": {},
    "/home/apps/one": { project: true },
    "/home/apps/one/tests": { tests: ["one.test.ts"] },
    "/home/apps/tests": { tests: ["loose.test.ts"] },
  })
  expect(discover("/home/apps", tree)).toEqual({ files: ["/home/apps/tests/loose.test.ts"], skipped: ["/home/apps/one"] })
})
