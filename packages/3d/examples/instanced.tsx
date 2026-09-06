// Instanced meshes: one draw entry covering a whole population. A material
// class declares `instanceAttributes`, createRecordMesh (here via the
// <RecordMesh> component) supplies one interleaved record per instance,
// and the vertex stage reads each record through the matching `in`
// variables. Two meshes share the one class: 400 scattered "rocks" and a
// breathing ring of "pines" - each is ONE entry and ONE uModel however
// many instances it draws, so the spinning group below moves both fleets
// with two matrix writes per frame.
//
// setRecordCount is the population dial (the pines breathe); records are
// data, not matrices - position/scale/tint here, whatever your shader
// wants in general. The explicit `bounds` cover the scatter so picking
// still works (one conservative box around the population; omit bounds and
// the mesh simply never picks).
//
// The fleets CAST: the class declares `shadowVertex` - its vertex stage
// reduced to the position math, instance placement included - and with it
// `castShadow` on an RecordMesh works like on any mesh (without it the
// shadow views skip instanced meshes). The lit ground receives; both
// populations throw shadows from the one casting sun, and the breathing
// pines' shadows appear and vanish with them.
//
// The fleets are also lit, shadowed and fogged exactly like the ground,
// with no lighting code of their own: the class pairs its instanced
// vertex stage with the stock `litFragment` (the fragment `lit` compiles),
// which is the first tier of custom looks - a vertex stage that writes
// the lit varyings gets the whole scene for free. The per-instance tint
// rides the `vertexColors` path as vColor.
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import {
  box,
  cone,
  DirectionalLight,
  Group,
  HemisphereLight,
  RecordMesh,
  lit,
  Mesh,
  PerspectiveCamera,
  plane,
  Scene,
  setRecordCount,
  shaderMaterialClass,
} from "@solidrt/3d"
import type { RecordMeshNode } from "@solidrt/3d"
import { litFragment } from "@solidrt/3d/glsl"

// The lit varyings (vWorldPos, vNormal, vUv, and vColor for the tint), as
// LIT_VERTEX writes them, from an instanced placement.
const INSTANCE_VERTEX = glsl`
  in vec3 aPos;
  in vec3 aNormal;
  in vec2 aUV;
  in vec3 iPos;
  in float iScale;
  in vec3 iTint;
  out vec3 vWorldPos;
  out vec3 vNormal;
  out vec2 vUv;
  out vec4 vColor;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform mat4 uNormal;

  void main() {
    vec4 world = uModel * vec4(aPos * iScale + iPos, 1.0);
    gl_Position = uViewProj * world;
    vWorldPos = world.xyz;
    vNormal = mat3(uNormal) * aNormal;
    vUv = aUV;
    vColor = vec4(iTint, 1.0);
  }
`

// The shadow pass's view of the same placement: aPos * iScale + iPos and
// nothing else - no normal, no tint. The depth fragment is the engine's.
const INSTANCE_SHADOW_VERTEX = glsl`
  in vec3 aPos;
  in vec3 iPos;
  in float iScale;
  uniform mat4 uModel;
  uniform mat4 uViewProj;

  void main() {
    gl_Position = uViewProj * uModel * vec4(aPos * iScale + iPos, 1.0);
  }
`

// One record per instance, interleaved in attribute order: 7 floats.
const STRIDE = 7

// A deterministic scatter (no per-run surprises when eyeballing).
function rocks(count: number): Float32Array {
  let records = new Float32Array(count * STRIDE)
  let a = 0
  for (let i = 0; i < count; i++) {
    a += 2.399963 // golden angle: an even spiral scatter
    let r = 0.35 + 3.4 * Math.sqrt((i + 0.5) / count)
    let s = 0.05 + 0.11 * ((i * 7) % 10) / 10
    let o = i * STRIDE
    records[o] = Math.cos(a) * r
    records[o + 1] = s / 2
    records[o + 2] = Math.sin(a) * r
    records[o + 3] = s
    records[o + 4] = 0.55 + 0.3 * ((i * 3) % 5) / 5
    records[o + 5] = 0.5 + 0.2 * ((i * 11) % 7) / 7
    records[o + 6] = 0.45
  }
  return records
}

