// Microphone capture. A session delivers raw mono float32 samples at the
// requested sample rate (the device format is converted by SDL); read()
// drains whatever was captured since the last call. Captured audio buffers
// until read, so poll read() regularly (e.g. once per frame) while open.

import { createSignal, onCleanup } from "@solidjs/signals"

export type MicrophoneInfo = {
  id: number
  name: string
}

export type MicrophoneOptions = {
  /** Explicit device id from listMicrophones(); default is the system default recording device. */
  microphone?: number
  /** Sample rate of the delivered samples (the device rate is converted). Default 16000. */
  sampleRate?: number
}

export type Microphone = {
  /** Sample rate of read() samples. */
  sampleRate: number
  /** Drain the mono float32 samples captured since the last read. */
  read(): Float32Array
  /** Release the device. */
  close(): void
}

export function listMicrophones(): MicrophoneInfo[] {
  return microphone.listMicrophones()
}

/**
 * Opens a microphone for capture. Async to leave room for an OS permission
 * prompt on platforms that need one (the desktop backends open synchronously).
 */
export async function openMicrophone(options: MicrophoneOptions = {}): Promise<Microphone> {
  let opened = microphone.open(options)
  return {
    sampleRate: opened.sampleRate,
    read: () => microphone.read(opened.handle),
    close: () => microphone.close(opened.handle),
  }
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
 * once per frame). The lower-level openMicrophone() is the imperative
 * alternative.
 */
export function createMicrophone(options: MicrophoneOptions = {}): MicrophoneStream {
  let [error, setError] = createSignal<Error | undefined>(undefined)
  let handle: number | undefined
  let rate = 0
  try {
    let opened = microphone.open(options)
    handle = opened.handle
    rate = opened.sampleRate
  } catch (e) {
    setError(e instanceof Error ? e : new Error(String(e)))
  }

  onCleanup(() => {
    if (handle !== undefined) {
      microphone.close(handle)
      handle = undefined
    }
  })

  return {
    sampleRate: () => rate,
    read: () => (handle !== undefined ? microphone.read(handle) : new Float32Array(0)),
    error,
  }
}