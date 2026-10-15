// Sound playback (gui-enabled runtime only: feature-detect with
// `Flux.capabilities.includes("audio")` before importing on a runtime that may
// lack it - a static import fails at module load there). The imperative
// primitive; `play` decodes and starts a clip in one call, while
// `load`/`loadPcm`/`stream` yield a Clip that starts cheap overlapping
// Playbacks, and `createSink` takes samples as an app produces them. Handles
// carry controls bound to just that clip, playback or sink, so raw ids never
// leave the runtime.
//
// Clips are WAV (PCM or float) or Opus, in an Ogg file (.opus) or a WebM
// (.webm/.weba, the sound of a video file included). Anything else is
// refused by name; encode it as Opus.
//
// One mixer plays everything: clips, streams, sinks and video audio, each
// voice under its own gain, its bus's gain and the master gain, every gain
// rampable sample-accurately.

declare module "flux:audio" {
  /** Options for {@link play} and {@link Clip.play}. */
  type PlayOptions = {
    /** Repeat the clip until stopped (see {@link Playback.setLoop}). Defaults to false. */
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
    /**
     * Name of the bus this playback belongs to. Buses are plain names created
     * by use (no setup call): a bus has a gain ({@link setBusGain}), can be
     * paused ({@link pause}) and stopped ({@link stop}) as a group.
     */
    bus?: string
  }

