// The decision of the bundler's `detached` pass (src/bundle/detached.ts), on
// its own: the Babel glue around it is covered end to end by the app test in
// packages/core/tests/detached-attribute.test.tsx.
import { test, expect } from "flux:test"
import { resolveDetached } from "../src/bundle/detached.ts"

let PAIRED = ["view", "rect", "oval", "line", "path", "texture", "text"]

test("a bare or {true} detached on a paired tag compiles to its d-* form", () => {
  for (let tag of PAIRED) expect(resolveDetached(tag, "true")).toEqual({ tag: "d-" + tag })
})

test("detached={false} keeps the laid-out tag", () => {
  expect(resolveDetached("rect", "false")).toEqual({ tag: "rect" })
})

test("a non-literal value is a compile error", () => {
  expect(resolveDetached("view", "other")).toEqual({ error: "Detached is set once, as a literal attribute" })
})

test("a d-* tag with detached points at the plain tag", () => {
  let error = { error: "<d-rect> is the detached form already, write <rect detached>" }
  expect(resolveDetached("d-rect", "true")).toEqual(error)
  expect(resolveDetached("d-rect", "other")).toEqual(error)
})

test("a host tag with no detached form is a compile error", () => {
  expect(resolveDetached("span", "true")).toEqual({ error: "<span> has no detached form" })
  expect(resolveDetached("window", "false")).toEqual({ error: "<window> has no detached form" })
})
