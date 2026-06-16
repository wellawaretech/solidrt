// Speech recognition. A session captures the microphone, segments utterances
// by silence (Silero VAD) and transcribes each one with Whisper, delivering
// final transcripts through onResult. With wakeWord the session starts
// asleep behind an efficient wake word detector (livekit-wakeword) and only
// transcribes after the wake word. startRecognition resolves once the models
// are loaded and listening has begun; it rejects when loading fails.
// Models are passed as bytes so any source composes: flux:fs file(), fetch
// (incl. the dev-server file proxy), or a download cache layered on top.
// Requires a runtime built with speech support.

export type SpeechOptions = {
  /** A ggml Whisper model (file contents, e.g. ggml-tiny.en.bin). */
  model: Uint8Array
  /** A ggml Silero VAD model (file contents). */
  vadModel: Uint8Array
  /** Whisper language code; "auto" detects (multilingual models only). Default "en". */
  lang?: string
  /** Explicit microphone device id from listMicrophones(). */
  microphone?: number
  /**
   * Keep transcribing utterance after utterance. Default true. Set false to
   * stop after the first final result (with wakeWord: re-arm instead, one
   * result per wake). Inverse of the Web Speech API default (false there).
   */
  continuous?: boolean
  /** Also deliver snapshot transcripts (final: false) while an utterance is still being spoken. */
  interimResults?: boolean
  /**
   * Wake word: start asleep, fire onWake when it is heard, then transcribe
   * the speech that follows. How the wake word is specified depends on the
   * engine. The current engine detects with a trained classifier and takes
   * the model's bytes (livekit-wakeword ONNX, e.g. the pretrained "hey
   * livekit"; custom phrases are trained offline with its toolkit). Phrase
   * strings are reserved for engines that match text; passing them to this
   * engine rejects with an error.
   */
  wakeWord?: Uint8Array | string | string[]
  /** Detector confidence (0..1) that counts as a wake. Default 0.5. */
  wakeThreshold?: number
}

export type SpeechResult = {
  /** Transcript of the utterance (a snapshot of it when isFinal is false). */
  transcript: string
  /** True for the completed utterance, false for interim snapshots. */
  isFinal: boolean
}

export type SpeechSession = {
  /** Receive transcripts (replaces any previous callback). */
  onResult(callback: (result: SpeechResult) => void): void
  /** The user started speaking (replaces any previous callback). */
  onSpeechStart(callback: () => void): void
  /** The utterance ended; its final result follows once transcribed. */
  onSpeechEnd(callback: () => void): void
  /** The wake word was heard (wakeWordModel sessions only). */
  onWake(callback: () => void): void
  /** Release the microphone and discard any utterance in progress. */
  stop(): void
}

export async function startRecognition(options: SpeechOptions): Promise<SpeechSession> {
  let started = await speech.start(options)
  return {
    onResult: (callback: (result: SpeechResult) => void) => speech.setResultCallback(started.handle, callback),
    onSpeechStart: (callback: () => void) => speech.setSpeechStartCallback(started.handle, callback),
    onSpeechEnd: (callback: () => void) => speech.setSpeechEndCallback(started.handle, callback),
    onWake: (callback: () => void) => speech.setWakeCallback(started.handle, callback),
    stop: () => speech.stop(started.handle),
  }
}