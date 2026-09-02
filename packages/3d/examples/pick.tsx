// Mesh pointer events: hover to tint (enter/leave), click to pop (down),
// with a Group seeing its children's clicks through bubbling and one mesh
// stopping the walk. The scene is STATIC - no onFrame - so it renders only
// when an event changes something: picking is demand-driven like
// everything else. Events flow because the built-in <Scene> leaf carries
// scene.handlers (default on); hit testing runs over the scene's BVH, so a
// pointer move costs O(log meshes).
//
// The volume tier is honest about its shape: hits test each mesh's
// bounding box, so hovering just outside the ball's silhouette (inside its
// box corners) still counts. Triangle-accurate hits are a later tier.

import { createSignal, pct, render } from "@solidrt/core"
import { box, cone, Group, Mesh, PerspectiveCamera, plane, Scene, setMeshParams, setTransform, sphere, torus, unlit } from "@solidrt/3d"
import type { Geometry, ScenePointerEvent, Vec3 } from "@solidrt/3d"

type Color = [number, number, number]

let [hovered, setHovered] = createSignal("nothing")
let popped = new Set<object>()

/** A pickable mesh: unlit color that brightens on hover, scale-pop on
 * click. setMeshParams writes raw uniform values, so the tint premultiplies
 * here (alpha 1: the rgb passes through). */
function Pickable(p: { name: string; color: Color; geometry: Geometry; position: Vec3; scale?: number; stop?: boolean }) {
  let lift = (up: number): [number, number, number, number] => [
    Math.min(p.color[0] + up, 1),
    Math.min(p.color[1] + up, 1),
    Math.min(p.color[2] + up, 1),
    1,
  ]
  return (
    <Mesh
      geometry={p.geometry}
      material={unlit({ color: p.color })}
      position={p.position}
      scale={p.scale}
      onPointerEnter={(e: ScenePointerEvent) => {
        setHovered(p.name)
        setMeshParams(e.mesh, { uColor: lift(0.25) })
        console.log(`enter ${p.name}`)
      }}
      onPointerLeave={(e: ScenePointerEvent) => {
        setHovered("nothing")
        setMeshParams(e.mesh, { uColor: lift(0) })
        console.log(`leave ${p.name}`)
      }}
      onPointerDown={(e: ScenePointerEvent) => {
        let pop = !popped.has(e.mesh)
        if (pop) popped.add(e.mesh)
        else popped.delete(e.mesh)
        setTransform(e.mesh, { scale: pop ? (p.scale ?? 1) * 1.25 : (p.scale ?? 1) })
        let pt = e.point!
        console.log(`down ${p.name} at ${pt[0].toFixed(2)},${pt[1].toFixed(2)},${pt[2].toFixed(2)} d=${e.distance!.toFixed(2)}`)
        // The cone demonstrates stopping the bubble: its group never hears it.
        if (p.stop) e.stopPropagation()
      }}
    />
  )
}

function App() {
  let floor = plane({ width: 8, height: 8, label: "floor" })
  let crate = box()
  let ball = sphere({ radius: 0.45 })
  let spike = cone({ radius: 0.45, height: 1 })
  let ring = torus({ radius: 0.4, tube: 0.14 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.07, 0.07, 0.1, 1]} label="pick">
          <PerspectiveCamera fov={55} position={[0, 2.6, 5]} lookAt={[0, 0.4, 0]} />
          <Mesh geometry={floor} material={unlit({ color: [0.15, 0.16, 0.2] })} rotation={[-Math.PI / 2, 0, 0]} />
          {/* The group hears every child click that is not stopped. */}
          <Group onPointerDown={() => console.log("group saw down")}>
            <Pickable name="crate" color={[0.85, 0.3, 0.3]} geometry={crate} position={[-1.6, 0.4, 0]} scale={0.8} />
            <Pickable name="cone" color={[0.35, 0.65, 0.9]} geometry={spike} position={[1.6, 0.5, 0]} stop />
          </Group>
          <Pickable name="ball" color={[0.9, 0.8, 0.35]} geometry={ball} position={[0, 0.45, 0]} />
          <Pickable name="ring" color={[0.5, 0.85, 0.5]} geometry={ring} position={[0.9, 0.35, -1.6]} />
        </Scene>
        <view position="absolute" x={0} y={0} padding={16} gap={4}>
          <text color="#eef4ff" fontSize={22} fontWeight={700}>
            {`hover: ${hovered()}`}
          </text>
          <text color="#8fa6c8" fontSize={13}>
            hover tints, click pops - a static scene, rendered only on change
          </text>
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
