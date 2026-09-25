// The navigation state over a tabbed tree: where paths land, push, replace,
// reset, back, the blocker scopes a move touches, and the initial state.
// `bun test packages/router/tests`.
import { describe, expect, test } from "bun:test"
import { createRootRoute, createRoute } from "../src/route.ts"
import { back, changed, currentPath, findTabs, initialState, push, replace, reset, resolve } from "../src/stack.ts"
import type { NavState, Target } from "../src/stack.ts"

let noop = () => null

// /            tabs: /feed (index + /post/$id), /search, /profile
// /viewer/$id  full window, above the tabs
// /login       full window
// /$           not found
function tree() {
  let post = createRoute({ path: "/post/$id", component: noop })
  let feed = createRoute({ path: "/feed", children: [createRoute({ path: "/", component: noop }), post] })
  let search = createRoute({ path: "/search", component: noop })
  let profile = createRoute({ path: "/profile", component: noop })
  let main = createRoute({ path: "/", tabs: true, component: noop, children: [feed, search, profile] })
  let viewer = createRoute({ path: "/viewer/$id", component: noop })
  let login = createRoute({ path: "/login", component: noop })
  let notFound = createRoute({ path: "/$", component: noop })
  let root = createRootRoute({ children: [main, viewer, login, notFound] })
  let tabs = findTabs(root)!
  let to = (path: string): Target => {
    let t = resolve(root, tabs, path)
    if (!t) throw new Error(`no route for ${path}`)
    return t
  }
  let home = initialState(root, tabs, undefined, (p) => p)
  return { root, tabs, to, home, main, feed }
}

describe("tabs rules", () => {
  test("a tab's path is literal", () => {
    expect(() =>
      createRoute({ path: "/", tabs: true, children: [createRoute({ path: "/$id", component: noop })] }),
    ).toThrow(/literal/)
    expect(() =>
      createRoute({ path: "/", tabs: true, children: [createRoute({ path: "/$", component: noop })] }),
    ).toThrow(/literal/)
  })

  test("a tab is declared once", () => {
    expect(() =>
      createRoute({
        path: "/",
        tabs: true,
        children: [createRoute({ path: "/a", component: noop }), createRoute({ path: "/a", component: noop })],
      }),
    ).toThrow(/twice/)
  })

  test("no tabs inside a tab, checked at the placement that closes the chain", () => {
    let inner = createRoute({ path: "/", tabs: true, children: [createRoute({ path: "/x", component: noop })] })
    let tab = createRoute({ path: "/a", children: [inner] })
    expect(() => createRoute({ path: "/", tabs: true, children: [tab] })).toThrow(/inside a tab/)
  })

  test("no params above a tabs route", () => {
    let tabs = createRoute({ path: "/", tabs: true, children: [createRoute({ path: "/a", component: noop })] })
    expect(() => createRoute({ path: "/org/$id", children: [tabs] })).toThrow(/no params above/)
  })

  test("one tabs route per tree", () => {
    let a = createRoute({ path: "/a", tabs: true, children: [createRoute({ path: "/x", component: noop })] })
    let b = createRoute({ path: "/b", tabs: true, children: [createRoute({ path: "/y", component: noop })] })
    expect(() => findTabs(createRootRoute({ children: [a, b] }))).toThrow(/one tabs route/)
  })

  test("findTabs describes the tabs", () => {
    let { tabs, main } = tree()
    expect(tabs.route).toBe(main)
    expect(tabs.path).toBe("/")
    expect(tabs.roots).toEqual(["/feed", "/search", "/profile"])
  })
})

describe("resolve", () => {
  test("a path in a tab lands in it; the tabs route itself lands on the first tab", () => {
    let { to } = tree()
    expect(to("/feed/post/1").tab).toBe("/feed")
    expect(to("/search").tab).toBe("/search")
    expect(to("/viewer/1").tab).toBeNull()
    expect(to("/")).toMatchObject({ path: "/feed", tab: "/feed" })
  })
})