function pines(count: number): Float32Array {
  let records = new Float32Array(count * STRIDE)
  for (let i = 0; i < count; i++) {
    let a = (i / count) * Math.PI * 2
    let s = 0.5 + 0.25 * ((i * 5) % 8) / 8
    let o = i * STRIDE
    records[o] = Math.cos(a) * 2.4
    records[o + 1] = s / 2
    records[o + 2] = Math.sin(a) * 2.4
    records[o + 3] = s
    records[o + 4] = 0.15
    records[o + 5] = 0.4 + 0.25 * ((i * 3) % 6) / 6
    records[o + 6] = 0.2
  }
  return records
}

const PINE_COUNT = 48

function App() {
  let [spin, setSpin] = createSignal(0)
  let pinesMesh!: RecordMeshNode
  onFrame(tick => {
    setSpin(tick / 6000)
    // The population dial: draw the first N records. The buffer holds the
    // full ring; only the draw range moves, one setDrawRange per change.
    let n = Math.round(PINE_COUNT * (0.5 + 0.5 * Math.sin(tick / 900)))
    if (pinesMesh) setRecordCount(pinesMesh, n)
  })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.07, 0.08, 0.1, 1]} fog={{ color: [0.07, 0.08, 0.1], near: 5, far: 13 }} label="instanced">
          <PerspectiveCamera fov={55} position={[0, 3.2, 5.4]} lookAt={[0, 0.2, 0]} />
          <HemisphereLight sky={[0.4, 0.42, 0.45]} ground={[0.12, 0.13, 0.11]} />
          <DirectionalLight
            color={[1, 0.95, 0.85]}
            intensity={0.7}
            position={[4, 3.5, 3]}
            direction={[-4, -3.5, -3]}
            castShadow
            shadow={{ normalBias: 0.02, camera: { near: 1, far: 20 } }}
          />
          <Mesh geometry={plane({ width: 9, height: 9, label: "meadow" })} material={lit({ color: [0.24, 0.27, 0.24] })} rotation={[-Math.PI / 2, 0, 0]} />
          <Group rotation={[0, spin(), 0]}>
            <RecordMesh
              geometry={box({ label: "rock" })}
              material={instancedLook.instance({ params: LOOK })}
              records={rocks(400)}
              bounds={[-3.9, 0, -3.9, 3.9, 0.2, 3.9]}
              castShadow
            />
            <RecordMesh
              geometry={cone({ radius: 0.3, height: 1, radialSegments: 10, label: "pine" })}
              material={instancedLook.instance({ params: LOOK })}
              records={pines(PINE_COUNT)}
              bounds={[-2.8, 0, -2.8, 2.8, 0.8, 2.8]}
              castShadow
              ref={m => (pinesMesh = m)}
            />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

// One class, one compiled pipeline; each mesh gets its own instance() so
// per-mesh uniforms stay independent. The fragment is the stock lit one,
// so the instance carries lit's per-entry uniforms: a white base (the
// tint arrives per instance through vColor) and a modest highlight.
const LOOK = { uColor: [1, 1, 1, 1], uSpecular: 0.25, uShininess: 30 }
let instancedLook = shaderMaterialClass({
  vertex: INSTANCE_VERTEX,
  shadowVertex: INSTANCE_SHADOW_VERTEX,
  fragment: litFragment({ vertexColors: true }),
  instanceAttributes: [
    { name: "iPos", format: "vec3" },
    { name: "iScale", format: "f32" },
    { name: "iTint", format: "vec3" },
  ],
  label: "instanced-look",
})

render(() => <App />)
