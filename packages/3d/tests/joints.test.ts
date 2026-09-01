// The skinned vertex stage's palette contract, and - by importing glsl.ts
// under bun at all - the module's purity contract: glsl.ts must keep
// working with no flux:gpu in its graph (bake tools and headless rigs run
// it without a runtime).
import { describe, expect, test } from "bun:test"
import { litVertex, unlitVertex } from "../src/glsl.ts"

describe("skinned palette", () => {
  test("samples uBones as a float texture, not a uniform array", () => {
    for (let src of [litVertex({ skinned: true }), unlitVertex({ skinned: true })]) {
      expect(src).toContain("uniform sampler2D uBones;")
      expect(src).toContain("texelFetch(uBones")
      expect(src).toContain("boneAt(int(aJoints.x))")
      expect(src).not.toContain("uniform mat4 uBones[")
    }
  })

  test("unskinned stages carry no palette", () => {
    expect(litVertex()).not.toContain("uBones")
    expect(unlitVertex()).not.toContain("uBones")
  })
})
