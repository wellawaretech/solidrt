// Video playback (gui-enabled runtime only). The imperative primitive;
// @solidrt/core wraps it with SolidJS reactivity. There is no video element:
// a texture player's `texture` id is displayed with <texture>/<d-texture>,
// and a richer Video component composes in a higher layer. A plane player
// (Android) has no texture at all: the platform composites the decoded
// picture fullscreen beneath the UI, off the frame loop entirely.

declare module "flux:video" {
  import type { TextureId } from "flux:gpu"

  /** Controls and facts shared by both kinds of player. */
  export type VideoTransport = {
    /** Frame width in pixels. */
    width: number
    /** Frame height in pixels. */
    height: number
    /** Duration in seconds; undefined when the source has none (a live stream). */
    duration: number | undefined
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
    /** Stop playback and release everything the player holds. */
    close(): void
  }

  /** A texture player: frames land in a GPU texture on the UI's clock. */
  export type VideoPlayer = VideoTransport & {
    /**
     * GPU texture id decoded frames are uploaded into (use as a texture
     * source). Holds the current frame; black until playback starts.
     */
    texture: TextureId
  }

  /**
   * A plane player: the decoder renders into the platform's own video
   * surface, fullscreen beneath the UI, on the video's clock. Exists from
   * open until close; one at a time. The UI draws over it wherever it
   * paints, and the window's uncovered pixels show the video.
   */
  export type VideoPlane = VideoTransport & {
    /**
     * Seek to a time in seconds. Lands on the frame at that time (decoding
     * from the preceding keyframe); while paused the target frame is shown.
     */
    seek(seconds: number): void
  }

  export type VideoOpenOptions = {
    /** Where the picture goes: a texture (default) or the platform's video plane. */
    present?: "texture"
  }

  export type VideoPlaneOptions = {
    present: "plane"
    /** How the picture fits the window: letterboxed (default) or cropped to fill. */
    fit?: "contain" | "cover"
  }

  /**
   * Open a video file (MP4 with 8-bit 4:2:0 VP9 video, i.e. profile 0;
   * AAC audio plays, other audio tracks are ignored). The path resolves
   * like file() paths (through the app's assets in a packed app). Playback
   * starts paused; call `play()`. Rejects when the file is unreadable or
   * its codec unsupported, and for `present: "plane"` on a platform without
   * a video plane (Android only) or while another plane is open.
   *
   * Encode with `ffmpeg -c:v libvpx-vp9 -c:a aac out.mp4`; VP9 is
   * royalty-free, so the decoder ships on every platform.
   */
  export function open(path: string, options?: VideoOpenOptions): Promise<VideoPlayer>
  export function open(path: string, options: VideoPlaneOptions): Promise<VideoPlane>
}
