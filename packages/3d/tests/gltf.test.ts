// The glTF parser and .srtm container under bun test: the check rig in
// checks/gltf-check.ts builds a glb in memory (hierarchy, a mirrored node,
// a skinned part with joint boxes, clips, materials) and throws on the
// first failed expectation, so importing it IS the test. Kept as a check
// so it also runs headless on flux (see its header); wrapped here so a
// parser regression fails `bun test` rather than only the next bake.
import { expect, test } from "bun:test"

test("gltf check rig passes", async () => {
  await expect(import("../checks/gltf-check.ts")).resolves.toBeDefined()
})
