// The rules `sol tool` reads a source and a tool file by (src/tool/pick.ts):
// an installed tool by its `<pkg>/<name>`, a project's script by its path,
// told apart by the `.ts` suffix a name never has; the host each runs
// under is the file's suffix.
import { test, expect } from "flux:test"
import { hostOf, isScriptPath, isToolFile, nameOf } from "../src/tool/pick.ts"

test("a source ending in .ts is a script path, an installed tool's name is not", () => {
  expect(isScriptPath("./scripts/bake.flux.ts")).toBe(true)
  expect(isScriptPath("scripts/bake.ts")).toBe(true)
  expect(isScriptPath("/abs/scripts/bake.ts")).toBe(true)
  expect(isScriptPath("3d/model")).toBe(false)
  expect(isScriptPath("3d/model.flux")).toBe(false)
  expect(isScriptPath("components/qrcode.tsx")).toBe(false)
})

test("a tool file is a .ts that is not a declaration, named without its suffix", () => {
  expect(isToolFile("model.flux.ts")).toBe(true)
  expect(isToolFile("environment.ts")).toBe(true)
  expect(isToolFile("node-env.d.ts")).toBe(false)
  expect(isToolFile("README.md")).toBe(false)
  expect(nameOf("model.flux.ts")).toBe("model")
  expect(nameOf("environment.ts")).toBe("environment")
})

test("the suffix picks the host, for an installed tool and a project's script alike", () => {
  expect(hostOf("model.flux.ts")).toBe("flux")
  expect(hostOf("environment.ts")).toBe("bun")
  expect(hostOf("./scripts/bake-character.flux.ts")).toBe("flux")
  expect(hostOf("scripts/fetch-assets.ts")).toBe("bun")
})
