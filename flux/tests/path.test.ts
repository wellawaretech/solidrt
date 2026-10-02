// flux:path is a lexical path module: resolveWithin fuses normalization
// with a containment check, join concatenates and normalizes segments,
// basename/dirname/extname split a path the way Node's do, relative leads
// from one path to another and matchesGlob matches a path against a glob
// pattern. All are pure string operations (no filesystem access), so these
// tests assert on output for fixed inputs rather than touching disk. The
// separator is the platform's, so the cases assume unix separators and are
// registered off Windows only.
import { expect, test } from "flux:test"
import { basename, dirname, extname, join, matchesGlob, relative, resolveWithin } from "flux:path"
import { platform } from "flux:process"

if (platform !== "win32") {
  test("resolveWithin returns the contained path", () => {
    expect(resolveWithin("/srv/www", "index.html")).toBe("/srv/www/index.html")
  })

  test("resolveWithin normalizes an interior ..", () => {
    // A `..` that stays inside the root is allowed and normalized away.
    expect(resolveWithin("/srv/www", "a/../b.html")).toBe("/srv/www/b.html")
  })

  // A rejected resolve returns an explicit `null`, matching the documented
  // `string | null` contract.

  test("resolveWithin rejects an escape via ..", () => {
    expect(resolveWithin("/srv/www", "../secret")).toBeNull()
  })

  test("resolveWithin rejects an absolute path", () => {
    expect(resolveWithin("/srv/www", "/etc/passwd")).toBeNull()
  })

  test("resolveWithin rejects a sibling prefix", () => {
    // Component-wise containment: `<root>-secret` shares a string prefix
    // with the root but is not inside it, so it must be rejected.
    expect(resolveWithin("/srv/www", "../www-secret")).toBeNull()
  })

  test("join concatenates segments", () => {
    expect(join("assets", "img", "logo.png")).toBe("assets/img/logo.png")
  })

  test("join normalizes ..", () => {
    expect(join("a/b", "../c")).toBe("a/c")
    expect(join("/foo", "..", "bar")).toBe("/bar")
  })

  test("join skips empty segments", () => {
    // An empty segment must not introduce a separator that turns the join
    // absolute; the result stays relative.
    expect(join("foo", "", "bar")).toBe("foo/bar")
    expect(join("", "foo")).toBe("foo")
  })

  test("join of nothing is .", () => {
    expect(join()).toBe(".")
  })

  test("basename, dirname and extname split a path", () => {
    expect(basename("/a/model.gltf")).toBe("model.gltf")
    expect(basename("/a/model.gltf", ".gltf")).toBe("model")
    expect(dirname("/a/model.gltf")).toBe("/a")
    expect(dirname("model.gltf")).toBe(".")
    expect(extname("/a/model.gltf")).toBe(".gltf")
    expect(extname("model")).toBe("")
  })

  // A wrapper forwards its own optional parameter as it got it, so an
  // explicit undefined reads as "no ext".
  test("basename takes an explicit undefined ext", () => {
    expect(basename("/a/model.gltf", undefined)).toBe("model.gltf")
  })

  test("relative leads from one path to another", () => {
    expect(relative("/a/b", "/a/c/d.txt")).toBe("../c/d.txt")
    expect(relative("/a/b", "/a/b/c")).toBe("c")
    expect(relative("/a/b", "/a/b")).toBe("")
    // Against the cwd on both sides, so the cwd cancels out.
    expect(relative(".", "assets/a.png")).toBe("assets/a.png")
  })

  test("matchesGlob matches whole paths", () => {
    expect(matchesGlob("assets/lion_head.png", "assets/lion_*.png")).toBe(true)
    expect(matchesGlob("assets/sub/lion_head.png", "assets/lion_*.png")).toBe(false)
    expect(matchesGlob("assets/sub/deep/a.png", "assets/**/*.png")).toBe(true)
    expect(matchesGlob("assets/tile_7.png", "assets/tile_[0-9].png")).toBe(true)
    expect(matchesGlob("assets/Tile_7.png", "assets/tile_?.png")).toBe(false)
  })

  test("matchesGlob throws on a malformed pattern", () => {
    expect(() => matchesGlob("ab", "a[b")).toThrow(/^matchesGlob: the pattern "a\[b" is malformed at character 1: invalid range pattern$/)
  })
}
