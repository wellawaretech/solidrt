// Doc-contract check for picking: the behavioral claims the docs make in
// more than one place (AGENTS.md Picking + layers, scene.ts pick()/Hit/
// setLayers docs), asserted against the running scene so the copies
// cannot drift from the runtime again - a Traps entry once claimed
// box-only picking long after the narrowphase went triangle-accurate.
// Claims covered:
//   1. An ordinary mesh picks per triangle: hits carry face/uv/normal,
//      and a ray through a gap INSIDE the merged geometry's box misses.
//   2. An instanced mesh is box-only: its hits carry the box face's
//      normal and neither face nor uv.
//   3. pick(x, y) and raycast cast the same ray: same nearest mesh.
//   4. A scene-masked-out mesh is skipped by raycast like an invisible
//      one - unless the query passes its own { layers }, which sees it
//      (the undrawn collision-mesh pattern).
//   5. { meshes } is an include-list: hits only from the listed meshes.
// The scene is GPU state, so like core's gpu-lease-check this runs on the
// playback client, from the repo root:
//
//   bunx srt render packages/3d/checks/raycast-check.tsx --project --duration 3 --size 128x128
//
// Asserts on the second frame and prints one PASS/FAIL summary, then
// exits; read the output, not the exit code.
import { exit, onFrame, pct, render } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import {
  add,
  box,
  createRecordMesh,
  createMesh,
  createScene,
  mergeGeometries,
  setLayers,
  setTransform,
  shaderMaterialClass,
  transformGeometry,
  unlit,
} from "@solidrt/3d"

const SIZE = 128

let failures = 0
function fail(msg: string) {
  failures++
  console.log(`FAIL: ${msg}`)
}

function App() {
  let scene = createScene(SIZE, SIZE, { clearColor: [0.07, 0.07, 0.1, 1], label: "raycast-check" })
  scene.setCamera({ fov: 55, position: [0, 0, 10], target: [0, 0, 0] })
  let grey = unlit({ color: [0.5, 0.5, 0.5] })

  // Two unit cubes merged into ONE geometry with a 2-wide gap between
  // them: the merged local box spans the gap, so a ray through the middle
  // separates triangle testing from box testing.
  let pair = mergeGeometries([
    transformGeometry(box(), { position: [-2, 0, 0] }),
    transformGeometry(box(), { position: [2, 0, 0] }),
  ])
  let merged = createMesh(pair, grey)
  add(scene.root, merged)

  // Box-only tier: an instanced mesh with explicit population bounds
  // (records are opaque to picking, so the box is the whole story).
  let look = shaderMaterialClass({
    vertex: glsl`
      in vec3 aPos;
      in vec3 iPos;
      uniform mat4 uModel;
      uniform mat4 uViewProj;

      void main() {
        gl_Position = uViewProj * uModel * vec4(aPos + iPos, 1.0);
      }
    `,
    fragment: glsl`
      void main() {
        fragColor = vec4(0.8, 0.4, 0.2, 1.0);
      }
    `,
    instanceAttributes: [{ name: "iPos", format: "vec3" }],
    label: "raycast-check-instanced",
  })
  let instanced = createRecordMesh(box(), look.instance(), new Float32Array([0, 0, 0]), 1, {
    bounds: [-0.5, -0.5, -0.5, 0.5, 0.5, 0.5],
  })
  setTransform(instanced, { position: [0, 3, 0] })
  add(scene.root, instanced)

  // The undrawn collision stand-in: layer 2, outside the scene mask (1).
  let collision = createMesh(box(), grey)
  setTransform(collision, { position: [0, -3, 0] })
  setLayers(collision, 2)
  add(scene.root, collision)

  onFrame((_tick, frame) => {
    if (frame !== 2) return
    let down: [number, number, number] = [0, 0, -1]

    // 1. Triangle accuracy on the merged mesh.
    let solid = scene.raycast([-2, 0, 10], down)
    if (solid.length !== 1 || solid[0]!.mesh !== merged) fail("ray into the merged cube should hit it once")
    else {
      let h = solid[0]!
      if (h.face === undefined || h.uv === undefined || h.normal === undefined)
        fail("an ordinary mesh hit must carry face, uv and normal")
      if (h.normal && Math.abs(h.normal[2] - 1) > 1e-4) fail(`hit normal should face the ray, got [${h.normal}]`)
      if (Math.abs(h.distance - 9.5) > 1e-3) fail(`hit distance should be 9.5, got ${h.distance}`)
    }
    if (scene.raycast([0, 0, 10], down).length !== 0)
      fail("a ray through the gap inside the merged box must miss (triangle test, not box test)")

    // 2. Instanced hits are box-only: the face's normal, no face/uv.
    let inst = scene.raycast([0, 3, 10], down)
    if (inst.length !== 1 || inst[0]!.mesh !== instanced) fail("ray into the instanced box should hit it once")
    else if (inst[0]!.face !== undefined || inst[0]!.uv !== undefined) fail("an instanced hit must carry no face or uv")
    else if (Math.abs(inst[0]!.normal[2] - 1) > 1e-4) fail(`an instanced hit carries the struck face's normal, got [${inst[0]!.normal}]`)

    // 3. pick() casts the same ray as raycast(screenRay).
    let px = scene.project([-2, 0, 0])
    if (px === null) fail("project() lost a point in front of the camera")
    else {
      let picked = scene.pick(px.x, px.y)[0]
      let { origin, direction } = scene.screenRay(px.x, px.y)
      let rayed = scene.raycast(origin, direction)[0]
      if (picked?.mesh !== merged || rayed?.mesh !== merged) fail("pick and raycast disagree on the same pixel")
    }

    // 4. Layer masks: invisible to the scene mask, visible to an override.
    if (scene.raycast([0, -3, 10], down).length !== 0) fail("a scene-masked-out mesh must be skipped by default")
    let coll = scene.raycast([0, -3, 10], down, { layers: 2 })
    if (coll.length !== 1 || coll[0]!.mesh !== collision) fail("{ layers: 2 } must see the undrawn collision mesh")
    if (coll[0] !== undefined && coll[0].face === undefined) fail("the collision mesh hit should be triangle-accurate")

    // 5. { meshes } include-list.
    let included = scene.raycast([0, 3, 10], down, { meshes: [merged] })
    if (included.length !== 0) fail("{ meshes } must exclude hits from unlisted meshes")

    if (failures === 0) console.log("PASS: triangle accuracy, box tier, pick/raycast parity, layer masks, mesh filter")
    else console.log(`${failures} FAILURES`)
    exit()
  })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} designSize={[SIZE, SIZE]}>
        <texture src={scene.texture} width={SIZE} height={SIZE} />
      </view>
    </window>
  )
}

render(() => <App />)
