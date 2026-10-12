// createMicrophone over a test microphone: the clip a test installs is what
// the session reads, a read drains only what came since the last, a later
// install reaches an open session, the session id is there for a runtime
// module to take a reader by, and an open at another rate fails naming both.

import { test, expect } from "@solidrt/test"
import { createMicrophone } from "@solidrt/core/microphone"
import type { MicrophoneStream } from "@solidrt/core/microphone"
import { listMicrophones, open } from "flux:microphone"

const RATE = 16000

// A short ramp, distinct per call so a read can be told apart from another.
function clip(from: number, count: number): Float32Array {
  let samples = new Float32Array(count)
  for (let i = 0; i < count; i++) samples[i] = (from + i) / 1000
  return samples
}

test("the installed clip is the microphone the app opens, and read() drains it once", async app => {
  await app.microphone({ sampleRate: RATE, samples: clip(0, 160) })
  expect(listMicrophones().map(mic => mic.id)).toEqual([0])
  let mic!: MicrophoneStream
  await app.mount(() => {
    mic = createMicrophone({ sampleRate: RATE })
    return <view />
  })
  await app.settle()
  expect(mic.error()).toBe(undefined)
  expect(mic.sampleRate()).toBe(RATE)
  expect(typeof mic.id()).toBe("number")
  expect(mic.read()).toEqual(clip(0, 160))
  expect(mic.read()).toEqual(new Float32Array(0))
})

test("a later install is heard by an open session", async app => {
  await app.microphone({ sampleRate: RATE, samples: clip(0, 16) })
  let mic!: MicrophoneStream
  await app.mount(() => {
    mic = createMicrophone({ sampleRate: RATE })
    return <view />
  })
  await app.settle()
  expect(mic.read()).toEqual(clip(0, 16))
  await app.microphone({ sampleRate: RATE, samples: clip(500, 32) })
  expect(mic.read()).toEqual(clip(500, 32))
})

test("two sessions each hear the whole clip", async app => {
  await app.microphone({ sampleRate: RATE, samples: clip(0, 8) })
  let first = await open({ sampleRate: RATE })
  let second = await open({ sampleRate: RATE })
  expect(first.id).not.toBe(second.id)
  expect(first.read()).toEqual(clip(0, 8))
  expect(second.read()).toEqual(clip(0, 8))
  first.close()
  second.close()
})

test("an open at another rate than the clip's fails naming both", async app => {
  await app.microphone({ sampleRate: RATE, samples: clip(0, 8) })
  let mic!: MicrophoneStream
  await app.mount(() => {
    mic = createMicrophone({ sampleRate: 8000 })
    return <view />
  })
  await app.settle()
  expect(mic.error()?.message).toContain("16000 Hz")
  expect(mic.error()?.message).toContain("8000 Hz")
  expect(mic.read()).toEqual(new Float32Array(0))
})
