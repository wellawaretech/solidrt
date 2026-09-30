// The skinned vertex stage's palette contract, and - by importing glsl.ts
// on the bare flux binary at all - the module's purity contract: glsl.ts
// must keep working with no flux:gpu in its graph (bake tools and headless
// rigs run it without a gui runtime, and this binary has none: a gui
// import would fail to link here).
import { expect, test } from "flux:test"
import { litVertex, unlitVertex } from "../src/glsl.ts"

test("skinned palette: samples uBones as a float texture, not a uniform array", () => {
  for (let src of [litVertex({ skinned: true }), unlitVertex({ skinned: true })]) {
    expect(src).toContain("uniform sampler2D uBones;")
    expect(src).toContain("texelFetch(uBones")
    expect(src).toContain("in uvec4 aJoints;")
    expect(src).toContain("boneAt(aJoints.x)")
    expect(src).not.toContain("uniform mat4 uBones[")
  }
})

test("skinned palette: unskinned stages carry no palette", () => {
  expect(litVertex()).not.toContain("uBones")
  expect(unlitVertex()).not.toContain("uBones")
})
