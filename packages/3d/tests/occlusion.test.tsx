// Occlusion culling through the package (agents/large-worlds.md Culling,
// occluder.ts, the core's spatial/occlusion.rs): a wall marked as an
// occluder hides the mesh behind it and not the ones before or beside it,
// read back through scene.cullState; the wall moving or hiding uncovers;
// the scene's and a view's switches hold; occludersFromMeshes marks the
// large meshes; the components carry the prop; the shortcuts refuse the
// meshes whose triangles move. The buffer and the gate are core state over
// live scene nodes, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { add, box, createMesh, createScene, createSprite, Group, Mesh, occludersFromMeshes, Scene, setOccluder, setTransform, setVisible, sprite, standard } from "../src/index.ts"
import type { MeshNode, SceneHandle } from "../src/index.ts"
import type { Scene as SceneApi } from "../src/scene.ts"

const SIZE = 64
/** The camera on +z looking at the origin; the wall across the view at
 * z = 0, the probes 3 units before and behind it. */
const EYE = 10
const DEPTH = 3
/** The wall: 4 by 4, far wider than a probe at this distance. */
const WALL = 4
/** A probe box's side. */
const PROBE = 0.8

async function mounted(app: TestApp): Promise<SceneApi> {
  let scene!: SceneApi
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { label: "occlusion" })
    scene.setCamera({ position: [0, 0, EYE], target: [0, 0, 0] })
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

let gray = () => standard({ color: [0.5, 0.5, 0.5] })

function probe(scene: SceneApi, position: [number, number, number]): MeshNode {
  let m = createMesh(box({ width: PROBE, height: PROBE, depth: PROBE }), gray())
  setTransform(m, { position })
  add(scene.root, m)
  return m
}

/** A wall at the origin facing the camera, occluding by its geometry. */
function wall(scene: SceneApi): MeshNode {
  let w = createMesh(box({ width: WALL, height: WALL, depth: 0.2 }), gray())
  setOccluder(w, "geometry")
  add(scene.root, w)
  return w
}

test("a wall hides the mesh behind it, not the ones before or beside it; moving or hiding the wall uncovers", async app => {
  let scene = await mounted(app)
  let w = wall(scene)
  let behind = probe(scene, [0, 0, -DEPTH])
  let before = probe(scene, [0, 0, DEPTH])
  let beside = probe(scene, [WALL, 0, -DEPTH])
  await app.settle()
  expect(scene.cullState(behind)).toBe("occluded")
  expect(scene.cullState(before)).toBe("on")
  expect(scene.cullState(beside)).toBe("on")
  expect(scene.cullState(w)).toBe("on")
  setTransform(w, { position: [WALL * 3, 0, 0] })
  await app.settle()
  expect(scene.cullState(behind)).toBe("on")
  setTransform(w, { position: [0, 0, 0] })
  await app.settle()
  expect(scene.cullState(behind)).toBe("occluded")
  setVisible(w, false)
  await app.settle()
  expect(scene.cullState(behind)).toBe("on")
  setVisible(w, true)
  await app.settle()
  expect(scene.cullState(behind)).toBe("occluded")
  setOccluder(w, null)
  await app.settle()
  expect(scene.cullState(behind)).toBe("on")
})

test("the scene's and a view's switches hold, and a view reads through its own camera", async app => {
  let scene = await mounted(app)
  wall(scene)
  let behind = probe(scene, [0, 0, -DEPTH])
  let view = scene.createView({ width: SIZE, height: SIZE, label: "occlusion-view" })
  view.setCamera({ position: [0, 0, EYE], target: [0, 0, 0] })
  let open = scene.createView({ width: SIZE, height: SIZE, label: "occlusion-open", occlusion: false })
  open.setCamera({ position: [0, 0, EYE], target: [0, 0, 0] })
  // A view from behind the wall: the probe is before it from there.
  let reverse = scene.createView({ width: SIZE, height: SIZE, label: "occlusion-reverse" })
  reverse.setCamera({ position: [0, 0, -EYE], target: [0, 0, 0] })
  await app.settle()
  expect(scene.cullState(behind)).toBe("occluded")
  expect(view.cullState(behind)).toBe("occluded")
  expect(open.cullState(behind)).toBe("on")
  expect(reverse.cullState(behind)).toBe("on")
  scene.setOcclusion(false)
  view.setOcclusion(false)
  await app.settle()
  expect(scene.cullState(behind)).toBe("on")
  expect(view.cullState(behind)).toBe("on")
  scene.setOcclusion(true)
  open.setOcclusion(true)
  await app.settle()
  expect(scene.cullState(behind)).toBe("occluded")
  expect(open.cullState(behind)).toBe("occluded")
  // A mesh the camera cannot see at all reads the frustum first.
  let far = probe(scene, [100, 0, 0])
  let hidden = probe(scene, [0, 0, -DEPTH])
  setVisible(hidden, false)
  await app.settle()
  expect(scene.cullState(far)).toBe("frustum")
  expect(scene.cullState(hidden)).toBe("hidden")
})

test("occludersFromMeshes marks the large meshes by their geometry and skips what cannot occlude", async app => {
  let scene = await mounted(app)
  let w = createMesh(box({ width: WALL, height: WALL, depth: 0.2 }), gray())
  add(scene.root, w)
  let behind = probe(scene, [0, 0, -DEPTH])
  let dot = createSprite(sprite({ color: [1, 1, 1] }))
  setTransform(dot, { position: [0, 0, DEPTH], scale: [WALL, WALL, 1] })
  add(scene.root, dot)
  await app.settle()
  expect(scene.cullState(behind)).toBe("on")
  let marked = occludersFromMeshes(scene, { minSize: WALL - 1 })
  expect(marked).toEqual([w])
  await app.settle()
  expect(scene.cullState(behind)).toBe("occluded")
  expect(() => setOccluder(dot, "geometry")).toThrow("sprite")
  expect(() => setOccluder(dot, "bounds")).toThrow("sprite")
  expect(() => occludersFromMeshes(scene, { minSize: -1 })).toThrow("minSize")
})

test("the components carry the occluder: a Mesh by its geometry, a Group by an explicit quad", async app => {
  let scene!: SceneHandle
  let wallRef!: MeshNode
  let behindWall!: MeshNode
  let behindQuad!: MeshNode
  let probeGeometry = box({ width: PROBE, height: PROBE, depth: PROBE })
  await app.mount(() => (
    <Scene label="occlusion-components" width={SIZE} height={SIZE} camera={{ position: [0, 0, EYE], target: [0, 0, 0] }} ref={s => (scene = s)}>
      <Mesh geometry={box({ width: WALL, height: WALL, depth: 0.2 })} material={gray()} occluder="geometry" position={[-WALL, 0, 0]} ref={m => (wallRef = m)} />
      <Mesh geometry={probeGeometry} material={gray()} position={[-WALL, 0, -DEPTH]} ref={m => (behindWall = m)} />
      <Group position={[WALL, 0, 0]} occluder={{ quad: [WALL, WALL] }} />
      <Mesh geometry={probeGeometry} material={gray()} position={[WALL, 0, -DEPTH]} ref={m => (behindQuad = m)} />
    </Scene>
  ))
  await app.settle()
  expect(scene.cullState(wallRef)).toBe("on")
  expect(scene.cullState(behindWall)).toBe("occluded")
  expect(scene.cullState(behindQuad)).toBe("occluded")
})
