// Sound playback (gui-enabled runtime only). The imperative primitive; `play`
// decodes and starts a clip in one call, while `load` decodes once so a clip can
// be replayed cheaply. Handles carry a `stop()` bound to just that voice, so the
// raw track id never leaves the runtime.

declare module "flux:audio" {
  /** Options for {@link play} and {@link LoadedSound.play}. */
  type PlayOptions = {
    /** Repeat the clip until stopped. Defaults to false. */
    loop?: boolean
    /** Volume scale, 1.0 leaves the clip unchanged. Defaults to 1.0. */
    gain?: number
  }

  /** A playing voice with controls bound to it. */
  type SoundHandle = {
    /** Stop this voice. A no-op if it already finished. */
    stop(): void
  }

  /** A decoded clip that can be replayed without re-decoding. */
  type LoadedSound = {
    /** Start a fresh overlapping voice for this clip. */
    play(options?: PlayOptions): SoundHandle
    /** Release the decoded clip. Voices already playing keep going. */
    unload(): void
  }

  /**
   * Decode and start an encoded audio clip (Ogg/Vorbis or WAV). Returns
   * immediately; the sound plays on the mixer's own thread. Use {@link load} to
   * replay a clip repeatedly without decoding it each time.
   */
  export function play(bytes: Uint8Array, options?: PlayOptions): SoundHandle
  /**
   * Decode an encoded clip (Ogg/Vorbis or WAV) once and keep it in memory so it
   * can be replayed cheaply. Call `unload()` on the result when done.
   */
  export function load(bytes: Uint8Array): LoadedSound
  /** Stop every playing sound. */
  export function stopAll(): void
}
