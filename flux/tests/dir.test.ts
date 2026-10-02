// dir(path): entries(), exists(), watch(); realpath and glob. Fixtures are
// laid down in the sandbox through dir().create() and file().write().
import { expect, test } from "flux:test"
import { dir, file, glob, realpath } from "flux:fs"
import { basename, join } from "flux:path"

test("entries lists names and types", async () => {
  await dir("root/sub").create()
  await file("root/a.txt").write("a")
  await file("root/b.txt").write("b")
  let es = await dir("root").entries()
  es.sort((a, b) => (a.name < b.name ? -1 : 1))
  expect(es).toEqual([
    { name: "a.txt", type: "file" },
    { name: "b.txt", type: "file" },
    { name: "sub", type: "directory" },
  ])
})

test("exists is true for a directory and false for a file or a missing path", async () => {
  await dir("root").create()
  await file("root/f.txt").write("x")
  // exists() is is_dir, so a file path and a missing path are both false.
  expect(await dir("root").exists()).toBe(true)
  expect(await dir("root/f.txt").exists()).toBe(false)
  expect(await dir("root/no-such-dir").exists()).toBe(false)
})

test("entries on a missing directory rejects", async () => {
  await expect(dir("no-such-dir").entries()).rejects.toThrow()
})

test("realpath resolves a path and rejects a missing one", async () => {
  await dir("sub").create()
  let resolved = await realpath("sub/../sub")
  expect(resolved).toBe(await realpath("sub"))
  expect(resolved.includes("..")).toBe(false)
  expect(basename(resolved)).toBe("sub")
  await expect(realpath("nope")).rejects.toThrow()
})

test("watch throws for a missing directory", () => {
  expect(() => dir("no-such-dir").watch(() => {})).toThrow(Error)
  expect(() => dir("no-such-dir").watch(() => {})).toThrow(/^watch /)
})

test("glob lists the matching files from cwd", async () => {
  // glob(pattern, { cwd }) lists the files a pattern matches, as seen from
  // cwd, sorted; directories are walked and never listed.
  await dir("assets/ui").create()
  for (let path of ["assets/lion_head.png", "assets/lion_tail.png", "assets/ui/icon.png", "assets/base.jpg"]) {
    await file(path).write(path)
  }
  let cwd = "."
  // The pattern is written with "/"; the listed paths carry the platform's
  // separator, as flux:path's join does.
  expect(await glob("assets/lion_*.png", { cwd })).toEqual([join("assets", "lion_head.png"), join("assets", "lion_tail.png")])
  expect(await glob("assets/**", { cwd })).toEqual([
    join("assets", "base.jpg"),
    join("assets", "lion_head.png"),
    join("assets", "lion_tail.png"),
    join("assets", "ui", "icon.png"),
  ])
  expect(await glob("assets/tiger_*.png", { cwd })).toEqual([])
  expect(() => glob("assets/[", { cwd })).toThrow(/^glob: the pattern "assets\/\[" is malformed at character 7: invalid range pattern$/)
  // @ts-expect-error not a cwd; glob has to say so at runtime too
  expect(() => glob("assets/*", { cwd: 7 })).toThrow(/^glob: cwd must be a string$/)
})
