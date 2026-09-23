// Matching, formatting and link normalization over a small tree.
// `bun test packages/router/tests`.
import { describe, expect, test } from "bun:test"
import { createRootRoute, createRoute, formatPath, linkToPath, matchPath, place } from "../src/route.ts"

let noop = () => null

function tree() {
  let home = createRoute({ path: "/", component: noop })
  let section = createRoute({
    path: "/$section",
    params: {
      parse: (raw) => {
        if (raw.section !== "theme" && raw.section !== "about") throw new Error("unknown section")
        return { section: raw.section as "theme" | "about" }
      },
    },
    component: noop,
  })
  let settings = createRoute({ path: "/settings", component: noop, children: [section] })
  let item = createRoute({
    path: "/item/$id/$tab?",
    params: { parse: (raw) => ({ id: Number(raw.id), tab: raw.tab ?? "info" }) },
    component: noop,
  })
  let files = createRoute({ path: "/files/$", component: noop })
  let notFound = createRoute({ path: "/$", component: noop })
  let root = createRootRoute({ children: [home, settings, item, files, notFound] })
  return { root, home, settings, section, item, files, notFound }
}

describe("matchPath", () => {
  test("the root path takes the index route", () => {
    let t = tree()
    let m = matchPath(t.root, "/")!
    expect(m.map((x) => x.route)).toEqual([t.root, t.home])
  })

  test("a layout alone matches without its child", () => {
    let t = tree()
    let m = matchPath(t.root, "/settings")!
    expect(m.map((x) => x.route)).toEqual([t.root, t.settings])
  })

  test("nested params parse and merge root-first", () => {
    let t = tree()
    let m = matchPath(t.root, "/settings/theme")!
    expect(m.map((x) => x.route)).toEqual([t.root, t.settings, t.section])
    expect(m[2]!.params).toEqual({ section: "theme" })
  })

  test("a failed parse falls through to the catch-all", () => {
    let t = tree()
    let m = matchPath(t.root, "/settings/bogus")!
    expect(m[m.length - 1]!.route).toBe(t.notFound)
    expect(m[m.length - 1]!.params).toEqual({ rest: "settings/bogus" })
  })

  test("optional params", () => {
    let t = tree()
    expect(matchPath(t.root, "/item/42")![1]!.params).toEqual({ id: 42, tab: "info" })
    expect(matchPath(t.root, "/item/42/reviews")![1]!.params).toEqual({ id: 42, tab: "reviews" })
    expect(matchPath(t.root, "/item/42/reviews/extra")![1]!.route).toBe(t.notFound)
  })

  test("rest captures the remainder, decoded per segment", () => {
    let t = tree()
    let m = matchPath(t.root, "/files/a/b%20c")!
    expect(m[1]!.route).toBe(t.files)
    expect(m[1]!.params).toEqual({ rest: "a/b c" })
  })

  test("nothing matches without a catch-all", () => {
    let root = createRootRoute({ children: [createRoute({ path: "/only", component: noop })] })
    expect(matchPath(root, "/other")).toBeNull()
    expect(matchPath(root, "/")).not.toBeNull()
  })

  test("a pathless layout passes the path to its children", () => {
    let leaf = createRoute({ path: "/leaf", component: noop })
    let layout = createRoute({ path: "/", component: noop, children: [leaf] })
    let root = createRootRoute({ children: [layout] })
    expect(matchPath(root, "/leaf")!.map((x) => x.route)).toEqual([root, layout, leaf])
    expect(matchPath(root, "/")!.map((x) => x.route)).toEqual([root, layout])
  })
})

describe("place", () => {
  test("children are placed in order, after any already there", () => {
    let a = createRoute({ path: "/a", component: noop })
    let b = createRoute({ path: "/b", component: noop })
    let root = createRootRoute({ children: [a] })
    place(root, [b])
    expect(root.children).toEqual([a, b])
    expect(b.parent).toBe(root)
  })

  test("a route has one place", () => {
    let a = createRoute({ path: "/a", component: noop })
    createRootRoute({ children: [a] })
    expect(() => createRootRoute({ children: [a] })).toThrow(/already placed/)
  })
})

describe("formatPath", () => {
  test("round-trips params, encoding segments", () => {
    let t = tree()
    expect(formatPath(t.section, { section: "theme" })).toBe("/settings/theme")
    expect(formatPath(t.item, { id: 42 })).toBe("/item/42")
    expect(formatPath(t.item, { id: 42, tab: "a b" })).toBe("/item/42/a%20b")
    expect(formatPath(t.files, { rest: "a/b" })).toBe("/files/a/b")
    expect(formatPath(t.home)).toBe("/")
  })

  test("a missing required param throws", () => {
    let t = tree()
    expect(() => formatPath(t.section, {})).toThrow(/missing param "section"/)
  })
})

describe("route patterns", () => {
  test("rest and optional must be last", () => {
    expect(() => createRoute({ path: "/$/x", component: noop })).toThrow()
    expect(() => createRoute({ path: "/$a?/x", component: noop })).toThrow()
  })
})

describe("linkToPath", () => {
  test("custom schemes keep everything after the scheme", () => {
    expect(linkToPath("myapp://settings/theme")).toBe("/settings/theme")
    expect(linkToPath("com.example.app:/item/42")).toBe("/item/42")
    expect(linkToPath("myapp://")).toBe("/")
  })

  test("http links drop the host", () => {
    expect(linkToPath("https://example.com/item/42")).toBe("/item/42")
    expect(linkToPath("HTTP://example.com")).toBe("/")
  })

  test("query and fragment are dropped, bare paths pass", () => {
    expect(linkToPath("myapp://a/b?x=1#y")).toBe("/a/b")
    expect(linkToPath("/settings")).toBe("/settings")
    expect(linkToPath("settings")).toBe("/settings")
  })
})
