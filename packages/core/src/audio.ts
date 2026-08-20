// Sound playback, reactive (SolidJS) layer. `createSound` decodes an encoded
// clip (Ogg/Vorbis or WAV) once and owns its lifecycle: the decoded clip is
// released, and any playing voices stopped, when the reactive owner is disposed.
// Each play() is cheap (no re-decode). `createPcmSound` is the same over raw
// samples the app generated itself; `createSoundStream` reads a large track
// from a file path on demand instead of decoding it into memory.
//
// The imperative primitive lives in the `flux:audio` module; import
// { play, load, loadPcm, stream } from "flux:audio" for non-reactive use.

import { createSignal, onCleanup } from "@solidjs/signals"
import { load, loadPcm, stream } from "flux:audio"
import { file } from "flux:fs"

type FluxFile = ReturnType<typeof file>

type Clip = ReturnType<typeof load>
type Playback = ReturnType<Clip["play"]>

export type SoundOptions = {
  /** Repeat the clip until stopped. Defaults to false. */
  loop?: boolean
  /** Volume scale, 1.0 leaves the clip unchanged. Defaults to 1.0. */
  gain?: number
  /**
   * Stereo position in [-1, 1], -1 = left, 0 = center, 1 = right (equal-power).
   * Omitted means unspatialized.
   */
  pan?: number
  /**
   * Playback rate: 1.0 plays as loaded, higher is faster and higher-pitched
   * (clamped to [0.01, 100]). Defaults to 1.0.
   */
  rate?: number
  /** Fade each play() in from silence over this many milliseconds. */
  fadeInMs?: number
  /** Bus name for every voice of this sound (see flux:audio `stop({ bus })`). */
  bus?: string
  /**
   * Let play() stack overlapping voices instead of restarting. Defaults to
   * true: rapid triggers overlap. Set false for a single-voice sound where each
   * play() cuts off the previous one.
   */
  overlap?: boolean
}

/** Options for the live setters: ramp to the value instead of jumping. */
export type SoundRampOptions = {
  /**
   * Reach the new value over this many milliseconds, engine-smoothed (immune
   * to frame hitches). Omitted (or 0) sets immediately.
   */
  rampMs?: number
}

/** Options for {@link Sound.stop}. */
export type SoundStopOptions = {
  /** Fade to silence over this many milliseconds before stopping. */
  fadeOutMs?: number
}

/** Options for a PCM sound: `SoundOptions` plus the channel count. */
export type PcmSoundOptions = SoundOptions & {
  /** Channel count, interleaved samples when 2. Defaults to 1 (mono). */
  channels?: 1 | 2
}

/** Options for a streamed sound. Streams are always single-voice. */
export type SoundStreamOptions = {
  /** Repeat the track until stopped. Defaults to false. */
  loop?: boolean
  /** Volume scale, 1.0 leaves the track unchanged. Defaults to 1.0. */
  gain?: number
  /** Stereo position in [-1, 1] (see {@link SoundOptions.pan}). */
  pan?: number
  /** Playback rate (see {@link SoundOptions.rate}). Defaults to 1.0. */
  rate?: number
  /** Fade each play() in from silence over this many milliseconds. */
  fadeInMs?: number
  /** Bus name for the stream's voice (see flux:audio `stop({ bus })`). */
  bus?: string
}

/** A decoded sound with reactive lifecycle. */
export type Sound = {
  /** Start the clip. Overlaps or restarts per the `overlap` option. */
  play(): void
  /** Stop every voice started from this sound, fading first if asked. */
  stop(options?: SoundStopOptions): void
  /** Set the volume of every live voice, and of voices started later. */
  setGain(gain: number, options?: SoundRampOptions): void
  /** Set the stereo position of every live voice, and of voices started later. */
  setPan(pan: number, options?: SoundRampOptions): void
  /** Set the playback rate of every live voice, and of voices started later. */
  setRate(rate: number, options?: SoundRampOptions): void
  /** True after play() until stop() (does not track natural completion). */
  playing(): boolean
  /** Set if loading failed. */
  error(): Error | undefined
}

