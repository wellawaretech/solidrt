// Video playback (gui-enabled runtime only). The imperative primitive;
// @solidrt/core wraps it with SolidJS reactivity. There is no video element:
// the player's `texture` id is displayed with <texture>/<d-texture>, and a
// richer Video component composes in a higher layer.

declare module "flux:video" {
  import type { TextureId } from "flux:gpu"

  /** An opened video: the frame texture plus controls bound to it. */
  type VideoPlayer = {
    /**
     * GPU texture id decoded frames are uploaded into (use as a texture
     * source). Holds the current frame; black until playback starts.
     */
    texture: TextureId
    /** Frame width in pixels. */
    width: number
    /** Frame height in pixels. */
    height: number
    /** Duration in seconds. */
    duration: number
    /** Whether the file has a playable audio track. */
    hasAudio: boolean
    /** Start or resume playback. */
    play(): void
    /** Pause playback (the current frame stays displayed). */
    pause(): void
    /** Whether playback is running. */
    playing(): boolean
    /** Presentation time of the displayed frame, in seconds. */
    currentTime(): number
    /** Whether the last frame has been displayed. */
    finished(): boolean
    /** Stop playback and release the decoder, texture, and audio sink. */
    close(): void
  }

  /**
   * Open a video file (MP4 with H.264 video; AAC audio plays, other audio
   * tracks are ignored). The path resolves like file() paths (through the
   * app's assets in a packed app). Playback starts paused; call `play()`.
   * Rejects when the file is unreadable or its codec unsupported.
   *
   * The built-in software decoder does not decode B-frames; encode dev
   * content with `-bf 0` until the platform hardware decoders land.
   */
  export function open(path: string): Promise<VideoPlayer>
}
