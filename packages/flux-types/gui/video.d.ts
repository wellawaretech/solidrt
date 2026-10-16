// Video playback and recording (gui-enabled runtime only). The imperative primitive;
// @solidrt/core wraps it with SolidJS reactivity. There is no video element:
// a texture player's `texture` id is displayed with <texture>/<d-texture>,
// and a richer Video component composes in a higher layer. A plane player
// (Android) has no texture at all: the platform composites the decoded
// picture fullscreen beneath the UI. Both players run off the frame loop
// (demux, decode, the clock, frame selection and audio on their own
// threads; a texture player's frames are shown at the UI's cadence, the
// way a browser's compositor shows a video's) and both stream: the source
// may be a URL, read as it plays.

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
     * Whether `seek` does anything. False for a source that cannot be read
     * at an offset (an HTTP response without Range support); `seek` is
     * then a no-op.
     */
    seekable: boolean
    /**
     * Scale the audio track's volume (1.0 = unchanged, 0 = silent), ramped
     * over `rampMs` like a flux:audio playback's gain. The track also obeys
     * flux:audio's master gain and, when opened with `bus`, that bus's gain
     * and pause. A no-op without an audio track.
     */
    setGain(gain: number, options?: { rampMs?: number }): void
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
    /**
     * Presentation time of the displayed frame, in seconds: for a texture
     * player the frame on screen (the one the compositor last latched),
     * for a plane the frame last released to the surface.
     */
    currentTime(): number
    /**
     * Whether the last frame has been displayed. A failure does not set
     * it: see `error`.
     */
    finished(): boolean
    /**
     * Whether playback is held for the source to catch up: playing, with
     * the picture and the sound paused until enough is read ahead.
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

  /**
   * A texture player: frames are decoded off the frame loop and shown in
   * a GPU texture at the UI's cadence, each at the frame closest to its
   * time.
   */
  export type VideoPlayer = VideoTransport & {
    /**
     * GPU texture id decoded frames land in (use as a texture source).
     * Holds the current frame; black until playback starts.
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
   * throws. A URL is read by the runtime directly: no disk cache, no
   * dev-server proxying, so the device must reach the host itself.
   */
  export type VideoSource = string | FluxFile

  export type VideoOpenOptions = {
    /** Where the picture goes: a texture (default) or the platform's video plane. */
    present?: "texture"
    /**
     * The flux:audio bus the audio track plays on (a plain name created by
     * use), so `setBusGain`, `pause({ bus })` and `stop({ bus })` reach it.
     * Omitted, the track plays under the master gain alone.
     */
    bus?: string
    /**
     * Aborting it abandons the open: the promise rejects with the signal's
     * reason and the source is closed.
     */
    signal?: AbortSignal
  }

  export type VideoPlaneOptions = {
    present: "plane"
    /** How the picture fits the window: letterboxed (default) or cropped to fill. */
    fit?: "contain" | "cover"
    /** The flux:audio bus the audio track plays on (see VideoOpenOptions). */
    bus?: string
    /** Aborting it abandons the open (see VideoOpenOptions). */
    signal?: AbortSignal
  }

  /**
   * Open a video (WebM with 8-bit 4:2:0 VP9 video, i.e. profile 0; Opus
   * audio plays, other audio tracks are ignored). Playback starts paused;
   * call `play()`. Rejects with a VideoError: "network" when the source
   * cannot be opened (or does not answer within the open timeout),
   * "unsupported" when its codec is not played, "no-plane" for
   * `present: "plane"` on a platform without a video plane (Android only)
   * or while another plane is open or opening. The header is read off the
   * JS thread; the promise resolves once the player (and, for a plane, the
   * plane) exists.
   *
   * Encode with `ffmpeg -c:v libvpx-vp9 -c:a libopus out.webm`; VP9 and
   * Opus are royalty-free, so both decoders ship on every platform.
   */
  export function open(source: VideoSource, options?: VideoOpenOptions): Promise<VideoPlayer>
  export function open(source: VideoSource, options: VideoPlaneOptions): Promise<VideoPlane>

  /**
   * What a recording takes its picture and sound from. The picture is a
   * camera session (anything carrying its texture id: a `CameraSession`
   * of flux:camera, or `{ texture: cam.texture()! }` from core's
   * createCamera), recorded at its open size, or a render tree node by
   * id (`{ node: ref.id }`, the id `captureSnapshot` takes): the window
   * node records the whole window as it is on screen (window shader
   * applied), any other node records its painted box cut out of the
   * window, overlapping content included, following the box when it
   * moves and recording nothing while it has none (hidden). The sound is
   * the microphone session (anything carrying its id), at a rate Opus
   * takes (8, 12, 16, 24 or 48 kHz; the default 16 kHz is). At least one.
   */
  export type RecordSources = {
    video?: { texture: TextureId } | { node: number }
    audio?: { id: number }
  }

  export type RecordOptions = {
    /** The WebM file the clip is written to (created, or truncated). */
    to: FluxFile
    /**
     * The encoded picture size in pixels. A node source scales into it
     * (one dimension given: the other follows the source's aspect; none:
     * the source's own pixel size, so the window's at the display scale);
     * the size is fixed for the clip and stays even. A camera records at
     * its open size: a different one rejects.
     */
    width?: number
    height?: number
    /**
     * Video bitrate in bits per second. Default: a budget from the frame
     * size and rate (about 0.9 Mbit/s for 640x480 at 30 fps, 2.8 for
     * 720p30).
     */
    videoBitrate?: number
    /** Audio bitrate in bits per second. Default 32000, transparent for mono voice. */
    audioBitrate?: number
    /**
     * The nominal frame rate the encoder budgets for (default 30). Frames
     * keep their own capture times: a camera slower than this yields fewer
     * frames, a camera faster than the encoder loses frames, and the clip
     * keeps real time either way.
     */
    frameRate?: number
  }

  /** A recording in progress, bound to its file. */
  export type Recording = {
    /** Whether frames and samples are being taken: false while paused, after `stop`, and after a failure. */
    recording(): boolean
    /** Seconds of media written so far. */
    duration(): number
    /** Take nothing until `resume`; the clip has no gap for the pause. */
    pause(): void
    resume(): void
    /**
     * End the recording: the encoders flush and the file is finished (its
     * sizes and seek index written), off the JS thread. Resolves once the
     * file is final; rejects with the VideoError that stopped the
     * recording early, if one did. A second call resolves at once.
     */
    stop(): Promise<void>
    /**
     * The failure that stopped the recording, if one did: the file could
     * not be written ("network"), or the encoder gave up ("decode").
     * Nothing more is taken after it; `stop` still finishes the file with
     * what was written.
     */
    error(): VideoError | undefined
  }

  /**
   * Record the camera or the window, and/or the microphone, to a WebM
   * file (VP9 and Opus, what `open` plays): nothing crosses into JS, the
   * taps feed the encoders on their own threads; a window source is read
   * back on the raster thread after each drawn frame, asynchronously, and
   * a window at rest adds no frames (the clip holds the last one).
   * Resolves once the encoders are up and the file is open. Rejects with a
   * VideoError: "unsupported" when a source is not open (no camera streams
   * into that texture, the microphone is closed, the node is not in the
   * tree or has no box), when the microphone's rate is not one Opus takes,
   * or when this build has no encoder (`Flux.capabilities` lists
   * "video-encode" where it does); "network" when the file cannot be
   * created. A malformed argument throws.
   */
  export function record(sources: RecordSources, options: RecordOptions): Promise<Recording>
}
