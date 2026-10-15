// The mixer through flux:audio on the test client: what the surface
// answers at once, without waiting on the mix. The mix renders on the
// audio device's own clock, which the harness does not step (app time is
// frames), so positions advancing, fades landing and sinks draining are
// the probe's to time on a real client (probes/audio-mix-probe.tsx).

import { test, expect } from "@solidrt/test"
import { createSink, loadPcm, setBusGain, stop } from "flux:audio"

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
