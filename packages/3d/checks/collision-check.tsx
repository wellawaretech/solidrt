// Doc-contract check for collision: the behavioral claims AGENTS.md
// (Collision) and scene.ts/collision.ts make about overlap(), sweep()
// and moveAndSlide, asserted against a running scene so the copies
// cannot drift from the runtime. Claims covered:
//   1. A sweep is exact: a sphere cast at a cube stops at the analytic
//      time with the face normal and the touch point on the cube.
//   2. World-space testing: a non-uniformly scaled cube is hit where its
//      scaled face is, not where its unit box would be.
//   3. The slide filter: a volume resting on a surface sweeps along it
//      unhindered and pressing into it reports time 0.
//   4. Layers: a scene-masked-out mesh is invisible to overlap/sweep
//      unless the query passes its own { layers }; { meshes } is an
//      include-list.
//   5. The box tier: an instanced mesh counts by its population box and
//      reports a contact like any surface.
//   6. moveAndSlide lands a capsule a skin above a floor and reports it.
// Runs on the playback client, from the repo root:
//
//   bunx srt render packages/3d/checks/collision-check.tsx --project --duration 3 --size 128x128
//
// Asserts on the second frame and prints one PASS/FAIL summary, then
// exits; read the output, not the exit code.
import { exit, onFrame, pct, render } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import { add, box, createInstancedMesh, createMesh, createScene, moveAndSlide, setLayers, setTransform, shaderMaterialClass, unlit } from "@solidrt/3d"
import type { Vec3 } from "@solidrt/3d"

const SIZE = 128
const EPS = 1e-3

let failures = 0
function fail(msg: string) {
  failures++
  console.log(`FAIL: ${msg}`)
}
let near = (a: Vec3, b: Vec3) => Math.abs(a[0] - b[0]) < EPS && Math.abs(a[1] - b[1]) < EPS && Math.abs(a[2] - b[2]) < EPS

