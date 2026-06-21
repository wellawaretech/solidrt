// Microphone capture (gui-enabled runtime only). The imperative primitive;
// @solidrt/core's createMicrophone wraps it with SolidJS reactivity. `open`
// returns a bound session object, so the raw handle never leaves the runtime.

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
    /** The actual capture sample rate in Hz. */
    sampleRate: number
    /** Drain the mono float samples captured since the last read. */
    read(): Float32Array
    /** Release the device. */
    close(): void
  }

  /** List the available microphone devices. */
  export function listMicrophones(): MicrophoneDevice[]
  /**
   * Open a microphone. Synchronous: current platforms expose no audio permission
   * prompt.
   */
  export function open(options?: MicrophoneOpenOptions): MicrophoneSession
}