  /** Options for the live setters ({@link Playback.setGain} and friends). */
  type RampOptions = {
    /**
     * Reach the new value by ramping over this many milliseconds instead of
     * jumping. Linear and sample-accurate for gains (the playback rate steps
     * once per mix block, about every 10 ms), so a fade stays smooth whatever
     * the app's frame rate. A later set on the same parameter takes over
     * from the ramp's current value; omitted (or 0) sets immediately and
     * cancels any ramp in flight.
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

  /** Options for the module-level {@link stop}. */
  type StopAllOptions = StopOptions & {
    /**
     * Stop only the playbacks on this bus (see {@link PlayOptions.bus})
     * instead of everything.
     */
    bus?: string
  }

  /** Options for the module-level {@link pause} and {@link resume}. */
  type BusOptions = {
    /**
     * Pause or resume only this bus (see {@link PlayOptions.bus}) instead of
     * every playback. A paused bus holds its playbacks, live and future, until
     * resumed.
     */
    bus?: string
  }

  /** One playing instance of a clip, with live controls bound to it. */
  type Playback = {
    /**
     * Stop this playback. A no-op if it already finished. Stopping is not
     * pausing: a stopped playback is gone for good, including a looping one;
     * to silence it for a while, {@link pause} it or ramp its gain to 0.
     */
    stop(options?: StopOptions): void
    /**
     * Hold the playback where it is: it renders nothing and {@link position}
     * stands until {@link resume}. A no-op after the playback finished.
     */
    pause(): void
    /** Continue a paused playback from where it stood. */
    resume(): void
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
    /**
     * Change whether the playback repeats. Turned off on a looping playback,
     * it finishes the pass it is in and ends: "finish this loop then stop".
     * A no-op after the playback finished.
     */
    setLoop(loop: boolean): void
    /**
     * Where the playback is in its clip, in seconds (wrapping on a looping
     * clip), as of the last mix block (about 10 ms). After the playback
     * finished, where it stopped.
     */
    position(): number
    /**
     * Whether playback finished, naturally or via {@link stop}. It turns
     * true once the last of the clip has been rendered into the device's
     * buffer, which runs ahead of the speaker by that buffer (tens of ms;
     * more on some TVs); {@link position} is net of it.
     */
    ended(): boolean
  }

  /** A loaded clip that can be played without re-decoding. */
  type Clip = {
    /**
     * The clip's length in seconds; undefined for a stream whose container
     * does not say.
     */
    duration: number | undefined
    /**
     * Start a fresh overlapping playback of this clip (a streamed clip plays
     * one at a time: this restarts it). Throws once 256 playbacks are live at
     * once - a guard that turns a runaway play() loop into an error instead
     * of a saturated mixer.
     */
    play(options?: PlayOptions): Playback
    /**
     * Release the clip. Playbacks already running keep going (a stream plays
     * out what it has decoded ahead and ends); `play()` after unloading
     * throws.
     */
    unload(): void
  }

  /** Options for {@link loadPcm}. */
  type PcmOptions = {
    /** Channel count, interleaved samples when 2. Defaults to 1 (mono). */
    channels?: 1 | 2
  }

  /** Options for {@link createSink}. */
  type SinkOptions = {
    /** Sample rate of the pushed samples in Hz (any rate; the mixer resamples). */
    sampleRate: number
    /** Channel count, interleaved samples when 2. Defaults to 1 (mono). */
    channels?: 1 | 2
    /** Bus the sink plays on (see {@link PlayOptions.bus}). */
    bus?: string
  }

  /**
   * A streaming PCM output: samples pushed as they are produced play
   * seamlessly, one after the other. For speech synthesis, live processing,
   * a synth - anything that cannot load a clip up front.
   */
  type Sink = {
    /**
     * The sink's id: what the app hands a runtime module that pushes into
     * this sink from a thread of its own (`pcm_sink_handle` in the runtime).
     * One pusher per sink, the app or the module.
     */
    id: number
    /**
     * Queue interleaved float samples (a whole number of frames) after what
     * is queued. Throws when the sink is closed or holds more than it can
     * take: push while {@link queuedMs} is under your lookahead (a few
     * hundred ms is plenty), not in a tight loop.
     */
    push(samples: Float32Array): void
    /**
     * Milliseconds pushed and not yet heard: the backpressure signal. 0 means
     * the next push is late (the sink plays silence meanwhile).
     */
    queuedMs(): number
    /**
     * Milliseconds of audio played since the sink opened (what {@link clear}
     * dropped counts as played), as of the last mix block.
     */
    positionMs(): number
    /** Hold the sink: the queue holds and {@link positionMs} stands. */
    pause(): void
    /** Continue a paused sink. */
    resume(): void
    /** Drop everything queued (and nothing pushed after). */
    clear(): void
    /** Scale the sink's volume (1.0 = unchanged), ramped like a playback's. */
    setGain(gain: number, options?: RampOptions): void
    /**
     * Close the sink: queued audio is dropped, not drained, and a module's
     * further pushes fail. Closing is a stop; fade the gain first to end
     * softly.
     */
    close(): void
  }

  /**
   * Decode and start an audio clip (WAV, or Opus in Ogg or WebM). Returns
   * immediately; the sound plays on the mixer's own thread. Use {@link load}
   * to replay a clip repeatedly without decoding it each time.
   */
  export function play(bytes: Uint8Array, options?: PlayOptions): Playback
  /**
   * Decode a clip (WAV, or Opus in Ogg or WebM) once and keep it in memory
   * so it can be replayed cheaply. Call `unload()` on the result when done.
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
   * Open a clip for streaming: it is decoded on demand, ahead of playback on
   * the mixer's decode thread, so a large track needs little RAM. Takes a
   * `file()` from `flux:fs` (not a path), read from local storage like any
   * `file()` (the app's assets in a packed app); there is no network
   * streaming. A stream plays one playback at a time: `play()` while it plays
   * restarts it from the beginning. Call `unload()` when done.
   */
  export function stream(source: ReturnType<typeof import("flux:fs").file>): Clip
  /**
   * Stop every playing sound - or just one bus with `{ bus }` - fading it
   * out first if asked. Stopping is not pausing: stopped playbacks (looping
   * ones included) cannot be restarted - to silence a group for a while,
   * {@link pause} it. Sinks are not stopped here; they are closed by their
   * owners.
   */
  export function stop(options?: StopAllOptions): void
  /**
   * Hold every playback - or just one bus with `{ bus }` - where it is, until
   * {@link resume}. A paused bus holds playbacks started on it later too.
   */
  export function pause(options?: BusOptions): void
  /** Continue every playback, or just one bus with `{ bus }`. */
  export function resume(options?: BusOptions): void
  /**
   * Scale the whole mix: every playing and future playback, sink and video
   * track, on top of per-playback and bus gains (1.0 = unchanged, 0 =
   * silence). A finite number >= 0; ramps like the per-playback setters.
   * Resets to 1.0 when the app reloads.
   */
  export function setMasterGain(gain: number, options?: RampOptions): void
  /**
   * Scale one bus (see {@link PlayOptions.bus}): a playback's audible level
   * is its own gain x its bus's gain x the master gain, each layer set
   * independently - none overwrites another. Applies to live and future
   * playbacks on the bus, defaults to 1.0, resets to 1.0 when the app
   * reloads, and ramps like the other gain setters.
   */
  export function setBusGain(bus: string, gain: number, options?: RampOptions): void
  /**
   * Open a streaming PCM output (see {@link Sink}). It plays as soon as
   * samples are pushed, under the master gain and its bus like any voice.
   * Call `close()` when done.
   */
  export function createSink(options: SinkOptions): Sink
  /**
   * The mixer's output sample rate in Hz. Synthesize PCM at this rate and
   * {@link loadPcm} or a {@link Sink} feeds the mix without a resample.
   * Opens the audio device on first use, like the load and play calls.
   */
  export function outputSampleRate(): number
}
