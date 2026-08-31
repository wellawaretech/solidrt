// Behavior check for createMixer (src/mixer.ts): the contracts the doc
// comments state, driven with hand-fed dt steps over a fake model whose
// nodes are DETACHED groups (setTransform still writes the node-local
// fields, which is all the assertions read - no Scene needed, but the
// module imports the runtime, so like raycast-check this runs as an app
// from the repo root:
//
//   bunx srt render packages/3d/checks/mixer-check.tsx --project --duration 3 --size 128x128
//
// Covered: linear sampling through update, loop:false clamps and fires
// onFinish exactly once (the pose holds), crossfade blends by weight and
// completes, stop() ends updates, unknown clip names throw listing the
// clips. Prints one PASS/FAIL summary, then exits.
import { exit, render } from "@solidrt/core"
import { createGroup, createMixer } from "@solidrt/3d"
import type { Model } from "@solidrt/3d"

let failures = 0
let fail = (m: string): void => {
  failures++
  console.log("FAIL:", m)
}
let near = (a: number, b: number, eps = 1e-4): boolean => Math.abs(a - b) <= eps

function run(): void {
  let node = createGroup()
  let clipA = {
    name: "A",
    duration: 1,
    channels: [{
      node: 0,
      path: "position" as const,
      interpolation: "linear" as const,
      times: new Float32Array([0, 1]),
      values: new Float32Array([0, 0, 0, 10, 0, 0]),
    }],
  }
  let clipB = {
    name: "B",
    duration: 1,
    channels: [{
      node: 0,
      path: "position" as const,
      interpolation: "step" as const,
      times: new Float32Array([0]),
      values: new Float32Array([20, 0, 0]),
    }],
  }
  let model = { nodes: [{ name: "n", node }], clips: [clipA, clipB], _skins: [], _parents: [null], _worlds: [] } as unknown as Model

  let mixer = createMixer(model)
  let finished: string[] = []
  mixer.onFinish = (name) => finished.push(name)

  mixer.play("A", { loop: false })
  mixer.update(0.5)
  if (!near(node.position[0], 5)) fail(`mid-clip: x ${node.position[0]}, expected 5`)
  mixer.update(1.0)
  if (!near(node.position[0], 10)) fail(`clamped at end: x ${node.position[0]}, expected 10`)
  if (finished.join() !== "A") fail(`onFinish at the end: [${finished.join()}], expected [A]`)
  mixer.update(0.5)
  if (finished.join() !== "A") fail(`onFinish fired again: [${finished.join()}]`)
  if (!near(node.position[0], 10)) fail(`pose held after finish: x ${node.position[0]}`)
  if (mixer.playing().join() !== "A") fail(`playing after finish: [${mixer.playing().join()}]`)

  // Crossfade halfway: A (holding 10) and B (20) at equal weight -> 15.
  mixer.play("B", { fadeMs: 1000 })
  mixer.update(0.5)
  if (!near(node.position[0], 15)) fail(`fade midpoint blend: x ${node.position[0]}, expected 15`)
  if (mixer.playing().join() !== "B") fail(`playing during fade: [${mixer.playing().join()}]`)
  mixer.update(0.6)
  if (!near(node.position[0], 20)) fail(`fade completed: x ${node.position[0]}, expected 20`)

  mixer.stop()
  if (mixer.update(0.1) !== false) fail("update after stop() still returns true")

  let threw = false
  try {
    mixer.play("nope")
  } catch (e) {
    threw = String(e).includes("A, B")
  }
  if (!threw) fail("unknown clip name did not throw listing the clips")
}

function App() {
  run()
  console.log(failures === 0 ? "PASS: mixer behavior" : `mixer-check: ${failures} failure(s)`)
  exit()
  return <window />
}

render(() => <App />)
