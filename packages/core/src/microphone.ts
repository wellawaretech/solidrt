// Microphone capture. A session delivers raw mono float32 samples at the
// requested sample rate (the device format is converted by SDL); read()
// drains whatever was captured since the last call. Captured audio buffers
// until read, so poll read() regularly (e.g. once per frame) while open.

export type MicrophoneInfo = {
  id: number
  name: string
}

export type MicrophoneOptions = {
  /** Explicit device id from listMicrophones(); default is the system default recording device. */
  microphone?: number
  /** Sample rate of the delivered samples (the device rate is converted). Default 16000. */
  sampleRate?: number
}

export type Microphone = {
  /** Sample rate of read() samples. */
  sampleRate: number
  /** Drain the mono float32 samples captured since the last read. */
  read(): Float32Array
  /** Release the device. */
  close(): void
}

export function listMicrophones(): MicrophoneInfo[] {
  return microphone.listMicrophones()
}

// Async to leave room for an OS permission prompt on platforms that need one
// (the desktop backends open synchronously).
export async function openMicrophone(options: MicrophoneOptions = {}): Promise<Microphone> {
  let opened = microphone.open(options)
  return {
    sampleRate: opened.sampleRate,
    read: () => microphone.read(opened.handle),
    close: () => microphone.close(opened.handle),
  }
}