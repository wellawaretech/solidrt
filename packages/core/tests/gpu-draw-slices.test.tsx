// The submission budget on a live GPU (alloy gl::slicing): a draw above
// the work floor is issued in slices on its first render (the probe) and
// as one draw call once its rate is measured, with the pixels of the
// unsliced draw both times. An instanced draw slices over its instances
// (the instance attribute pointers re-pointed per slice) unless its vertex
// stage reads gl_InstanceID, which slices over the vertex range instead.
// Read off the inventory's `slices` and `submissionMs`, the target's
// passes, and its pixels: one instance per pixel, each tinted by its own
// grid position, so a slice issued at the wrong base shows as the wrong
// color at a known pixel.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { addDraw, compileShader, createBuffer, createDrawTarget, createRenderPipeline, glsl, linkProgram, readTexture, setDrawParams } from "../src/gpu.ts"
import type { DrawId, TextureId } from "../src/gpu.ts"

// A 256 x 128 grid of one-pixel quads (6 vertices each): 196608 vertices
// of work, three times the engine's slicing floor (65536), so the first
// render probes in at least two slices; its first slice is 65536 / 6 =
// 10922 instances, which is row 42, column 170 of the grid.
const WIDTH = 256
const HEIGHT = 128
const INSTANCES = WIDTH * HEIGHT
const QUAD = 6
const FIRST_SLICE_END = Math.floor(65536 / QUAD)
// A population under the floor: never sliced, never measured.
const SMALL = 64
// The engine's submission budget (alloy gl::slicing), and the margin a
// probe slice must sit under it by for the whole draw (three times the
// probe's work) to be predicted under the merge line.
const BUDGET_MS = 500
const CONVERGE_MARGIN = 20

// Each instance's tint is its grid position, through the instance record
// (iPixel) or through gl_InstanceID; the two must draw the same picture.
let VERTEX_RECORD = glsl`
  in vec2 aPos;
  in vec2 iPixel;
  uniform float uBrightness;
  out vec3 vTint;
  void main() {
    vec2 center = (iPixel + 0.5) / vec2(${WIDTH}.0, ${HEIGHT}.0) * 2.0 - 1.0;
    gl_Position = vec4(center + aPos / vec2(${WIDTH}.0, ${HEIGHT}.0), 0.0, 1.0);
    vTint = vec3(iPixel.x / 255.0, iPixel.y / 255.0, 0.0) * uBrightness;
  }
`
let VERTEX_INSTANCE_ID = glsl`
  in vec2 aPos;
  in vec2 iPixel;
  uniform float uBrightness;
  out vec3 vTint;
  void main() {
    vec2 center = (iPixel + 0.5) / vec2(${WIDTH}.0, ${HEIGHT}.0) * 2.0 - 1.0;
    gl_Position = vec4(center + aPos / vec2(${WIDTH}.0, ${HEIGHT}.0), 0.0, 1.0);
    vTint = vec3(float(gl_InstanceID & 255) / 255.0, float(gl_InstanceID >> 8) / 255.0, 0.0) * uBrightness;
  }
`
let FRAGMENT = glsl`
  in vec3 vTint;
  void main() {
    fragColor = vec4(vTint, 1.0);
  }
`

type Draw = { id: number; slices?: number; submissionMs?: number }
type Pipeline = { kind: string; label?: string; passes: number; draws: Draw[] }

function pipeline(app: TestApp, label: string): Pipeline {
  let pipelines = app.gpu({ label }).pipelines as Pipeline[]
  let found = pipelines.find(p => p.kind === "draws" && p.label === label)
  if (found === undefined) throw new Error("draw target not in the inventory: " + label)
  return found
}

function pixel(data: Uint8Array, x: number, y: number): [number, number, number] {
  let at = (y * WIDTH + x) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!]
}

function rig(vertex: string, instances: number, label: string): { target: TextureId; draw: DrawId } {
  let quad = createBuffer(new Float32Array([-1, -1, 1, -1, 1, 1, -1, -1, 1, 1, -1, 1]), { label: label + "-quad" })
  let records = new Float32Array(instances * 2)
  for (let i = 0; i < instances; i++) {
    records[i * 2] = i % WIDTH
    records[i * 2 + 1] = Math.floor(i / WIDTH)
  }
  let pixels = createBuffer(records, { label: label + "-pixels" })
  let program = linkProgram(compileShader("vertex", vertex, { header: true }), compileShader("fragment", FRAGMENT, { header: true }))
  let pipe = createRenderPipeline(program, {
    topology: "triangles",
    buffers: [
      { attributes: [{ name: "aPos", format: "float32x2" }] },
      { stepMode: "instance", attributes: [{ name: "iPixel", format: "float32x2" }] },
    ],
  })
  let target = createDrawTarget(WIDTH, HEIGHT, null, { clearColor: [0, 0, 0, 1], label })
  let draw = addDraw(target, pipe, { uBrightness: 1 }, { buffers: [quad, pixels], vertexCount: QUAD, label: label + "-grid" })
  return { target, draw }
}

