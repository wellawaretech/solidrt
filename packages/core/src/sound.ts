// Sound playback, reactive (SolidJS) layer. `createSound` decodes an encoded
// clip (Ogg/Vorbis or WAV) once and owns its lifecycle: the decoded clip is
// released, and any playing voices stopped, when the reactive owner is disposed.
// Each play() is cheap (no re-decode). `createSoundStream` is the same but reads
// a large track from a file path on demand instead of decoding it into memory.
//
// The imperative primitive lives in the `flux:audio` module; import
// { play, load, stream } from "flux:audio" for non-reactive use.

import { createSignal, onCleanup } from "@solidjs/signals"
import { load, stream } from "flux:audio"

type LoadedSound = ReturnType<typeof load>

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

/** Options for a streamed sound. Streams are always single-voice. */
export type SoundStreamOptions = {
  /** Repeat the track until stopped. Defaults to false. */
  loop?: boolean
  /** Volume scale, 1.0 leaves the track unchanged. Defaults to 1.0. */
  gain?: number
}

/** A decoded sound with reactive lifecycle. */
export type Sound = {
  /** Start the clip. Overlaps or restarts per the `overlap` option. */
  play(): void
  /** Stop every voice started from this sound. */
  stop(): void
  /** True after play() until stop() (does not track natural completion). */
  playing(): boolean
  /** Set if loading failed. */
  error(): Error | undefined
}

// Shared reactive wrapper: owns the loaded handle, tracks live voices, and
// disposes both on cleanup. `loader` runs once (may throw -> error signal).
function reactiveSound(
  loader: () => LoadedSound,
  overlap: boolean,
  playOptions: { loop?: boolean; gain?: number },
): Sound {
  let [error, setError] = createSignal<Error | undefined>(undefined, { ownedWrite: true })
  let [playing, setPlaying] = createSignal(false, { ownedWrite: true })

  let handle: LoadedSound | undefined
  let voices: { stop(): void }[] = []
  try {
    handle = loader()
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
      voices.push(handle.play(playOptions))
      setPlaying(true)
    },
    stop: stopAll,
    playing,
    error,
  }
}

/**
 * Decodes a sound once and owns its lifecycle: releases the clip and stops its
 * voices when the reactive owner is disposed. play() replays without decoding.
 * For imperative use, call load()/play() from "flux:audio".
 */
export function createSound(source: Uint8Array, options: SoundOptions = {}): Sound {
  return reactiveSound(() => load(source), options.overlap ?? true, {
    loop: options.loop,
    gain: options.gain,
  })
}

/**
 * Streams a large track from a file path, decoding on demand instead of loading
 * it into memory. Single-voice: each play() restarts it. The path resolves like
 * flux:fs (relative to the process cwd), so the file must exist on disk. Owns
 * the stream's lifecycle: stopped and released when the reactive owner is
 * disposed. For imperative use, call stream()/play() from "flux:audio".
 */
export function createSoundStream(path: string, options: SoundStreamOptions = {}): Sound {
  return reactiveSound(() => stream(path), false, { loop: options.loop, gain: options.gain })
}