function App() {
  let scene = createScene(SIZE, SIZE, { clearColor: [0.07, 0.07, 0.1, 1], label: "collision-check" })
  scene.setCamera({ fov: 55, position: [0, 0, 10], target: [0, 0, 0] })
  let grey = unlit({ color: [0.5, 0.5, 0.5] })

  // A 2x2x2 cube at the origin: faces at +-1.
  let cube = createMesh(box({ width: 2, height: 2, depth: 2 }), grey)
  add(scene.root, cube)
  // A unit cube scaled to 4 wide along x, off to the side: faces at x = 10 +- 2.
  let wide = createMesh(box(), grey)
  setTransform(wide, { position: [10, 0, 0], scale: [4, 1, 1] })
  add(scene.root, wide)
  // The undrawn collision stand-in: layer 2, outside the scene mask (1).
  let collision = createMesh(box({ width: 2, height: 2, depth: 2 }), grey)
  setTransform(collision, { position: [0, -10, 0] })
  setLayers(collision, 2)
  add(scene.root, collision)
  // Box tier: an instanced mesh with explicit population bounds.
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
    label: "collision-check-instanced",
  })
  let instanced = createInstancedMesh(box(), look.instance(), new Float32Array([0, 0, 0]), 1, {
    bounds: [-1, -1, -1, 1, 1, 1],
  })
  setTransform(instanced, { position: [0, 10, 0] })
  add(scene.root, instanced)

  onFrame((_tick, frame) => {
    if (frame !== 2) return

    // 1. Exact sweep: the sphere's center stops at x = -1.5, t = 0.35.
    let hits = scene.sweep({ center: [-5, 0, 0], radius: 0.5 }, [10, 0, 0], { meshes: [cube] })
    if (hits.length !== 1) fail(`a sphere cast at the cube should hit it once, got ${hits.length}`)
    else {
      let h = hits[0]!
      if (Math.abs(h.time - 0.35) > EPS) fail(`sweep time should be 0.35, got ${h.time}`)
      if (!near(h.normal, [-1, 0, 0])) fail(`sweep normal should face the sphere, got [${h.normal}]`)
      if (!near(h.point, [-1, 0, 0])) fail(`sweep point should sit on the cube's face, got [${h.point}]`)
    }
    if (scene.sweep({ center: [-5, 0, 0], radius: 0.5 }, [3, 0, 0], { meshes: [cube] }).length !== 0)
      fail("a cast that falls short must miss")

    // 2. World-space testing under scale: the wide cube's face is at x = 8.
    let scaled = scene.sweep({ center: [3, 0, 0], radius: 0.5 }, [10, 0, 0], { meshes: [wide] })
    if (scaled.length !== 1 || Math.abs(scaled[0]!.time - 0.45) > EPS)
      fail(`the scaled cube should be hit at t = 0.45 (its scaled face), got ${scaled[0]?.time}`)

    // 3. The slide filter on a sphere resting on the cube's top (y = 1),
    //    moving half a unit so it stays clear of the top's edge (reaching
    //    the edge is a legitimate grazing touch).
    let resting = { center: [0, 1.5, 0] as Vec3, radius: 0.5 }
    if (scene.sweep(resting, [0.5, 0, 0], { meshes: [cube] }).length !== 0) fail("sliding along a contact must report nothing")
    let press = scene.sweep(resting, [0.5, -0.5, 0], { meshes: [cube] })
    if (press.length !== 1 || press[0]!.time > EPS || !near(press[0]!.normal, [0, 1, 0]))
      fail(`pressing into a contact must report time 0 with the surface normal, got ${JSON.stringify(press)}`)
    if (scene.sweep(resting, [0, 1, 0], { meshes: [cube] }).length !== 0) fail("leaving a contact must report nothing")

    // 4. Layers and the include-list.
    let probe = { center: [0, -10, 0] as Vec3, radius: 1.5 }
    if (scene.overlap(probe).length !== 0) fail("a scene-masked-out mesh must be skipped by overlap by default")
    let seen = scene.overlap(probe, { layers: 2 })
    if (seen.length !== 1 || seen[0]!.mesh !== collision) fail("{ layers: 2 } must see the undrawn collision mesh in overlap")
    if (scene.sweep({ center: [-5, -10, 0], radius: 0.5 }, [10, 0, 0]).length !== 0)
      fail("a scene-masked-out mesh must be skipped by sweep by default")
    if (scene.sweep({ center: [-5, -10, 0], radius: 0.5 }, [10, 0, 0], { layers: 2 }).length !== 1)
      fail("{ layers: 2 } must see the undrawn collision mesh in sweep")
    if (scene.overlap({ center: [0, 0, 0], radius: 3 }, { meshes: [wide] }).length !== 0)
      fail("{ meshes } must exclude contacts from unlisted meshes")

    // 5. The instanced box tier: a sphere 0.3 into its face at y = 9.
    let inst = scene.overlap({ center: [0, 8.7, 0], radius: 0.5 })
    if (inst.length !== 1 || inst[0]!.mesh !== instanced) fail("a sphere into the population box should contact the instanced mesh")
    else {
      let c = inst[0]!
      if (Math.abs(c.depth - 0.2) > EPS || !near(c.normal, [0, -1, 0]) || !near(c.point, [0, 9, 0]))
        fail(`the box-tier contact should be depth 0.2 out of the face, got ${JSON.stringify(c)}`)
    }

    // 6. moveAndSlide lands a capsule on the cube's top a skin short.
    let body = { a: [0, 3, 0] as Vec3, b: [0, 4, 0] as Vec3, radius: 0.5 }
    let move = moveAndSlide(scene, body, [0, -3, 0], { meshes: [cube] })
    if (move.floor === null || !near(move.floor, [0, 1, 0])) fail("a capsule dropped on the cube should report its top as floor")
    if (Math.abs(move.motion[1] - -(1.5 - 0.01)) > EPS) fail(`the landing should stop a skin above the top, got ${move.motion[1]}`)
    if (move.wall || move.ceiling) fail("a landing is neither a wall nor a ceiling")

    if (failures === 0) console.log("PASS: exact sweep, world-space scale, slide filter, layers and meshes, box tier, moveAndSlide landing")
    else console.log(`${failures} FAILURES`)
    exit()
  })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <texture src={scene.texture} width={SIZE} height={SIZE} />
      </view>
    </window>
  )
}

render(() => <App />)
