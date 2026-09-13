// Video playback (gui-enabled runtime only). The imperative primitive;
// @solidrt/core wraps it with SolidJS reactivity. There is no video element:
// a texture player's `texture` id is displayed with <texture>/<d-texture>,
// and a richer Video component composes in a higher layer. A plane player
// (Android) has no texture at all: the platform composites the decoded
// picture fullscreen beneath the UI, off the frame loop entirely, and it
// streams: its source may be a URL, read as it plays.

declare module "flux:video" {
  import type { FluxFile } from "flux:fs"
  import type { TextureId } from "flux:gpu"

  /**
   * What went wrong with a stream, for the app to key on:
   * - "network": the source could not be opened or stopped delivering (an
   *   unreachable or failing server, an unreadable file, a truncated body);
   * - "decode": the bytes are not a stream the decoders can follow;
   * - "unsupported": a stream, codec or feature this player does not play;
   * - "no-plane": no video plane (not on this platform, or one is open).
   * An app falling back from a plane to a texture keys on "no-plane" only:
   * a network error would fail the texture the same way.
   */
  export type VideoErrorKind = "network" | "decode" | "unsupported" | "no-plane"

  /** The Error `open` rejects with and `error()` returns. */
  export interface VideoError extends Error {
    kind: VideoErrorKind
  }

  /** Controls and facts shared by both kinds of player. */
  export type VideoTransport = {
    /** Frame width in pixels, at open. */
    width: number
    /** Frame height in pixels, at open. */
    height: number
    /**
     * Duration in seconds; undefined when the source has no known end (a
     * live stream, a container written to a pipe).
     */
    duration: number | undefined
    /** Whether the file has a playable audio track. */
    hasAudio: boolean
    /**
     * Whether `seek` does anything. False for a texture player, and for a
     * source that cannot be read at an offset (an HTTP response without
     * Range support); `seek` is then a no-op.
     */
    seekable: boolean
    /** Start or resume playback. */
    play(): void
    /** Pause playback (the current frame stays displayed). */
    pause(): void
    /**
     * Seek to a time in seconds. Lands on the frame at that time (decoding
     * from the preceding keyframe); while paused the target frame is shown.
     * A no-op when `seekable` is false.
     */
    seek(seconds: number): void
    /** Whether playback is running. */
    playing(): boolean
    /** Presentation time of the displayed frame, in seconds. */
    currentTime(): number
    /**
     * Whether the last frame has been displayed. A failure does not set
     * it: see `error`.
     */
    finished(): boolean
    /**
     * Whether playback is held for the source to catch up: playing, with
     * the picture and the sound paused until enough is read ahead. Never
     * true for a texture player.
     */
    buffering(): boolean
    /**
     * The failure that stopped playback, if one did: the reader could not
     * read on, or the decoder gave up. Playback stops where it was
     * (`playing()` goes false, the last frame stays up); a `seek` retries
     * the source. The same object `failed` resolves with.
     */
    error(): VideoError | undefined
    /**
     * Resolves with the failure that stops playback, once, or with
     * undefined when the player closes without one. What a reactive layer
     * watches instead of polling `error()`.
     */
    failed: Promise<VideoError | undefined>
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
   * paints, and the window's uncovered pixels show the video. Its audio
   * track plays on the default output, in sync with the picture.
   */
  export type VideoPlane = VideoTransport

  /**
   * A video source: a path resolved like file() paths (through the app's
   * assets in a packed app), an http: or https: URL, or a file() from
   * flux:fs (a packed asset is read out of the exe). Any other scheme
   * throws. A URL plays on a plane only, and is read by the runtime
   * directly: no disk cache, no dev-server proxying, so the device must
   * reach the host itself.
   */
  export type VideoSource = string | FluxFile

  export type VideoOpenOptions = {
    /** Where the picture goes: a texture (default) or the platform's video plane. */
    present?: "texture"
    /**
     * Aborting it abandons the open: the promise rejects with the signal's
     * reason and the source is closed. (A texture player opens at once;
     * the signal matters for a plane's streamed open.)
     */
    signal?: AbortSignal
  }

  export type VideoPlaneOptions = {
    present: "plane"
    /** How the picture fits the window: letterboxed (default) or cropped to fill. */
    fit?: "contain" | "cover"
    /** Aborting it abandons the open (see VideoOpenOptions). */
    signal?: AbortSignal
  }

  /**
   * Open a video (WebM with 8-bit 4:2:0 VP9 video, i.e. profile 0; Opus
   * audio plays, other audio tracks are ignored). Playback starts paused;
   * call `play()`. Rejects with a VideoError: "network" when the source
   * cannot be opened (or does not answer within the open timeout),
   * "unsupported" when its codec is not played or a URL is given to a
   * texture player, "no-plane" for `present: "plane"` on a platform without
   * a video plane (Android only) or while another plane is open or opening.
   * A plane open reads the header off the JS thread and resolves once the
   * plane exists; a texture player opens at once.
   *
   * Encode with `ffmpeg -c:v libvpx-vp9 -c:a libopus out.webm`; VP9 and
   * Opus are royalty-free, so both decoders ship on every platform.
   */
  export function open(source: VideoSource, options?: VideoOpenOptions): Promise<VideoPlayer>
  export function open(source: VideoSource, options: VideoPlaneOptions): Promise<VideoPlane>
}
