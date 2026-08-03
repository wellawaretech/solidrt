// Sound playback (gui-enabled runtime only). The imperative primitive; `play`
// decodes and starts a clip in one call, while `load`/`loadPcm`/`stream` yield
// a Clip that starts cheap overlapping Playbacks. Handles carry controls bound
// to just that clip or playback, so raw ids never leave the runtime.

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
  }

  /** One playing instance of a clip, with live controls bound to it. */
  type Playback = {
    /** Stop this playback. A no-op if it already finished. */
    stop(): void
    /**
     * Change the volume while playing. A finite number >= 0; 1.0 is the clip's
     * own level. A no-op after the playback finished.
     */
    setGain(gain: number): void
    /**
     * Move the stereo position while playing (see {@link PlayOptions.pan}).
     * A no-op after the playback finished.
     */
    setPan(pan: number): void
    /** Whether playback finished, naturally or via {@link stop}. */
    ended(): boolean
  }

  /** A loaded clip that can be played without re-decoding. */
  type Clip = {
    /** Start a fresh overlapping playback of this clip. */
    play(options?: PlayOptions): Playback
    /** Release the clip. Playbacks already running keep going. */
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
  /** Stop every playing sound. */
  export function stop(): void
}
