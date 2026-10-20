// The mixer through flux:audio on the test client: what the surface
// answers at once, without waiting on the mix. The mix renders on the
// audio device's own clock, which the harness does not step (app time is
// frames), so positions advancing, fades landing and sinks draining are
// the probe's to time on a real client (probes/audio-mix-probe.tsx).

import { test, expect } from "@solidrt/test"
import { createListener, createSink, loadPcm, setBusGain, stop } from "flux:audio"
import { createNode, destroyNode } from "flux:spatial"
import type { NodeId } from "flux:spatial"
import type { TextureId } from "flux:gpu"

/** The emitter option of a play call. */
type Emitter = NonNullable<NonNullable<Parameters<ReturnType<typeof loadPcm>["play"]>[0]>["emitter"]>
/** A target id the test never draws into: the listener's view may be set later. */
const TARGET = 1 as TextureId

const RATE = 48000
// Half a second of a near-silent 440 Hz tone.
const CLIP_SECONDS = 0.5

function tone(seconds: number): Float32Array {
  let data = new Float32Array(Math.floor(RATE * seconds))
  for (let i = 0; i < data.length; i++) {
    data[i] = Math.sin((i * 440 * Math.PI * 2) / RATE) * 0.01
  }
  return data
}

test("a clip knows its duration and a playback starts at zero", () => {
  let clip = loadPcm(tone(CLIP_SECONDS), RATE)
  expect(clip.duration).toBeCloseTo(CLIP_SECONDS, 3)
  let playback = clip.play({ gain: 0 })
  expect(playback.position()).toBe(0)
  expect(playback.ended()).toBe(false)
  playback.stop()
  expect(playback.ended()).toBe(true)
  clip.unload()
  expect(() => clip.play()).toThrow("clip has been unloaded")
})

test("pause, resume, setLoop and the bus controls are accepted on a live playback", () => {
  let clip = loadPcm(tone(CLIP_SECONDS), RATE)
  let playback = clip.play({ gain: 0, loop: true, bus: "music" })
  playback.pause()
  playback.resume()
  playback.setLoop(false)
  setBusGain("music", 0.5, { rampMs: 50 })
  setBusGain("music", 1)
  expect(playback.ended()).toBe(false)
  stop({ bus: "music" })
  expect(playback.ended()).toBe(true)
  clip.unload()
})

test("a sink queues what is pushed and refuses pushes once closed", () => {
  let sink = createSink({ sampleRate: RATE })
  sink.push(tone(0.3))
  expect(sink.queuedMs()).toBeCloseTo(300, 0)
  sink.clear()
  expect(sink.queuedMs()).toBe(0)
  sink.close()
  expect(() => sink.push(tone(0.01))).toThrow()
})

test("the surface validates its arguments", () => {
  expect(() => createSink({ sampleRate: 0 })).toThrow("sampleRate must be a positive integer")
  expect(() => createSink({ sampleRate: RATE, channels: 3 as 1 })).toThrow("channels must be 1 or 2")
  expect(() => setBusGain("", 1)).toThrow("bus must be a non-empty string")
  expect(() => setBusGain("music", -1)).toThrow("gain must be a finite number >= 0")
})

/** A spatial node at `position`, identity rotation, unit scale. */
function node(position: [number, number, number]) {
  return createNode(new Float64Array([...position, 0, 0, 0, 1, 1, 1, 1]), true)
}

test("a playback is placed at a node and heard by a listener", () => {
  let clip = loadPcm(tone(CLIP_SECONDS), RATE)
  let ear = node([0, 0, 0])
  let listener = createListener({ node: ear })
  let kart = node([5, 0, 0])
  // Placed from the start: the emitter owns the pan, so a pan is refused
  // together with it and setPan is refused while attached.
  expect(() => clip.play({ gain: 0, pan: 0.5, emitter: { node: kart, listener } })).toThrow("pan and emitter are exclusive")
  let playback = clip.play({ gain: 0, loop: true, emitter: { node: kart, listener, refDistance: 2, doppler: 0 } })
  expect(() => playback.setPan(0)).toThrow("attached to an emitter")
  // Detached, the pan is the app's again; attached again, it is not.
  playback.detach({ rampMs: 20 })
  playback.setPan(0)
  playback.attach({ position: [0, 0, -3], listener, distanceModel: "linear", maxDistance: 10 })
  expect(() => playback.setPan(0)).toThrow("attached to an emitter")
  playback.stop()
  expect(playback.ended()).toBe(true)
  // Setters on an ended playback stay no-ops, attach included.
  playback.attach({ node: kart, listener })
  listener.set({ view: TARGET }, { speedOfSound: 100 })
  listener.destroy()
  listener.destroy()
  destroyNode(kart)
  destroyNode(ear)
  clip.unload()
})

test("the spatial surface validates its arguments", () => {
  let clip = loadPcm(tone(CLIP_SECONDS), RATE)
  let ear = node([0, 0, 0])
  let listener = createListener({ view: TARGET })
  expect(() => createListener({ node: ear }, { speedOfSound: 0 })).toThrow("speedOfSound must be a finite number > 0")
  expect(() => createListener({} as { view: TextureId })).toThrow("exactly one")
  expect(() => createListener({ node: (ear + 2 ** 40) as NodeId })).toThrow("not found")
  let play = (emitter: Emitter) => () => clip.play({ gain: 0, emitter })
  expect(play({ listener })).toThrow("names a node or a position")
  expect(play({ node: ear, position: [0, 0, 0], listener })).toThrow("names a node or a position")
  expect(play({ node: ear, listener: { id: 999999 } as typeof listener })).toThrow("not found")
  expect(play({ node: ear, listener, refDistance: 0 })).toThrow("refDistance must be")
  expect(play({ node: ear, listener, maxDistance: 0.5 })).toThrow("maxDistance must be")
  expect(play({ node: ear, listener, rolloff: -1 })).toThrow("rolloff must be")
  expect(play({ node: ear, listener, distanceModel: "cubic" as "linear" })).toThrow("distanceModel must be")
  expect(play({ position: [1, 2] as unknown as [number, number, number], listener })).toThrow("position must be [x, y, z]")
  // A refused emitter starts no voice: stopping everything finds none.
  stop()
  listener.destroy()
  destroyNode(ear)
  clip.unload()
})
