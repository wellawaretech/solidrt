// The surface maps on lit(), all procedural so the example is
// self-contained. normalMap bends the lighting per texel with NO tangent
// channel (the frame comes from screen-space derivatives) - the flat
// plane and the sphere both show relief from one bump texture.
// emissive/emissiveMap add light the lights do not provide (the window
// grid stays lit on the dark side). specularMap masks the highlight
// (the striped sphere is chrome on the bright bands, rubber between).
// mapTransform scrolls the belt's uv per frame with one setMeshParams
// write - per MATERIAL, Godot's uv1_offset/scale, not Three's
// per-texture transform. lightMap adds a baked glow by the aUV2 channel
// (withAttribute) at zero light cost.

import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { createTexture } from "@solidrt/core/gpu"
import { box, DirectionalLight, Group, HemisphereLight, lit, Mesh, PerspectiveCamera, plane, Scene, setMeshParams, sphere, withAttribute } from "@solidrt/3d"
import type { MeshNode } from "@solidrt/3d"

const SIZE = 720

// A tangent-space normal map from a procedural height field (a grid of
// round bumps): finite-difference the height, pack the normal as
// OpenGL-style rgb. What every baked normal map contains, minus the
// authoring tool.
function bumpNormalMap() {
  let n = 128
  const BUMPS = 4 // bump grid cells per texture edge
  let height = (x: number, y: number): number => {
    let u = ((x * BUMPS) / n) % 1
    let v = ((y * BUMPS) / n) % 1
    let dx = u - 0.5
    let dy = v - 0.5
    let d = Math.sqrt(dx * dx + dy * dy)
    return d < 0.35 ? Math.cos((d / 0.35) * Math.PI * 0.5) : 0
  }
  const STRENGTH = 3 // height units per texel step: how steep the bumps read
  let data = new Uint8Array(n * n * 4)
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      let sx = (height(x + 1, y) - height(x - 1, y)) * STRENGTH
      let sy = (height(x, y + 1) - height(x, y - 1)) * STRENGTH
      let inv = 1 / Math.sqrt(sx * sx + sy * sy + 1)
      data.set([((-sx * inv) * 0.5 + 0.5) * 255, ((-sy * inv) * 0.5 + 0.5) * 255, (inv * 0.5 + 0.5) * 255, 255], (y * n + x) * 4)
    }
  }
  return createTexture(data, n, n, { wrap: "repeat", mipmap: true })
}

// A window grid: dark wall, warm lit windows - the emissive map.
function windowMap() {
  let n = 64
  let data = new Uint8Array(n * n * 4)
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      let on = (x >> 3) % 2 === 1 && (y >> 3) % 2 === 1 && (x & 7) > 1 && (x & 7) < 6 && (y & 7) > 1 && (y & 7) < 6
      data.set(on ? [255, 190, 90, 255] : [0, 0, 0, 255], (y * n + x) * 4)
    }
  }
  return createTexture(data, n, n, { mipmap: true })
}

// Vertical stripes in the red channel: the specular mask.
function stripeMap() {
  let n = 64
  let data = new Uint8Array(n * n * 4)
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      let v = (x >> 3) & 1 ? 255 : 0
      data.set([v, v, v, 255], (y * n + x) * 4)
    }
  }
  return createTexture(data, n, n, { wrap: "repeat", mipmap: true })
}

// Chevrons for the scrolling belt.
function chevronMap() {
  let n = 64
  let data = new Uint8Array(n * n * 4)
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      let on = ((x + Math.abs(y - n / 2)) >> 3) & 1
      data.set(on ? [235, 220, 60, 255] : [40, 40, 45, 255], (y * n + x) * 4)
    }
  }
  return createTexture(data, n, n, { wrap: "repeat", mipmap: true })
}

// A radial warm glow, the stand-in for an offline lightmap bake.
function bakedGlowMap() {
  let n = 64
  let data = new Uint8Array(n * n * 4)
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      let dx = x / n - 0.5
      let dy = y / n - 0.5
      let g = Math.max(0, 1 - Math.sqrt(dx * dx + dy * dy) * 2.2)
      data.set([255 * g, 180 * g, 90 * g, 255], (y * n + x) * 4)
    }
  }
  return createTexture(data, n, n, { mipmap: true })
}

function App() {
  let [t, setT] = createSignal(0)
  let beltMesh: MeshNode | undefined
  onFrame(tick => {
    setT(tick / 1000)
    // The scrolling belt: one per-frame uniform write, the setTransform
    // split - structure through signals, frame-rate values through refs.
    if (beltMesh !== undefined) setMeshParams(beltMesh, { uMapTransform: [3, 1, tick / 1500, 0] })
  })

  let bumps = bumpNormalMap()
  let bumpy = lit({ color: [0.75, 0.5, 0.3], normalMap: bumps, specular: 0.5, shininess: 40 })
  let flat = lit({ color: [0.75, 0.5, 0.3], specular: 0.5, shininess: 40 })
  let city = lit({ color: [0.25, 0.28, 0.34], emissiveMap: windowMap() })
  let striped = lit({ color: [0.2, 0.22, 0.26], specularMap: stripeMap(), shininess: 90 })
  let belt = lit({ map: chevronMap(), mapTransform: { repeat: [3, 1] } })
  // The ground carries its own uv2 island (here just the 0..1 plane UVs
  // recomputed from position) and adds the "baked" glow with no light.
  let groundGeometry = withAttribute(plane({ width: 8, height: 8 }), { name: "aUV2", format: "vec2" }, (_i, pos) => [
    pos[0] / 8 + 0.5,
    pos[1] / 8 + 0.5,
  ])
  let ground = lit({ color: [0.45, 0.45, 0.5], lightMap: bakedGlowMap(), lightMapIntensity: 1.5 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} designSize={[SIZE, SIZE]}>
        <Scene width={SIZE} height={SIZE} clearColor={[0.06, 0.06, 0.09, 1]} samples={4} label="materials">
          <PerspectiveCamera fov={50} position={[0, 2.8, 5.2]} lookAt={[0, 0.5, 0]} />
          <HemisphereLight sky={[0.3, 0.33, 0.4]} ground={[0.1, 0.09, 0.08]} />
          <Group rotation={[0, t() / 2, 0]}>
            <DirectionalLight direction={[0.7, -0.8, 0.2]} color={[1, 0.95, 0.85]} intensity={1} />
          </Group>
          <Mesh geometry={groundGeometry} material={ground} rotation={[-Math.PI / 2, 0, 0]} />
          <Mesh geometry={sphere({ radius: 0.55 })} material={bumpy} position={[-1.7, 0.55, 0.6]} />
          <Mesh geometry={sphere({ radius: 0.55 })} material={flat} position={[-1.7, 0.55, -0.9]} />
          <Mesh geometry={box({ width: 1, height: 1.6, depth: 1 })} material={city} position={[0, 0.8, -0.6]} rotation={[0, t() / 3, 0]} />
          <Mesh geometry={sphere({ radius: 0.55 })} material={striped} position={[1.7, 0.55, -0.6]} rotation={[0, t() / 2, 0]} />
          <Mesh
            geometry={box({ width: 2.2, height: 0.12, depth: 0.7 })}
            material={belt}
            position={[0.6, 0.06, 1.6]}
            ref={mesh => (beltMesh = mesh)}
          />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
