// Microphone capture, reactive (SolidJS) layer. A session delivers raw mono
// float32 samples at the requested sample rate (the device format is converted
// by SDL); read() drains whatever was captured since the last call, out of a
// 10 s window the session keeps, so nothing has to poll: read when the app
// wants samples, and a runtime module that works on the audio reads the same
// session on its own thread, given id(). Opening the microphone IS the
// permission request (SDL semantics): it resolves once capture runs and
// rejects if the user denies access.
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

/** A live microphone as reactive accessors. */
export type MicrophoneStream = {
  /** The session id to hand a runtime module that reads this microphone, undefined while opening or if opening failed. */
  id(): number | undefined
  /** Sample rate of read() samples, 0 while opening or if opening failed. */
  sampleRate(): number
  /** Drain the mono float32 samples captured since the last read (empty if not open). */
  read(): Float32Array
  /** Set if opening failed (e.g. permission denied). */
  error(): Error | undefined
}

/**
 * Opens a microphone and exposes it as reactive signals: id() and
 * sampleRate() are set once capture runs, error() if the open failed or the
 * user denied the permission. Closes automatically when the reactive owner
 * is disposed (e.g. the component unmounts). Capture stays pull-based:
 * read() drains the samples captured since the last read, out of the
 * session's 10 s window. For imperative use, call open() from
 * "flux:microphone".
 */
export function createMicrophone(options: MicrophoneOptions = {}): MicrophoneStream {
  let [id, setId] = createSignal<number | undefined>(undefined)
  let [sampleRate, setSampleRate] = createSignal(0)
  let [error, setError] = createSignal<Error | undefined>(undefined)
  let session: Awaited<ReturnType<typeof open>> | undefined
  let disposed = false

  open(options)
    .then((mic) => {
      if (disposed) {
        mic.close()
        return
      }
      session = mic
      setId(mic.id)
      setSampleRate(mic.sampleRate)
    })
    .catch((e) => setError(e instanceof Error ? e : new Error(String(e))))

  onCleanup(() => {
    disposed = true
    if (session) {
      session.close()
      session = undefined
    }
  })

  return {
    id,
    sampleRate,
    read: () => (session ? session.read() : new Float32Array(0)),
    error,
  }
}