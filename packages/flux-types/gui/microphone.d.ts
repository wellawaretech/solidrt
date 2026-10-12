// Microphone capture (gui-enabled runtime only). The imperative primitive;
// @solidrt/core's createMicrophone wraps it with SolidJS reactivity. `open`
// resolves to a bound session object: its methods are bound, and its `id` is
// what a runtime module takes a reader of its own by.

declare module "flux:microphone" {
  /** A microphone device from {@link listMicrophones}. */
  type MicrophoneDevice = {
    /** Device id to pass as `open({ microphone })`. */
    id: number
    /** Human-readable device name. */
    name: string
  }

  /** Options for {@link open}. */
  type MicrophoneOpenOptions = {
    /** Device id from {@link MicrophoneDevice}. Omit to use the default. */
    microphone?: number
    /** Capture sample rate in Hz. Defaults to 16000. */
    sampleRate?: number
  }

  /** An opened microphone session with controls bound to it. */
  type MicrophoneSession = {
    /**
     * The session's id: what the app hands a runtime module that reads this
     * microphone on a thread of its own (`microphone_tap` in the runtime).
     */
    id: number
    /** The actual capture sample rate in Hz. */
    sampleRate: number
    /**
     * Drain the mono float samples captured since the last read. The session
     * keeps a 10 s window of audio, so there is no obligation to poll: a read
     * later than that gets the last 10 s and the older samples are lost (the
     * runtime logs it). A runtime module reading the session hears every
     * sample too; reads do not take from each other.
     */
    read(): Float32Array
    /** Release the device. Every reader of the session, a module's included, ends. */
    close(): void
  }

  /**
   * List the available microphone devices. Under a test that installed a
   * clip as the microphone (`app.microphone` of `@solidrt/test`) the list is
   * that one microphone, id 0.
   */
  export function listMicrophones(): MicrophoneDevice[]
  /**
   * Open a microphone. Opening is the permission request: on Android the
   * RECORD_AUDIO prompt comes up and the app keeps running behind it; the
   * other platforms show none. Resolves once capture runs. Rejects when the
   * user denies the permission ("microphone permission denied"), when the
   * device cannot be opened, and under a test when the sample rate differs
   * from the installed clip's.
   */
  export function open(options?: MicrophoneOpenOptions): Promise<MicrophoneSession>
}