describe("initial state", () => {
  test("undefined starts on the first tab", () => {
    let { home } = tree()
    expect(home).toEqual({
      stack: ["/"],
      tabs: { active: "/feed", stacks: { "/feed": ["/feed"], "/search": ["/search"], "/profile": ["/profile"] } },
    })
  })

  test("a screen in a tab lands with the tab's root beneath it", () => {
    let { root, tabs } = tree()
    let s = initialState(root, tabs, "/feed/post/7", (p) => p)
    expect(s.stack).toEqual(["/"])
    expect(s.tabs?.active).toBe("/feed")
    expect(s.tabs?.stacks["/feed"]).toEqual(["/feed", "/feed/post/7"])
  })

  test("a full-window screen stands alone", () => {
    let { root, tabs } = tree()
    let s = initialState(root, tabs, "/login", (p) => p)
    expect(s.stack).toEqual(["/login"])
    expect(s.tabs?.active).toBe("/feed")
  })

  test("a list is pushed in order", () => {
    let { root, tabs } = tree()
    let s = initialState(root, tabs, ["/search", "/viewer/3"], (p) => p)
    expect(s.stack).toEqual(["/", "/viewer/3"])
    expect(s.tabs?.active).toBe("/search")
  })

  test("a saved object that fits the tree is taken as given", () => {
    let { root, tabs } = tree()
    let saved = {
      stack: ["/", "/viewer/2"],
      tabs: {
        active: "/profile",
        stacks: { "/feed": ["/feed", "/feed/post/1"], "/search": ["/search"], "/profile": ["/profile"] },
      },
    }
    expect(initialState(root, tabs, saved, (p) => p)).toEqual(saved)
  })

  test("a saved object that does not fit is not resumed at all", () => {
    let { root, tabs, home } = tree()
    let good = { "/feed": ["/feed"], "/search": ["/search"], "/profile": ["/profile"] }
    let cases = [
      { stack: ["/", "/search"], tabs: { active: "/feed", stacks: good } },
      { stack: ["/"], tabs: { active: "/nope", stacks: good } },
      { stack: ["/"], tabs: { active: "/feed", stacks: { ...good, "/feed": ["/feed/post/1"] } } },
      { stack: ["/"], tabs: { active: "/feed", stacks: { ...good, "/search": ["/search", "/viewer/9"] } } },
      { stack: ["/"], tabs: { active: "/feed", stacks: { "/feed": ["/feed"] } } },
      { stack: [], tabs: { active: "/feed", stacks: good } },
    ]
    for (let saved of cases) expect(initialState(root, tabs, saved, (p) => p)).toEqual(home)
  })

  test("without tabs the state is the plain stack", () => {
    let root = createRootRoute({ children: [createRoute({ path: "/a", component: noop })] })
    expect(initialState(root, null, ["/a", "/a"], (p) => p)).toEqual({ stack: ["/a", "/a"], tabs: null })
    expect(initialState(root, null, "/zzz", (p) => p)).toEqual({ stack: ["/"], tabs: null })
  })
})

describe("push", () => {
  test("into the active tab", () => {
    let { home, tabs, to } = tree()
    let s = push(home, tabs, to("/feed/post/1"))
    expect(s.tabs?.stacks["/feed"]).toEqual(["/feed", "/feed/post/1"])
    expect(currentPath(s, tabs)).toBe("/feed/post/1")
  })

  test("into another tab shows it and keeps the first as left", () => {
    let { home, tabs, to } = tree()
    let s = push(push(home, tabs, to("/feed/post/1")), tabs, to("/search"))
    expect(s.tabs?.active).toBe("/search")
    expect(s.tabs?.stacks["/feed"]).toEqual(["/feed", "/feed/post/1"])
    expect(currentPath(s, tabs)).toBe("/search")
  })

  test("the active tab's root pops that tab to it; another tab's root shows it as left", () => {
    let { home, tabs, to } = tree()
    let deep = push(home, tabs, to("/feed/post/1"))
    expect(push(deep, tabs, to("/feed")).tabs?.stacks["/feed"]).toEqual(["/feed"])
    let other = push(deep, tabs, to("/search"))
    let again = push(other, tabs, to("/feed"))
    expect(again.tabs?.active).toBe("/feed")
    expect(again.tabs?.stacks["/feed"]).toEqual(["/feed", "/feed/post/1"])
    expect(changed(other, again)).toEqual([])
  })

  test("a tab's root from a full-window screen above the tabs shows it as left", () => {
    let { home, tabs, to } = tree()
    let s = push(push(home, tabs, to("/feed/post/1")), tabs, to("/viewer/1"))
    let t = push(s, tabs, to("/feed"))
    expect(t.stack).toEqual(["/"])
    expect(t.tabs?.stacks["/feed"]).toEqual(["/feed", "/feed/post/1"])
  })

  test("a full-window screen goes above the tabs; a tab screen after it pops back to them", () => {
    let { home, tabs, to } = tree()
    let s = push(home, tabs, to("/viewer/4"))
    expect(s.stack).toEqual(["/", "/viewer/4"])
    expect(currentPath(s, tabs)).toBe("/viewer/4")
    let t = push(s, tabs, to("/profile"))
    expect(t.stack).toEqual(["/"])
    expect(t.tabs?.active).toBe("/profile")
  })

  test("a tab screen with the tabs gone from the parent stack brings them back", () => {
    let { tabs, to } = tree()
    let s = push(reset(tabs, to("/login")), tabs, to("/search"))
    expect(s.stack).toEqual(["/login", "/"])
    expect(s.tabs?.active).toBe("/search")
  })
})

