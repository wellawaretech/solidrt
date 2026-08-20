// Sound playback (gui-enabled runtime only: feature-detect with
// `Flux.capabilities.includes("audio")` before importing on a runtime that may
// lack it - a static import fails at module load there). The imperative
// primitive; `play` decodes and starts a clip in one call, while
// `load`/`loadPcm`/`stream` yield a Clip that starts cheap overlapping
// Playbacks. Handles carry controls bound to just that clip or playback, so
// raw ids never leave the runtime.

declare module "flux:audio" {
  /** Options for {@link play} and {@link Clip.play}. */
  type PlayOptions = {
    /** Repeat the clip until stopped. Defaults to false. */
    loop?: boolean
    /** Volume scale, 1.0 leaves the clip unchanged. Defaults to 1.0. */
    gain?: number
    /**
     * Stereo position in [-1, 1] (clamped), -1 = left, 0 = center, 1 = right,
     * equal-power law. Omitted means unspatialized: no stereo processing at
     * all, which for a mono clip is about 3 dB louder than `pan: 0`.
     */
    pan?: number
    /**
     * Playback rate: 1.0 plays as loaded, higher is faster and higher-pitched,
     * lower slower and deeper (a plain resample, no formant correction).
     * Clamped to [0.01, 100]. Defaults to 1.0.
     */
    rate?: number
    /**
     * Fade in from silence over this many milliseconds (sample-accurate).
     * Defaults to 0 (start at full level).
     */
    fadeInMs?: number
  }

  /** Options for the live setters ({@link Playback.setGain} and friends). */
  type RampOptions = {
    /**
     * Reach the new value by ramping over this many milliseconds instead of
     * jumping. Linear, stepped by the engine at control rate (about every
     * 10 ms), so a fade stays smooth regardless of the app's frame rate. A
     * later set on the same parameter takes over from the ramp's current
     * value; omitted (or 0) sets immediately and cancels any ramp in flight.
     */
    rampMs?: number
  }

  /** Options for {@link Playback.stop} and the module-level {@link stop}. */
  type StopOptions = {
    /**
     * Fade to silence over this many milliseconds before stopping
     * (sample-accurate) instead of cutting immediately. The playback keeps
     * playing while it fades; ended() turns true once the fade completes.
     */
    fadeOutMs?: number
  }

  /** One playing instance of a clip, with live controls bound to it. */
  type Playback = {
    /** Stop this playback. A no-op if it already finished. */
    stop(options?: StopOptions): void
    /**
     * Change the volume while playing. A finite number >= 0; 1.0 is the clip's
     * own level. A no-op after the playback finished.
     */
    setGain(gain: number, options?: RampOptions): void
    /**
     * Move the stereo position while playing (see {@link PlayOptions.pan}).
     * A ramped set on a never-panned playback sweeps from center. A no-op
     * after the playback finished.
     */
    setPan(pan: number, options?: RampOptions): void
    /**
     * Change the playback rate while playing (see {@link PlayOptions.rate}) -
     * a live rate sweep is how an engine revs or a doppler pass falls.
     * A no-op after the playback finished.
     */
    setRate(rate: number, options?: RampOptions): void
    /** Whether playback finished, naturally or via {@link stop}. */
    ended(): boolean
  }

  /** A loaded clip that can be played without re-decoding. */
  type Clip = {
    /**
     * Start a fresh overlapping playback of this clip. Throws once 256
     * playbacks are live at once - a guard that turns a runaway play() loop
     * into an error instead of a saturated mixer.
     */
    play(options?: PlayOptions): Playback
    /**
     * Release the clip. Playbacks already running keep going; `play()` after
     * unloading throws.
     */
    unload(): void
  }

  /** Options for {@link loadPcm}. */
  type PcmOptions = {
    /** Channel count, interleaved samples when 2. Defaults to 1 (mono). */
    channels?: 1 | 2
  }

  /**
   * Decode and start an encoded audio clip (Ogg/Vorbis or WAV). Returns
   * immediately; the sound plays on the mixer's own thread. Use {@link load} to
   * replay a clip repeatedly without decoding it each time.
   */
  export function play(bytes: Uint8Array, options?: PlayOptions): Playback
  /**
   * Decode an encoded clip (Ogg/Vorbis or WAV) once and keep it in memory so it
   * can be replayed cheaply. Call `unload()` on the result when done.
   */
  export function load(bytes: Uint8Array): Clip
  /**
   * Load raw PCM samples as a clip; no decoding, no container. The typed array
   * is the sample format: Uint8Array = unsigned 8-bit, Int16Array = signed
   * 16-bit, Float32Array = 32-bit float, all as laid out in memory. Samples are
   * interleaved when `channels` is 2. Call `unload()` on the result when done.
   */
  export function loadPcm(
    data: Uint8Array | Int16Array | Float32Array,
    sampleRate: number,
    options?: PcmOptions,
  ): Clip
  /**
   * Open a clip for streaming: it is decoded on demand instead of loaded fully
   * into memory, so a large track needs little RAM. Takes a `file()` from
   * `flux:fs` (not a path), so the source rides the `file()` proxy override - a
   * dev-server-proxied file streams from the server. Play the result as a single
   * playback; do not overlap a stream with itself. Call `unload()` when done.
   */
  export function stream(source: ReturnType<typeof import("flux:fs").file>): Clip
  /** Stop every playing sound, fading the whole mix out first if asked. */
  export function stop(options?: StopOptions): void
  /**
   * Scale the whole mix: every playing and future flux:audio playback, on top
   * of per-playback gains (1.0 = unchanged, 0 = silence). A finite number
   * >= 0; ramps like the per-playback setters. Resets to 1.0 when the app
   * reloads. Does not affect `flux:video` audio, which has its own volume
   * control.
   */
  export function setMasterGain(gain: number, options?: RampOptions): void
}
