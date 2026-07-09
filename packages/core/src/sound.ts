// Sound playback, reactive (SolidJS) layer. `createSound` decodes an encoded
// clip (Ogg/Vorbis or WAV) once and owns its lifecycle: the decoded clip is
// released, and any playing voices stopped, when the reactive owner is disposed.
// Each play() is cheap (no re-decode).
//
// The imperative primitive lives in the `flux:audio` module; import
// { play, load } from "flux:audio" for non-reactive use.

import { createSignal, onCleanup } from "@solidjs/signals"
import { load } from "flux:audio"

export type SoundOptions = {
  /** Repeat the clip until stopped. Defaults to false. */
  loop?: boolean
  /** Volume scale, 1.0 leaves the clip unchanged. Defaults to 1.0. */
  gain?: number
  /**
   * Let play() stack overlapping voices instead of restarting. Defaults to
   * true: rapid triggers overlap. Set false for a single-voice sound where each
   * play() cuts off the previous one.
   */
  overlap?: boolean
}

/** A decoded sound with reactive lifecycle. */
export type Sound = {
  /** Start the clip. Overlaps or restarts per the `overlap` option. */
  play(): void
  /** Stop every voice started from this sound. */
  stop(): void
  /** True after play() until stop() (does not track natural completion). */
  playing(): boolean
  /** Set if decoding failed. */
  error(): Error | undefined
}

/**
 * Decodes a sound once and owns its lifecycle: releases the clip and stops its
 * voices when the reactive owner is disposed. play() replays without decoding.
 * For imperative use, call load()/play() from "flux:audio".
 */
export function createSound(source: Uint8Array, options: SoundOptions = {}): Sound {
  let [error, setError] = createSignal<Error | undefined>(undefined)
  let [playing, setPlaying] = createSignal(false)
  let overlap = options.overlap ?? true

  let handle: ReturnType<typeof load> | undefined
  let voices: { stop(): void }[] = []
  try {
    handle = load(source)
  } catch (e) {
    setError(e instanceof Error ? e : new Error(String(e)))
  }

  let stopAll = () => {
    for (let v of voices) v.stop()
    voices = []
    setPlaying(false)
  }

  onCleanup(() => {
    stopAll()
    if (handle) {
      handle.unload()
      handle = undefined
    }
  })

  return {
    play() {
      if (!handle) return
      if (!overlap) stopAll()
      voices.push(handle.play({ loop: options.loop, gain: options.gain }))
      setPlaying(true)
    },
    stop: stopAll,
    playing,
    error,
  }
}