describe("replace", () => {
  test("swaps a tab's top, never its root", () => {
    let { home, tabs, to } = tree()
    let s = replace(home, tabs, to("/feed/post/1"))
    expect(s.tabs?.stacks["/feed"]).toEqual(["/feed", "/feed/post/1"])
    let t = replace(s, tabs, to("/feed/post/2"))
    expect(t.tabs?.stacks["/feed"]).toEqual(["/feed", "/feed/post/2"])
  })

  test("a full-window target swaps the parent's top, the tab bar included", () => {
    let { home, tabs, to } = tree()
    let s = replace(home, tabs, to("/login"))
    expect(s.stack).toEqual(["/login"])
    let t = replace(push(home, tabs, to("/viewer/1")), tabs, to("/viewer/2"))
    expect(t.stack).toEqual(["/", "/viewer/2"])
  })
})

describe("reset", () => {
  test("every tab to its root, the target's root beneath it", () => {
    let { home, tabs, to } = tree()
    let s = push(push(home, tabs, to("/feed/post/1")), tabs, to("/viewer/1"))
    let r = reset(tabs, to("/search"))
    expect(r.stack).toEqual(["/"])
    expect(r.tabs?.active).toBe("/search")
    expect(r.tabs?.stacks["/feed"]).toEqual(["/feed"])
    expect(changed(s, r)).toEqual([null, "/feed", "/search", "/profile"])
  })
})

describe("back", () => {
  test("pops the active tab, then switches to the first tab, then the parent, then nothing", () => {
    let { home, tabs, to } = tree()
    let s = push(push(reset(tabs, to("/login")), tabs, to("/search")), tabs, to("/feed/post/1"))
    expect(s.stack).toEqual(["/login", "/"])
    let a = back(s, tabs)!
    expect(a.tabs?.stacks["/feed"]).toEqual(["/feed"])
    expect(a.tabs?.active).toBe("/feed")
    let b = back(push(a, tabs, to("/search")), tabs)!
    expect(b.tabs?.active).toBe("/feed")
    let c = back(b, tabs)!
    expect(c.stack).toEqual(["/login"])
    expect(back(c, tabs)).toBeNull()
  })

  test("a full-window screen pops before the tabs are touched", () => {
    let { home, tabs, to } = tree()
    let s = push(push(home, tabs, to("/feed/post/1")), tabs, to("/viewer/1"))
    let a = back(s, tabs)!
    expect(a.stack).toEqual(["/"])
    expect(a.tabs?.stacks["/feed"]).toEqual(["/feed", "/feed/post/1"])
  })

  test("at the first tab's root with nothing beneath there is nothing to pop", () => {
    let { home, tabs } = tree()
    expect(back(home, tabs)).toBeNull()
  })

  test("without tabs it is the plain pop", () => {
    let s: NavState = { stack: ["/a", "/b"], tabs: null }
    expect(back(s, null)).toEqual({ stack: ["/a"], tabs: null })
    expect(back({ stack: ["/a"], tabs: null }, null)).toBeNull()
  })
})

describe("changed", () => {
  test("a tab switch changes no stack; a push in a tab changes that tab only", () => {
    let { home, tabs, to } = tree()
    expect(changed(home, push(home, tabs, to("/search")))).toEqual([])
    expect(changed(home, push(home, tabs, to("/feed/post/1")))).toEqual(["/feed"])
    expect(changed(home, push(home, tabs, to("/viewer/1")))).toEqual([null])
    expect(changed(home, back(push(home, tabs, to("/search")), tabs)!)).toEqual([])
  })
})
