// Sound playback, reactive (SolidJS) layer. `createSound` decodes an encoded
// clip (WAV, or Opus in Ogg or WebM) once and owns its lifecycle: the decoded
// clip is released, and any playing voices stopped, when the reactive owner
// is disposed. Each play() is cheap (no re-decode). `createPcmSound` is the
// same over raw samples the app generated itself; `createSoundStream` reads
// a large track from a file on demand instead of decoding it into memory;
// `createSink` plays samples as the app produces them. A sound is placed in
// the world with `emitter` (the option, or `setEmitter`): bound to a
// spatial node or a fixed position and heard by a `createListener`
// listener, the spatial core writes its pan, distance gain, doppler rate
// and occlusion low-pass after every flush that moved something, with no
// per-frame JS; @solidrt/3d's `attachSound` and a 2d view's `listener()`
// wire the scene's nodes and camera to this.
//
// The imperative primitive lives in the `flux:audio` module; import
// { play, load, loadPcm, stream, createSink, createListener } from
// "flux:audio" for non-reactive use.

import { createSignal, onCleanup } from "@solidjs/signals"
import { createListener as openListener, createSink as openSink, load, loadPcm, stream } from "flux:audio"
import { file } from "flux:fs"

type FluxFile = ReturnType<typeof file>

type Clip = ReturnType<typeof load>
type Playback = ReturnType<Clip["play"]>
type FluxSink = ReturnType<typeof openSink>

/** A positional sound's listener (flux:audio createListener): what it is
 * heard from, a spatial node or a target's view (the camera). */
export type Listener = ReturnType<typeof openListener>
/** `{ node }` or `{ view }`: where a listener hears from. */
export type ListenerSource = Parameters<typeof openListener>[0]
/** `{ speedOfSound? }`: world units per second (343 for metres). */
export type ListenerOptions = NonNullable<Parameters<typeof openListener>[1]>
/** Where a sound is placed and how it falls off (flux:audio
 * EmitterOptions): a node or a fixed position, the listener, the Web
 * Audio distance model (inverse by default), refDistance, maxDistance,
 * rolloff, and the doppler factor (1 physical, 0 off). */
export type EmitterOptions = Parameters<Playback["attach"]>[0]

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
   * Place every voice in the world (see {@link EmitterOptions}): the core
   * then owns its pan, so `pan` is not applied while an emitter is set.
   * Live via {@link Sound.setEmitter}.
   */
  emitter?: EmitterOptions
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
  /** Place the voice in the world (see {@link SoundOptions.emitter}). */
  emitter?: EmitterOptions
}

/** A decoded sound with reactive lifecycle. */
export type Sound = {
  /** Start the clip. Overlaps or restarts per the `overlap` option. */
  play(): void
  /** Stop every voice started from this sound, fading first if asked. */
  stop(options?: SoundStopOptions): void
  /** Hold every live voice where it is, until resume(). */
  pause(): void
  /** Continue every paused voice. */
  resume(): void
  /** Set the volume of every live voice, and of voices started later. */
  setGain(gain: number, options?: SoundRampOptions): void
  /**
   * Set the stereo position of every live voice, and of voices started
   * later. Not applied while an emitter is set (the core owns the pan).
   */
  setPan(pan: number, options?: SoundRampOptions): void
  /**
   * Place every live voice in the world, and voices started later (see
   * {@link EmitterOptions}); null releases the placement (live voices keep
   * their last pan, gain and rate layers ramp back to 1). Attaching again
   * updates; the same node keeps its velocity history.
   */
  setEmitter(emitter: EmitterOptions | null): void
  /** Set the playback rate of every live voice, and of voices started later. */
  setRate(rate: number, options?: SoundRampOptions): void
  /**
   * Set whether voices repeat: every live voice (a looping one turned off
   * finishes its pass and ends), and voices started later.
   */
  setLoop(loop: boolean): void
  /**
   * Where the most recently started voice is in the clip, in seconds (a
   * single-voice sound's position); undefined before the first play().
   */
  position(): number | undefined
  /** The clip's length in seconds; undefined while not loaded or unknown. */
  duration(): number | undefined
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
  initial: {
    loop?: boolean
    gain?: number
    pan?: number
    rate?: number
    fadeInMs?: number
    bus?: string
    emitter?: EmitterOptions
  },
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
  let emitter: EmitterOptions | null = initial.emitter ?? null
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
      // The emitter owns the pan: a stored pan waits for the emitter's end.
      voices.push(
        clip.play({ loop, gain, pan: emitter === null ? pan : undefined, rate, fadeInMs, bus, emitter: emitter ?? undefined }),
      )
      setPlaying(true)
    },
    stop: stopAll,
    pause() {
      prune()
      for (let v of voices) v.pause()
    },
    resume() {
      prune()
      for (let v of voices) v.resume()
    },
    setGain(value, options) {
      gain = value
      prune()
      for (let v of voices) v.setGain(value, options)
    },
    setPan(value, options) {
      pan = value
      if (emitter !== null) return
      prune()
      for (let v of voices) v.setPan(value, options)
    },
    setEmitter(value) {
      emitter = value
      prune()
      for (let v of voices) {
        if (value !== null) v.attach(value)
        else v.detach()
      }
    },
    setRate(value, options) {
      rate = value
      prune()
      for (let v of voices) v.setRate(value, options)
    },
    setLoop(value) {
      loop = value
      prune()
      for (let v of voices) v.setLoop(value)
    },
    position: () => voices.at(-1)?.position(),
    duration: () => clip?.duration,
    playing,
    error,
  }
}

