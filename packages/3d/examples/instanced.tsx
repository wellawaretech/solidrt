// Instanced meshes: one draw entry covering a whole population. A material
// class declares `instanceAttributes`, createInstancedMesh (here via the
// <InstancedMesh> component) supplies one interleaved record per instance,
// and the vertex stage reads each record through the matching `in`
// variables. Two meshes share the one class: 400 scattered "rocks" and a
// breathing ring of "pines" - each is ONE entry and ONE uModel however
// many instances it draws, so the spinning group below moves both fleets
// with two matrix writes per frame.
//
// setInstanceCount is the population dial (the pines breathe); records are
// data, not matrices - position/scale/tint here, whatever your shader
// wants in general. The explicit `bounds` cover the scatter so picking
// still works (one conservative box around the population; omit bounds and
// the mesh simply never picks).
//
// The fleets CAST: the class declares `shadowVertex` - its vertex stage
// reduced to the position math, instance placement included - and with it
// `castShadow` on an InstancedMesh works like on any mesh (without it the
// shadow views skip instanced meshes). The lit ground receives; both
// populations throw shadows from the one casting sun, and the breathing
// pines' shadows appear and vanish with them.
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import {
  box,
  cone,
  DirectionalLight,
  Group,
  HemisphereLight,
  InstancedMesh,
  lit,
  Mesh,
  PerspectiveCamera,
  plane,
  Scene,
  setInstanceCount,
  shaderMaterialClass,
} from "@solidrt/3d"
import type { InstancedMeshNode } from "@solidrt/3d"
import { HEMISPHERE } from "@solidrt/3d/glsl"

const SIZE = 720

const INSTANCE_VERTEX = glsl`
  in vec3 aPos;
  in vec3 aNormal;
  in vec3 iPos;
  in float iScale;
  in vec3 iTint;
  out vec3 vNormal;
  out vec3 vTint;
  uniform mat4 uModel;
  uniform mat4 uViewProj;

  void main() {
    vec3 p = aPos * iScale + iPos;
    gl_Position = uViewProj * uModel * vec4(p, 1.0);
    vNormal = mat3(uModel) * aNormal;
    vTint = iTint;
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

const INSTANCE_FRAGMENT = glsl`
  in vec3 vNormal;
  in vec3 vTint;
  ${HEMISPHERE}

  void main() {
    vec3 c = vTint * hemisphere(normalize(vNormal), vec3(1.05, 1.0, 0.95), vec3(0.35, 0.32, 0.3));
    fragColor = vec4(c, 1.0);
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
  let pinesMesh!: InstancedMeshNode
  onFrame(tick => {
    setSpin(tick / 6000)
    // The population dial: draw the first N records. The buffer holds the
    // full ring; only the draw range moves, one setDrawRange per change.
    let n = Math.round(PINE_COUNT * (0.5 + 0.5 * Math.sin(tick / 900)))
    if (pinesMesh) setInstanceCount(pinesMesh, n)
  })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} designSize={[SIZE, SIZE]}>
        <Scene width={SIZE} height={SIZE} clearColor={[0.07, 0.08, 0.1, 1]} label="instanced">
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
            <InstancedMesh
              geometry={box({ label: "rock" })}
              material={instancedLook.instance()}
              records={rocks(400)}
              bounds={[-3.9, 0, -3.9, 3.9, 0.2, 3.9]}
              castShadow
            />
            <InstancedMesh
              geometry={cone({ radius: 0.3, height: 1, radialSegments: 10, label: "pine" })}
              material={instancedLook.instance()}
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
// per-mesh uniforms stay independent (none are used here).
let instancedLook = shaderMaterialClass({
  vertex: INSTANCE_VERTEX,
  shadowVertex: INSTANCE_SHADOW_VERTEX,
  fragment: INSTANCE_FRAGMENT,
  instanceAttributes: [
    { name: "iPos", format: "vec3" },
    { name: "iScale", format: "f32" },
    { name: "iTint", format: "vec3" },
  ],
  label: "instanced-look",
})

render(() => <App />)
