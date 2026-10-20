// Positional sound through the package (AGENTS.md Positional sound,
// sound.ts, scene.ts's listener): a sound attached to a node plays placed
// while the node is in the scene, stops and is released when the node
// leaves, places again on re-enter, the scene's listener is one object
// made on first use and re-pointed by setListener, and the checks throw.
// The placement itself is core state the mixer applies, so this is an
// app test over what the surface answers at once.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { createRoot, flush } from "solid-js"
import { createPcmSound } from "@solidrt/core/audio"
import type { Sound, SoundOptions } from "@solidrt/core/audio"
import { add, attachSound, box, createGroup, createMesh, createScene, remove, setOccluder, setTransform, standard } from "../src/index.ts"
import type { Scene } from "../src/scene.ts"

const SIZE = 32
const RATE = 48000
/** Half a second of a near-silent tone. */
const CLIP_SECONDS = 0.5

function tone(): Float32Array {
  let data = new Float32Array(Math.floor(RATE * CLIP_SECONDS))
  for (let i = 0; i < data.length; i++) {
    data[i] = Math.sin((i * 440 * Math.PI * 2) / RATE) * 0.01
  }
  return data
}

/** A sound under a root of its own (the reactive owner its cleanup
 * needs), with the disposer. */
function sound(options: SoundOptions): { sound: Sound; dispose: () => void } {
  let dispose!: () => void
  let made = createRoot(d => {
    dispose = d
    return createPcmSound(tone(), RATE, options)
  })
  return { sound: made, dispose }
}

async function mounted(app: TestApp): Promise<Scene> {
  let scene!: Scene
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { label: "sound" })
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

test("a sound attached to a node plays placed while the node is in the scene", async app => {
  let scene = await mounted(app)
  let kart = createGroup()
  setTransform(kart, { position: [5, 0, 0] })
  add(scene.root, kart)
  let { sound: engine, dispose } = sound({ loop: true, gain: 0 })
  expect(engine.error()).toBeUndefined()
  let detach = attachSound(kart, engine, { refDistance: 2, doppler: 0 })
  engine.play()
  flush()
  expect(engine.playing()).toBe(true)
  await app.settle()
  // Leaving the scene stops the voices (a looping engine must not
  // outlive its kart) and releases the placement.
  remove(kart)
  flush()
  expect(engine.playing()).toBe(false)
  // Back in: placed again under the new core node.
  add(scene.root, kart)
  engine.play()
  flush()
  expect(engine.playing()).toBe(true)
  await app.settle()
  detach()
  engine.stop()
  expect(() => attachSound(kart, engine)).not.toThrow()
  dispose()
})

test("the scene's listener is one object, re-pointed by setListener", async app => {
  let scene = await mounted(app)
  let listener = scene.listener()
  expect(scene.listener()).toBe(listener)
  let head = createGroup()
  add(scene.root, head)
  scene.setListener(head)
  remove(head)
  add(scene.root, head)
  scene.setListener(null)
  let other = createScene(SIZE, SIZE, { label: "other" })
  let stranger = createGroup()
  add(other.root, stranger)
  expect(() => scene.setListener(stranger)).toThrow("another scene")
  other.dispose()
  let { sound: placed, dispose } = sound({ gain: 0, emitter: { node: head._node!, listener } })
  placed.play()
  flush()
  expect(placed.playing()).toBe(true)
  await app.settle()
  placed.stop()
  dispose()
})

test("an occluder's audio field reaches the core", async app => {
  let scene = await mounted(app)
  let wall = createMesh(box({ width: 4, height: 4, depth: 0.2 }), standard({ color: [0.5, 0.5, 0.5] }))
  add(scene.root, wall)
  setOccluder(wall, { shape: "geometry", audio: false })
  setOccluder(wall, { quad: [4, 4], audio: { gain: 0.5, cutoff: 4000 } })
  expect(() => setOccluder(wall, { quad: [4, 4], audio: { gain: 2 } })).toThrow("audio.gain")
  expect(() => setOccluder(wall, { quad: [4, 4], audio: { cutoff: 0 } })).toThrow("audio.cutoff")
  setOccluder(wall, "geometry")
  await app.settle()
})
