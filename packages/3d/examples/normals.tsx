// Normals as data. Three cylinders from ONE triangle soup (a cylinder
// split by toNonIndexed, every face its own vertices - the shape an STL
// or a hand-written face list arrives in) shaded by withNormals at three
// crease angles: 0 is flat, the default 60 finds the rims and smooths the
// side, 180 blends the rims away. The result is indexed again -
// mergeVertices welds the corners back wherever a crease or a seam does
// not need them apart - so each costs what the generator's own cylinder
// costs. The sphere is the CPU deformation loop: fillAttribute moves its
// positions, computeVertexNormals recomputes its normals in place (the
// math runs in the core, one call), updateVertices re-uploads. Its normal
// lines are refreshed the same way, in place, from the sphere's data. The
// loop is here to be seen; a ripple like this one belongs in a vertex
// shader in a real app, where the CPU touches no vertex at all - the loop
// is for data that genuinely changes on the CPU.
// normalsHelper draws every vertex normal as a line: a crease is a fan of
// lines from one position, a smooth vertex one line. Space or the
// `helpers` debug command ({ on?: boolean }) toggles them.

import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { registerDebug } from "srt:dev"
import { computeVertexNormals, cylinder, DirectionalLight, fillAttribute, geometryAttribute, geometryVertexCount, Group, HemisphereLight, phong, Mesh, normalsHelper, PerspectiveCamera, Scene, sphere, toNonIndexed, unlit, updateVertices, withNormals } from "@solidrt/3d"

// The crease angles in degrees: flat, the default, everything smoothed.
const ANGLES = [0, 60, 180] as const
// Spacing between the three cylinders along x.
const SPACING = 2.4
// Tilt of the cylinders toward the camera, so their top rims show.
const TILT = 0.5
// Normal line length, a fraction of the unit shapes.
const HELPER_SIZE = 0.2
// Facets around the cylinders: few enough that flat shading reads as facets.
const RADIAL_SEGMENTS = 16
// The sphere ripple: radial amplitude, waves along y, radians per second.
const RIPPLE_AMPLITUDE = 0.12
const RIPPLE_WAVES = 5
const RIPPLE_SPEED = 2
const SHADE_COLOR: [number, number, number] = [0.85, 0.55, 0.3]
const SPHERE_COLOR: [number, number, number] = [0.35, 0.6, 0.85]

function App() {
  let [t, setT] = createSignal(0)
  let [helpers, setHelpers] = createSignal(true)
  let toggle = (): void => {
    setHelpers(v => !v)
  }
  registerDebug("helpers", (args?: Record<string, unknown>) => {
    if (typeof args?.on === "boolean") setHelpers(args.on)
    else toggle()
  })

  let soup = toNonIndexed(cylinder({ radialSegments: RADIAL_SEGMENTS }), "soup")
  let shaded = ANGLES.map(angle => withNormals(soup, angle, "cylinder-" + angle))
  let shade = phong({ color: SHADE_COLOR, specular: 0.5, shininess: 40 })
  let lines = unlit({ vertexColors: true })

  // The rippling sphere. Base positions are kept aside: fillAttribute's
  // callback sees the CURRENT position, and the ripple is a function of
  // the rest shape.
  let ball = sphere({ label: "ripple" })
  let count = geometryVertexCount(ball, "normals example")
  let pos = geometryAttribute(ball, "aPos")!
  let nrm = geometryAttribute(ball, "aNormal")!
  let base = new Float32Array(count * 3)
  for (let i = 0; i < count; i++) {
    base[i * 3] = pos.get(i, 0)
    base[i * 3 + 1] = pos.get(i, 1)
    base[i * 3 + 2] = pos.get(i, 2)
  }
  let ballLines = normalsHelper(ball, { size: HELPER_SIZE })
  onFrame(tick => {
    setT(tick / 1000)
    let phase = (tick / 1000) * RIPPLE_SPEED
    fillAttribute(ball, "aPos", i => {
      let x = base[i * 3]!, y = base[i * 3 + 1]!, z = base[i * 3 + 2]!
      let r = 1 + RIPPLE_AMPLITUDE * Math.sin(y * RIPPLE_WAVES * Math.PI + phase)
      return [x * r, y * r, z * r]
    })
    computeVertexNormals(ball)
    updateVertices(ball)
    // The helper's line ends follow: vertex 2v is the sphere's vertex v,
    // 2v + 1 its tip along the recomputed normal.
    fillAttribute(ballLines, "aPos", i => {
      let v = i >> 1
      let x = pos.get(v, 0), y = pos.get(v, 1), z = pos.get(v, 2)
      if ((i & 1) === 0) return [x, y, z]
      return [x + nrm.get(v, 0) * HELPER_SIZE, y + nrm.get(v, 1) * HELPER_SIZE, z + nrm.get(v, 2) * HELPER_SIZE]
    })
    updateVertices(ballLines)
  })

  return (
    <window
      onKeyDown={e => {
        if (e.key === " ") toggle()
      }}
    >
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.1, 0.11, 0.14, 1]} samples={4} label="normals">
          <PerspectiveCamera fov={40} position={[0, 1.6, 8.5]} lookAt={[0, 0, 0]} />
          <HemisphereLight sky={[0.35, 0.38, 0.45]} ground={[0.12, 0.1, 0.08]} />
          <DirectionalLight direction={[0.4, -0.7, -0.6]} color={[1, 0.95, 0.85]} intensity={0.9} />
          {shaded.map((geometry, i) => (
            <Group position={[(i - 1) * SPACING, 1.1, 0]} rotation={[TILT, t() / 3, 0]}>
              <Mesh geometry={geometry} material={shade} />
              <Mesh geometry={normalsHelper(geometry, { size: HELPER_SIZE })} material={lines} visible={helpers()} />
            </Group>
          ))}
          <Group position={[0, -1.3, 0]}>
            <Mesh geometry={ball} material={phong({ color: SPHERE_COLOR, specular: 0.4, shininess: 30 })} />
            <Mesh geometry={ballLines} material={lines} visible={helpers()} />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