// Shared reactive wrapper: owns the loaded clip, tracks live voices, and
// disposes both on cleanup. `loader` runs once (may throw -> error signal).
// Gain, pan and rate are remembered so later voices start where the setters
// left the sound, not back at the initial options.
function reactiveSound(
  loader: () => Clip,
  overlap: boolean,
  initial: { loop?: boolean; gain?: number; pan?: number; rate?: number; fadeInMs?: number; bus?: string },
): Sound {
  let [error, setError] = createSignal<Error | undefined>(undefined, { ownedWrite: true })
  let [playing, setPlaying] = createSignal(false, { ownedWrite: true })

  let clip: Clip | undefined
  let voices: Playback[] = []
  let loop = initial.loop
  let gain = initial.gain
  let pan = initial.pan
  let rate = initial.rate
  let fadeInMs = initial.fadeInMs
  let bus = initial.bus
  try {
    clip = loader()
  } catch (e) {
    setError(e instanceof Error ? e : new Error(String(e)))
  }

  // Voices that finished on their own keep a dead handle in `voices` until the
  // next call here; ended() lets each touch point clear them out.
  let prune = () => {
    voices = voices.filter((v) => !v.ended())
  }

  let stopAll = (options?: SoundStopOptions) => {
    for (let v of voices) v.stop(options)
    voices = []
    setPlaying(false)
  }

  onCleanup(() => {
    stopAll()
    if (clip) {
      clip.unload()
      clip = undefined
    }
  })

  return {
    play() {
      if (!clip) return
      if (overlap) prune()
      else stopAll()
      voices.push(clip.play({ loop, gain, pan, rate, fadeInMs, bus }))
      setPlaying(true)
    },
    stop: stopAll,
    setGain(value, options) {
      gain = value
      prune()
      for (let v of voices) v.setGain(value, options)
    },
    setPan(value, options) {
      pan = value
      prune()
      for (let v of voices) v.setPan(value, options)
    },
    setRate(value, options) {
      rate = value
      prune()
      for (let v of voices) v.setRate(value, options)
    },
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
    pan: options.pan,
    rate: options.rate,
    fadeInMs: options.fadeInMs,
    bus: options.bus,
  })
}

/**
 * A sound over raw PCM samples the app generated itself - no decoding, no
 * container. The typed array is the sample format (Uint8Array = unsigned
 * 8-bit, Int16Array = signed 16-bit, Float32Array = 32-bit float), interleaved
 * when `channels` is 2. Same handle and lifecycle as createSound; on a box with
 * no audio device the clip fails to load, so `error()` is set and play() is a
 * no-op, exactly like createSound there. For imperative use, call loadPcm()
 * from "flux:audio".
 */
export function createPcmSound(
  samples: Uint8Array | Int16Array | Float32Array,
  sampleRate: number,
  options: PcmSoundOptions = {},
): Sound {
  return reactiveSound(() => loadPcm(samples, sampleRate, { channels: options.channels }), options.overlap ?? true, {
    loop: options.loop,
    gain: options.gain,
    pan: options.pan,
    rate: options.rate,
    fadeInMs: options.fadeInMs,
    bus: options.bus,
  })
}

/**
 * Streams a large track, decoding on demand instead of loading it into memory.
 * Single-voice: each play() restarts it. Pass a path (resolved like flux:fs,
 * relative to the process cwd) or a `file()` from flux:fs; a path is wrapped in
 * `file()` for you, so a dev-server-proxied file streams over the proxy. Owns
 * the stream's lifecycle: stopped and released when the reactive owner is
 * disposed. For imperative use, call stream()/play() from "flux:audio".
 */
export function createSoundStream(source: string | FluxFile, options: SoundStreamOptions = {}): Sound {
  let src = typeof source === "string" ? file(source) : source
  return reactiveSound(() => stream(src), false, {
    loop: options.loop,
    gain: options.gain,
    pan: options.pan,
    rate: options.rate,
    fadeInMs: options.fadeInMs,
    bus: options.bus,
  })
}