/** Options for {@link createSink}: the pushed samples' layout and bus. */
export type SinkOptions = {
  /** Sample rate of the pushed samples in Hz (any rate; the mixer resamples). */
  sampleRate: number
  /** Channel count, interleaved samples when 2. Defaults to 1 (mono). */
  channels?: 1 | 2
  /** Bus the sink plays on (see flux:audio buses). */
  bus?: string
}

/** A streaming PCM output with reactive lifecycle. */
export type Sink = {
  /** The sink's id, for a runtime module that pushes into it; undefined if opening failed. */
  id(): number | undefined
  /** Queue interleaved float samples after what is queued (a no-op if opening failed). */
  push(samples: Float32Array): void
  /** Milliseconds pushed and not yet heard: push while this is under your lookahead. */
  queuedMs(): number
  /** Milliseconds played since the sink opened. */
  positionMs(): number
  /** Hold the sink; the queue holds. */
  pause(): void
  /** Continue a paused sink. */
  resume(): void
  /** Drop everything queued. */
  clear(): void
  /** Scale the sink's volume (1.0 = unchanged), ramped if asked. */
  setGain(gain: number, options?: SoundRampOptions): void
  /** Set if opening failed (no audio device). */
  error(): Error | undefined
}

/**
 * Opens a streaming PCM output and owns its lifecycle: closed when the
 * reactive owner is disposed. Samples pushed as they are produced play
 * seamlessly, under the master gain and the sink's bus like any voice; a
 * runtime module can push into it from its own thread by `id()`. On a box
 * with no audio device `error()` is set and the methods are no-ops. For
 * imperative use, call createSink() from "flux:audio".
 */
export function createSink(options: SinkOptions): Sink {
  let [error, setError] = createSignal<Error | undefined>(undefined, { ownedWrite: true })
  let sink: FluxSink | undefined
  try {
    sink = openSink(options)
  } catch (e) {
    setError(e instanceof Error ? e : new Error(String(e)))
  }
  onCleanup(() => {
    sink?.close()
    sink = undefined
  })
  return {
    id: () => sink?.id,
    push: (samples) => sink?.push(samples),
    queuedMs: () => sink?.queuedMs() ?? 0,
    positionMs: () => sink?.positionMs() ?? 0,
    pause: () => sink?.pause(),
    resume: () => sink?.resume(),
    clear: () => sink?.clear(),
    setGain: (gain, rampOptions) => sink?.setGain(gain, rampOptions),
    error,
  }
}

/**
 * A listener for positional sounds (see {@link Listener}), destroyed when
 * the reactive owner is disposed; the emitters heard by it are released
 * with it. @solidrt/3d's scene and a 2d view make their own; this is for
 * placing sound against a node or a view by hand. Opens no audio device.
 * For imperative use, call createListener() from "flux:audio".
 */
export function createListener(source: ListenerSource, options?: ListenerOptions): Listener {
  let listener = openListener(source, options)
  onCleanup(() => listener.destroy())
  return listener
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
    emitter: options.emitter,
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
    emitter: options.emitter,
  })
}

/**
 * Streams a large track, decoding on demand instead of loading it into memory.
 * Single-voice: each play() restarts it. Pass a path (resolved like flux:fs,
 * relative to the process cwd) or a `file()` from flux:fs; a path is wrapped in
 * `file()` for you. The track is read from local storage (the app's assets in
 * a packed app), not over the network. Owns the stream's lifecycle: stopped
 * and released when the reactive owner is disposed. For imperative use, call
 * stream()/play() from "flux:audio".
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
    emitter: options.emitter,
  })
}
