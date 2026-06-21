// Microphone capture, reactive (SolidJS) layer. A session delivers raw mono
// float32 samples at the requested sample rate (the device format is converted
// by SDL); read() drains whatever was captured since the last call. Captured
// audio buffers until read, so poll read() regularly (e.g. once per frame) while
// open.
//
// The imperative primitive lives in the `flux:microphone` module; import
// { open, listMicrophones } from "flux:microphone" for non-reactive use.

import { createSignal, onCleanup } from "@solidjs/signals"
import { open } from "flux:microphone"

export type MicrophoneOptions = {
  /** Explicit device id from flux:microphone listMicrophones(); default is the system default. */
  microphone?: number
  /** Sample rate of the delivered samples (the device rate is converted). Default 16000. */
  sampleRate?: number
}

/** A live microphone with reactive lifecycle. */
export type MicrophoneStream = {
  /** Sample rate of read() samples (0 if opening failed). */
  sampleRate(): number
  /** Drain the mono float32 samples captured since the last read (empty if not open). */
  read(): Float32Array
  /** Set if opening failed. */
  error(): Error | undefined
}

/**
 * Opens a microphone and owns its lifecycle: closes when the reactive owner is
 * disposed. Capture stays pull-based, so read() drains samples on demand (e.g.
 * once per frame). For imperative use, call open() from "flux:microphone".
 */
export function createMicrophone(options: MicrophoneOptions = {}): MicrophoneStream {
  let [error, setError] = createSignal<Error | undefined>(undefined)
  let session: ReturnType<typeof open> | undefined
  let rate = 0
  try {
    session = open(options)
    rate = session.sampleRate
  } catch (e) {
    setError(e instanceof Error ? e : new Error(String(e)))
  }

  onCleanup(() => {
    if (session) {
      session.close()
      session = undefined
    }
  })

  return {
    sampleRate: () => rate,
    read: () => (session ? session.read() : new Float32Array(0)),
    error,
  }
}