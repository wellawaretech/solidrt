// Sound playback (gui-enabled runtime only). The imperative primitive; `play`
// decodes an encoded clip (Ogg/Vorbis or WAV) and returns a handle whose
// `stop()` halts just that sound, so the raw track id never leaves the runtime.

declare module "flux:audio" {
  /** Options for {@link play}. */
  type PlayOptions = {
    /** Repeat the clip until stopped. Defaults to false. */
    loop?: boolean
    /** Volume scale, 1.0 leaves the clip unchanged. Defaults to 1.0. */
    gain?: number
  }

  /** A playing sound with controls bound to it. */
  type SoundHandle = {
    /** Stop this sound. A no-op if it already finished. */
    stop(): void
  }

  /**
   * Decode and start an encoded audio clip (Ogg/Vorbis or WAV). Returns
   * immediately; the sound plays on the mixer's own thread.
   */
  export function play(bytes: Uint8Array, options?: PlayOptions): SoundHandle
  /** Stop every playing sound. */
  export function stopAll(): void
}
