// What `sol init --runtime` writes (src/init/runtime.ts): the crate name
// from the app's name, the placeholders in the templates, and the three
// edits into the app's own files.
import { test, expect } from "flux:test"
import { RUNTIME_DIST, RUNTIME_TARGET, RUNTIME_TYPES, renderRuntime, runtimeCrate, withRuntime, withRuntimeIgnore, withRuntimeTypes } from "../src/init/runtime.ts"

test("the crate is named after the app", () => {
  expect(runtimeCrate("my-app")).toBe("my-app-runtime")
  expect(runtimeCrate("@acme/Vision.App")).toBe("vision-app-runtime")
  expect(runtimeCrate("3d-demo")).toBe("app-3d-demo-runtime")
  expect(runtimeCrate("")).toBe("app-runtime")
})

test("the templates get the crate, its lib name and the release", () => {
  let files = renderRuntime(
    {
      "runtime/Cargo.toml": 'name = "{{crate}}"\ntag = "v{{version}}"',
      "runtime/src/bin/solidrt.rs": "{{lib}}::program()",
    },
    "my-app",
    "0.0.73",
  )
  expect(files["runtime/Cargo.toml"]).toBe('name = "my-app-runtime"\ntag = "v0.0.73"')
  expect(files["runtime/src/bin/solidrt.rs"]).toBe("my_app_runtime::program()")
})

test("package.json names the runtime, keeping the rest", () => {
  let out = JSON.parse(withRuntime('{"name": "x", "solidrt": {"appId": "a", "runtime": null}, "scripts": {}}'))
  expect(out.solidrt).toEqual({ appId: "a", runtime: RUNTIME_DIST })
  expect(out.scripts).toEqual({})
  expect(JSON.parse(withRuntime('{"name": "x"}')).solidrt).toEqual({ runtime: RUNTIME_DIST })
})

test("tsconfig includes the module types once", () => {
  let once = withRuntimeTypes('{"include": ["src", "tests"]}')
  expect(JSON.parse(once!).include).toEqual(["src", "tests", RUNTIME_TYPES])
  expect(JSON.parse(withRuntimeTypes(once!)!).include).toEqual(["src", "tests", RUNTIME_TYPES])
  expect(JSON.parse(withRuntimeTypes("{}")!).include).toEqual([RUNTIME_TYPES])
  expect(withRuntimeTypes('{ /* comment */ "include": [] }')).toBe(null)
})

test(".gitignore skips the cargo target once", () => {
  expect(withRuntimeIgnore(null)).toBe(RUNTIME_TARGET + "\n")
  expect(withRuntimeIgnore("node_modules/\ndist/")).toBe("node_modules/\ndist/\n" + RUNTIME_TARGET + "\n")
  expect(withRuntimeIgnore("dist/\n" + RUNTIME_TARGET + "\n")).toBe("dist/\n" + RUNTIME_TARGET + "\n")
})
