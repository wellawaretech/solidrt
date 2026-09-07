// Mesh pointer events under an orbit control, the scene as the root of
// the walk: hover tints (enter/leave), a TAP pops (a down may be the start
// of an orbit drag, so the pop waits for the release), a drag on the
// crate slides it over the floor and never orbits - its down claims the
// press, so the orbit control's feed, listening at the root, sees none
// of it - while a drag anywhere else orbits and a wheel zooms; the ring
// claims the wheel to spin itself; a tap on empty space un-pops
// everything (the scene's own onTap with `mesh` null, the deselect
// idiom). The Group hears the cone's downs through bubbling (the crate's
// claim stops the walk at the crate). The scene is STATIC - no onFrame - so it renders only when an event changes
// something, and the orbit runs frames only while it glides; hit testing
// runs over the scene's BVH, so a pointer move costs O(log meshes).
//
// The volume tier is honest about its shape: hits test each mesh's
// bounding box, so hovering just outside the ball's silhouette (inside its
// box corners) still counts. Triangle-accurate hits are a later tier.
//
// Debug commands: `state` (the orbit pose, the crate's position, the
// ring's spin, the popped and hovered names) and `pixel` ({ name }: the
// WINDOW pixel a mesh's position projects to, for a synthetic tap).

import { createInputMap, createPointerFeed, createSignal, displayScale, pct, render } from "@solidrt/core"
import { box, cone, getRotation, Group, Mesh, OrbitCamera, orbitActions, orbitBindings, PerspectiveCamera, plane, Scene, setMeshParams, setTransform, sphere, torus, unlit, useScene } from "@solidrt/3d"
import type { Geometry, MeshNode, NodePointerEvent, NodeTapEvent, NodeWheelEvent, OrbitCameraHandle, SceneHandle, Vec3 } from "@solidrt/3d"
import { registerDebug } from "srt:dev"

type Color = [number, number, number]

// How much a popped mesh grows.
const POP_SCALE = 1.25
// Radians of ring spin per wheel pixel.
const WHEEL_SPIN = 0.01
// The orbit's starting pose: the old fixed camera at [0, 2.6, 5] looking
// at the scene's middle.
const ORBIT_TARGET: Vec3 = [0, 0.4, 0]
const ORBIT_DISTANCE = 5.5
const ORBIT_ELEVATION = 0.42

let [hovered, setHovered] = createSignal("nothing")
// Popped meshes and the scale they pop back to.
let popped = new Map<MeshNode, number>()
let meshes: Record<string, MeshNode> = {}
let orbit!: OrbitCameraHandle
let scene!: SceneHandle

let unpopAll = () => {
  for (let [mesh, base] of popped) setTransform(mesh, { scale: base })
  popped.clear()
}

/** A pickable mesh: unlit color that brightens on hover, scale-pop on
 * tap; `drag` slides it over the floor (claiming the press), `spin`
 * turns it on the wheel (claiming the wheel). setMeshParams writes raw
 * uniform values, so the tint premultiplies here (alpha 1: the rgb passes
 * through). */
function Pickable(p: { name: string; color: Color; geometry: Geometry; position: Vec3; scale?: number; drag?: boolean; spin?: boolean }) {
  let scene = useScene().scene
  let lift = (up: number): [number, number, number, number] => [
    Math.min(p.color[0] + up, 1),
    Math.min(p.color[1] + up, 1),
    Math.min(p.color[2] + up, 1),
    1,
  ]
  // The floor point under the pointer at the mesh's height: the camera
  // ray through the event's scene pixel meets the plane y = mesh y.
  let floorPoint = (e: NodePointerEvent): [number, number] | null => {
    let ray = scene.screenRay(e.x, e.y)
    if (Math.abs(ray.direction[1]) < 1e-6) return null
    let t = (e.mesh.position[1] - ray.origin[1]) / ray.direction[1]
    if (t < 0) return null
    return [ray.origin[0] + t * ray.direction[0], ray.origin[2] + t * ray.direction[2]]
  }
  // The mesh's offset from the grabbed floor point while a drag is on.
  let grab: [number, number] | null = null
  let spin = 0
  return (
    <Mesh
      geometry={p.geometry}
      material={unlit({ color: p.color })}
      position={p.position}
      scale={p.scale}
      ref={m => (meshes[p.name] = m)}
      onPointerEnter={(e: NodePointerEvent) => {
        setHovered(p.name)
        setMeshParams(e.mesh, { uColor: lift(0.25) })
      }}
      onPointerLeave={(e: NodePointerEvent) => {
        setHovered("nothing")
        setMeshParams(e.mesh, { uColor: lift(0) })
      }}
      onTap={(e: NodeTapEvent) => {
        let base = popped.get(e.mesh)
        if (base === undefined) {
          popped.set(e.mesh, p.scale ?? 1)
          setTransform(e.mesh, { scale: (p.scale ?? 1) * POP_SCALE })
        } else {
          popped.delete(e.mesh)
          setTransform(e.mesh, { scale: base })
        }
        let pt = e.point!
        console.log(`tap ${p.name} x${e.tapCount} at ${pt[0].toFixed(2)},${pt[1].toFixed(2)},${pt[2].toFixed(2)} d=${e.distance!.toFixed(2)}`)
      }}
      onPointerDown={
        p.drag
          ? (e: NodePointerEvent) => {
              let f = floorPoint(e)
              if (f === null) return
              grab = [e.mesh.position[0] - f[0], e.mesh.position[2] - f[1]]
              // The claim: this press is the crate's, the orbit never sees it.
              e.stopPropagation()
            }
          : undefined
      }
      onPointerMove={
        p.drag
          ? (e: NodePointerEvent) => {
              if (grab === null) return
              let f = floorPoint(e)
              if (f === null) return
              setTransform(e.mesh, { position: [f[0] + grab[0], e.mesh.position[1], f[1] + grab[1]] })
            }
          : undefined
      }
      onPointerUp={p.drag ? () => (grab = null) : undefined}
      onWheel={
        p.spin
          ? (e: NodeWheelEvent) => {
              spin += e.deltaY * WHEEL_SPIN
              setTransform(e.mesh, { rotation: [0, spin, 0] })
              e.stopPropagation()
            }
          : undefined
      }
    />
  )
}