async function mounted(app: TestApp, vertex: string, instances: number, label: string) {
  let r!: ReturnType<typeof rig>
  await app.mount(() => {
    r = rig(vertex, instances, label)
    return <texture src={r.target} width={WIDTH} height={HEIGHT} />
  })
  return r
}

// Every pixel is its own grid position; the probe's slice boundary and
// the corners are where a wrong base offset or a lost tail would show.
function expectGrid(data: Uint8Array, what: string) {
  let samples: [number, number][] = [
    [0, 0],
    [WIDTH - 1, 0],
    [0, HEIGHT - 1],
    [WIDTH - 1, HEIGHT - 1],
    [100, 50],
    [(FIRST_SLICE_END - 1) % WIDTH, Math.floor((FIRST_SLICE_END - 1) / WIDTH)],
    [FIRST_SLICE_END % WIDTH, Math.floor(FIRST_SLICE_END / WIDTH)],
    [(FIRST_SLICE_END + 1) % WIDTH, Math.floor((FIRST_SLICE_END + 1) / WIDTH)],
  ]
  for (let [x, y] of samples) {
    expect({ what, x, y, rgb: pixel(data, x, y) }).toEqual({ what, x, y, rgb: [x, y, 0] })
  }
}

test("a draw above the floor probes in slices, then converges to one draw call with the same pixels", async app => {
  let { target, draw } = await mounted(app, VERTEX_RECORD, INSTANCES, "slices")
  // The readback drains the render ahead of the inventory read.
  let sliced = readTexture(target).data
  expectGrid(sliced, "sliced")
  let first = pipeline(app, "slices")
  let entry = first.draws.find(d => d.id === draw)!
  expect(entry.slices).toBeGreaterThanOrEqual(2)
  expect(entry.submissionMs).toBeDefined()
  expect(entry.submissionMs!).toBeLessThanOrEqual(BUDGET_MS)
  // A params write re-renders the same picture. The second render plans
  // from the probe's measured rate: one draw call where the probe's
  // slowest slice was well under the budget (any real GPU; a software
  // renderer on a loaded machine may measure slow and stay sliced, which
  // is the budget at work), and every submission under the budget either way.
  setDrawParams(target, draw, { uBrightness: 1 })
  await app.frame()
  let converged = readTexture(target).data
  let second = pipeline(app, "slices")
  expect(second.passes).toBeGreaterThan(first.passes)
  let again = second.draws.find(d => d.id === draw)!
  if (entry.submissionMs! * CONVERGE_MARGIN <= BUDGET_MS) expect(again.slices).toBeUndefined()
  if (again.slices !== undefined) expect(again.submissionMs!).toBeLessThanOrEqual(BUDGET_MS)
  expect(converged).toEqual(sliced)
})

test("a vertex stage that reads gl_InstanceID slices over its vertices and stays exact", async app => {
  let { target, draw } = await mounted(app, VERTEX_INSTANCE_ID, INSTANCES, "slices-id")
  expectGrid(readTexture(target).data, "instance id")
  let entry = pipeline(app, "slices-id").draws.find(d => d.id === draw)!
  // Six vertices, one triangle per slice: two slices, every instance in each.
  expect(entry.slices).toBe(2)
})

test("a draw under the floor is neither sliced nor measured", async app => {
  let { target, draw } = await mounted(app, VERTEX_RECORD, SMALL, "slices-small")
  let data = readTexture(target).data
  let entry = pipeline(app, "slices-small").draws.find(d => d.id === draw)!
  expect(entry.slices).toBeUndefined()
  expect(entry.submissionMs).toBeUndefined()
  expect(pixel(data, 0, 0)).toEqual([0, 0, 0])
  expect(pixel(data, SMALL - 1, 0)).toEqual([SMALL - 1, 0, 0])
  expect(pixel(data, SMALL, 0)).toEqual([0, 0, 0])
})
