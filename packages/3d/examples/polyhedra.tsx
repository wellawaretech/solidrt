// The polyhedron family beside the lat/long sphere. Back row, flat-shaded
// (detail 0 keeps face normals): tetrahedron, octahedron, dodecahedron and
// icosahedron turning under the lights, so the facets catch the key one
// after another. Middle row, the icosahedron at detail 1, 2 and 3: the
// same builder, now a sphere of uniform triangles with radial normals -
// the icosphere. Front row, the two triangulations as wireframes at a
// matching triangle count: sphere({ widthSegments: 12, heightSegments: 8 })
// bunches slivers at its poles where icosahedron({ detail: 2 }) spreads
// its triangles evenly, which is what makes the icosphere the better
// sphere for flat shading and displacement. The textured sphere stays
// sphere(): the polyhedra's spherical UVs stretch toward the poles.
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { DirectionalLight, dodecahedron, Group, HemisphereLight, icosahedron, phong, Mesh, octahedron, PerspectiveCamera, plane, Scene, sphere, tetrahedron, unlit, wireframeGeometry } from "@solidrt/3d"

// One circumsphere radius for every solid, so the rows read at one scale.
const RADIUS = 0.45
// Row depths, back to front, and the column pitch.
const BACK = -2.1
const MIDDLE = 0
const FRONT = 2
const PITCH = 1.6

function App() {
  let [t, setT] = createSignal(0)
  onFrame(tick => setT(tick / 1000))

  let red = phong({ color: [0.85, 0.3, 0.25] })
  let amber = phong({ color: [0.9, 0.7, 0.3] })
  let green = phong({ color: [0.4, 0.75, 0.4] })
  let blue = phong({ color: [0.35, 0.55, 0.9] })
  let ivory = phong({ color: [0.85, 0.82, 0.75], specular: 0.4, shininess: 40 })
  let wire = unlit({ color: [0.9, 0.9, 0.95] })
  let floor = phong({ color: [0.16, 0.17, 0.22] })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.07, 0.07, 0.1, 1]} samples={4} label="polyhedra">
          <PerspectiveCamera fov={45} position={[0, 4.6, 6.8]} lookAt={[0, 0.1, 0]} />
          <HemisphereLight sky={[0.35, 0.38, 0.45]} ground={[0.12, 0.1, 0.08]} />
          <Group rotation={[0, t() / 3, 0]}>
            <DirectionalLight direction={[0.7, -0.8, 0.2]} color={[1, 0.92, 0.8]} intensity={0.9} />
          </Group>
          <DirectionalLight direction={[-0.6, -0.3, -0.5]} color={[0.4, 0.5, 0.8]} intensity={0.35} />
          <Mesh geometry={plane({ width: 10, height: 8 })} material={floor} rotation={[-Math.PI / 2, 0, 0]} />
          <Mesh geometry={tetrahedron({ radius: RADIUS })} material={red} position={[-1.5 * PITCH, RADIUS, BACK]} rotation={[0, t(), 0]} />
          <Mesh geometry={octahedron({ radius: RADIUS })} material={amber} position={[-0.5 * PITCH, RADIUS, BACK]} rotation={[0, t(), 0]} />
          <Mesh geometry={dodecahedron({ radius: RADIUS })} material={green} position={[0.5 * PITCH, RADIUS, BACK]} rotation={[0, t(), 0]} />
          <Mesh geometry={icosahedron({ radius: RADIUS })} material={blue} position={[1.5 * PITCH, RADIUS, BACK]} rotation={[0, t(), 0]} />
          <Mesh geometry={icosahedron({ radius: RADIUS, detail: 1 })} material={ivory} position={[-PITCH, RADIUS, MIDDLE]} />
          <Mesh geometry={icosahedron({ radius: RADIUS, detail: 2 })} material={ivory} position={[0, RADIUS, MIDDLE]} />
          <Mesh geometry={icosahedron({ radius: RADIUS, detail: 3 })} material={ivory} position={[PITCH, RADIUS, MIDDLE]} />
          <Mesh geometry={wireframeGeometry(sphere({ radius: RADIUS, widthSegments: 12, heightSegments: 8 }))} material={wire} position={[-0.5 * PITCH, RADIUS, FRONT]} rotation={[0, t() / 2, 0]} />
          <Mesh geometry={wireframeGeometry(icosahedron({ radius: RADIUS, detail: 2 }))} material={wire} position={[0.5 * PITCH, RADIUS, FRONT]} rotation={[0, t() / 2, 0]} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