function App() {
  let floor = plane({ width: 8, height: 8, label: "floor" })
  let crate = box()
  let ball = sphere({ radius: 0.45 })
  let spike = cone({ radius: 0.45, height: 1 })
  let ring = torus({ radius: 0.4, tube: 0.14 })
  // The orbit control's input: the scene leaf's pointer feed, bound to
  // the standard orbit actions (drag rotates, wheel and pinch zoom).
  let pointer = createPointerFeed()
  let input = createInputMap(orbitActions)
  input.bind(orbitBindings({ pointer }))

  registerDebug("state", () => ({
    pose: orbit.pose(),
    crate: meshes.crate?.position,
    ringSpin: meshes.ring ? getRotation(meshes.ring)[1] : null,
    popped: Object.keys(meshes).filter(n => popped.has(meshes[n]!)),
    hovered: hovered(),
  }))
  // The scene fills the window at device density, so a scene pixel over
  // the display scale is a window pixel.
  registerDebug("pixel", (args?: { name?: string }) => {
    let mesh = meshes[args?.name ?? ""]
    if (!mesh) return null
    let p = scene.project(mesh.position)
    if (p === null) return null
    return { x: p.x / displayScale(), y: p.y / displayScale() }
  })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene
          clearColor={[0.07, 0.07, 0.1, 1]}
          label="pick"
          pointer={pointer}
          ref={s => (scene = s)}
          onTap={e => {
            if (e.mesh !== null) return
            console.log(`tap on nothing at ${e.x.toFixed(0)},${e.y.toFixed(0)}: un-pop`)
            unpopAll()
          }}
        >
          <PerspectiveCamera fov={55} />
          <OrbitCamera input={input} target={ORBIT_TARGET} distance={ORBIT_DISTANCE} elevation={ORBIT_ELEVATION} ref={o => (orbit = o)} />
          <Mesh geometry={floor} material={unlit({ color: [0.15, 0.16, 0.2] })} rotation={[-Math.PI / 2, 0, 0]} />
          {/* The group hears the cone's downs through bubbling; the crate's
              claim stops the walk at the crate, so the group never sees those. */}
          <Group onPointerDown={e => console.log(`group saw down on ${e.mesh === meshes.crate ? "crate" : "cone"}`)}>
            <Pickable name="crate" color={[0.85, 0.3, 0.3]} geometry={crate} position={[-1.6, 0.4, 0]} scale={0.8} drag />
            <Pickable name="cone" color={[0.35, 0.65, 0.9]} geometry={spike} position={[1.6, 0.5, 0]} />
          </Group>
          <Pickable name="ball" color={[0.9, 0.8, 0.35]} geometry={ball} position={[0, 0.45, 0]} />
          <Pickable name="ring" color={[0.5, 0.85, 0.5]} geometry={ring} position={[0.9, 0.35, -1.6]} spin />
        </Scene>
        <view position="absolute" x={0} y={0} padding={16} gap={4}>
          <text color="#eef4ff" fontSize={22} fontWeight={700}>
            {`hover: ${hovered()}`}
          </text>
          <text color="#8fa6c8" fontSize={13}>
            hover tints, tap pops, drag the crate; drag elsewhere orbits, wheel zooms (over the ring: spins it), tap nothing to un-pop
          </text>
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
