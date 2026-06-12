//! Speech recognition: utterance segmentation (Silero VAD), transcription
//! (whisper.cpp via whisper-rs) and wake word detection (livekit-wakeword)
//! on a dedicated worker thread.
//!
//! The owner feeds mono f32 16 kHz samples through `feed` (e.g. once per
//! frame from a microphone session) and polls `take_events`. The worker
//! keeps a rolling pre-roll while idle; when VAD detects speech it
//! accumulates the utterance, and once the tail goes silent (or the
//! utterance hits the length cap) it transcribes the whole utterance and
//! emits `Final`. With a wake model the worker starts armed: only the wake
//! detector runs (no VAD/Whisper) over a rolling window until a classifier
//! score crosses the threshold, which emits `Wake` and switches to the
//! recognition flow above. Dropping the `Recognizer` disconnects the sample
//! channel and the worker exits.

use std::sync::mpsc;

use livekit_wakeword::WakeWordModel;
use whisper_rs::{
  FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState, WhisperVadContext,
  WhisperVadContextParams, WhisperVadParams,
};

/// The recognizer consumes mono f32 at this rate (Whisper's native rate).
pub const SAMPLE_RATE: u32 = 16_000;

// Segmentation tuning, in samples at 16 kHz.
/// Run a VAD check at most once per this much new audio (100 ms).
const CHECK_INTERVAL: usize = 1_600;
/// Rolling buffer kept while idle so an utterance includes its onset (1 s).
const PRE_ROLL: usize = 16_000;
/// Minimum buffered audio before VAD checks make sense (0.5 s).
const MIN_CHECK: usize = 8_000;
/// Window inspected for trailing silence while speaking (1.5 s).
const TAIL_WINDOW: usize = 24_000;
/// Trailing silence that ends an utterance (700 ms).
const END_SILENCE: usize = 11_200;
/// Silence kept after the last speech when trimming an utterance (200 ms).
const KEEP_PAD: usize = 3_200;
/// Force a cut when an utterance reaches this length (30 s).
const MAX_UTTERANCE: usize = 16_000 * 30;
/// With interim results on, re-transcribe the in-flight utterance at most
/// once per this much new audio (1 s).
const INTERIM_INTERVAL: usize = 16_000;
/// Rolling window handed to the wake word detector each check. The detector
/// is stateless and needs ~1.96 s to produce its full embedding sequence
/// (shorter windows silently score zero), so stay safely above that (2.2 s).
const WAKE_WINDOW: usize = 35_200;

pub struct RecognizerConfig {
  /// A ggml Whisper model (the file's bytes; callers fetch/read it themselves).
  pub model: Vec<u8>,
  /// A ggml Silero VAD model (the file's bytes).
  pub vad_model: Vec<u8>,
  /// Whisper language code (e.g. "en", "auto" to detect).
  pub language: String,
  /// Also transcribe utterances while they are still being spoken (`Interim`).
  pub interim: bool,
  /// An ONNX wake word classifier (livekit-wakeword; the file's bytes).
  /// Start armed: run only the detector (no VAD/Whisper) and discard audio
  /// until it fires, then emit `Wake` and deliver normally.
  pub wake_model: Option<Vec<u8>>,
  /// Classifier confidence (0..1) at which the wake word counts as detected.
  pub wake_threshold: f32,
  /// Samples arrive at real time from a live source (a microphone). While
  /// armed, the worker then drops audio older than the detection window to
  /// catch up after CPU starvation: a load spike costs that moment instead
  /// of time-shifting everything after it. Leave off for recorded/batch
  /// input, where every window must be scored.
  pub realtime: bool,
  /// With a wake model: re-arm after each delivered `Final` (one command per
  /// wake). Without one this is the caller's concern (stop on first result).
  pub single_utterance: bool,
}

pub enum RecognizerEvent {
  /// Models loaded; recognition is live.
  Ready,
  /// Loading or inference failed; the worker has exited.
  Error(String),
  /// VAD detected the start of speech.
  SpeechStart,
  /// VAD detected the end of an utterance; its `Final` follows after
  /// transcription.
  SpeechEnd,
  /// An armed recognizer heard the wake word.
  Wake,
  /// Snapshot transcript of the utterance still being spoken (interim only).
  Interim(String),
  /// Transcript of a completed utterance.
  Final(String),
}

pub struct Recognizer {
  samples_tx: mpsc::Sender<Vec<f32>>,
  events_rx: mpsc::Receiver<RecognizerEvent>,
}

impl Recognizer {
  /// Spawn the worker and start loading the models; readiness (or failure)
  /// arrives as an event. Samples fed before `Ready` are buffered.
  pub fn start(config: RecognizerConfig) -> Self {
    let (samples_tx, samples_rx) = mpsc::channel::<Vec<f32>>();
    let (events_tx, events_rx) = mpsc::channel::<RecognizerEvent>();
    std::thread::Builder::new()
      .name("speech-recognizer".into())
      .spawn(move || worker(config, samples_rx, events_tx))
      .expect("spawn speech-recognizer thread");
    Recognizer { samples_tx, events_rx }
  }

  pub fn feed(&self, samples: Vec<f32>) {
    if !samples.is_empty() {
      let _ = self.samples_tx.send(samples);
    }
  }

  /// Drain the events emitted since the last call.
  pub fn take_events(&self) -> Vec<RecognizerEvent> {
    self.events_rx.try_iter().collect()
  }
}

fn worker(config: RecognizerConfig, samples_rx: mpsc::Receiver<Vec<f32>>, events_tx: mpsc::Sender<RecognizerEvent>) {
  // Route whisper.cpp/ggml stderr chatter into the log crate (idempotent).
  whisper_rs::install_logging_hooks();

  let fail = |msg: String| {
    log::warn!("[speech] {msg}");
    let _ = events_tx.send(RecognizerEvent::Error(msg));
  };
  let whisper = match WhisperContext::new_from_buffer_with_params(&config.model, WhisperContextParameters::default()) {
    Ok(ctx) => ctx,
    Err(e) => return fail(format!("failed to load model: {e}")),
  };
  let mut state = match whisper.create_state() {
    Ok(state) => state,
    Err(e) => return fail(format!("failed to create whisper state: {e}")),
  };
  let mut vad = match vad_from_bytes(&config.vad_model) {
    Ok(vad) => vad,
    Err(e) => return fail(format!("failed to load VAD model: {e}")),
  };
  let mut wake = match &config.wake_model {
    Some(bytes) => match wake_from_bytes(bytes) {
      Ok(wake) => Some(wake),
      Err(e) => return fail(format!("failed to load wake word model: {e}")),
    },
    None => None,
  };
  if events_tx.send(RecognizerEvent::Ready).is_err() {
    return;
  }

  let mut audio: Vec<f32> = Vec::new();
  let mut queue: std::collections::VecDeque<f32> = std::collections::VecDeque::new();
  let mut speaking = false;
  let mut since_interim: usize = 0;
  let mut armed = wake.is_some();

  // Ingest exactly CHECK_INTERVAL samples per iteration so segmentation works
  // the same whether audio arrives in real time or all at once; the blocking
  // recv paces the loop and its disconnect ends the worker.
  loop {
    while queue.len() < CHECK_INTERVAL {
      match samples_rx.recv() {
        Ok(chunk) => queue.extend(chunk),
        Err(_) => return,
      }
      while let Ok(more) = samples_rx.try_recv() {
        queue.extend(more);
      }
    }
    audio.extend(queue.drain(..CHECK_INTERVAL));

    // Armed: only the wake detector runs. It is stateless, so each check
    // rescores the rolling window; the models are small CNNs, far cheaper
    // than Whisper. On detection the window is dropped (it must not retrigger
    // or leak into the utterance) and recognition picks up the speech that
    // follows.
    if armed {
      let detector = wake.as_mut().expect("armed without wake model");
      // Live sources: only the freshest window can fire a wake, so backlog
      // beyond it is history; drop it instead of scoring it. With a healthy
      // worker the queue holds under a frame of audio and this is the normal
      // window slide; after CPU starvation it erases the accumulated lag.
      if config.realtime {
        while let Ok(more) = samples_rx.try_recv() {
          queue.extend(more);
        }
        if audio.len() + queue.len() > WAKE_WINDOW {
          let dropped = audio.len() + queue.len() - WAKE_WINDOW;
          if dropped > SAMPLE_RATE as usize {
            log::debug!("[speech] armed catch-up: dropped {:.1}s of stale audio", dropped as f64 / SAMPLE_RATE as f64);
          }
          audio.extend(queue.drain(..));
          audio.drain(..audio.len() - WAKE_WINDOW);
        }
      }
      if audio.len() < WAKE_WINDOW {
        continue;
      }
      audio.drain(..audio.len() - WAKE_WINDOW);
      let pcm: Vec<i16> = audio.iter().map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect();
      match detector.predict(&pcm) {
        Ok(scores) => {
          if scores.values().any(|&score| score >= config.wake_threshold) {
            armed = false;
            audio.clear();
            if events_tx.send(RecognizerEvent::Wake).is_err() {
              return;
            }
          }
        }
        Err(e) => log::warn!("[speech] wake detection failed: {e}"),
      }
      continue;
    }

    if audio.len() < MIN_CHECK {
      continue;
    }

    if !speaking {
      if audio.len() > PRE_ROLL {
        audio.drain(..audio.len() - PRE_ROLL);
      }
      speaking = vad_segments(&mut vad, &audio).map_or(false, |segs| !segs.is_empty());
      if speaking {
        since_interim = 0;
        if events_tx.send(RecognizerEvent::SpeechStart).is_err() {
          return;
        }
      }
    } else {
      since_interim += CHECK_INTERVAL;
      let tail_start = audio.len().saturating_sub(TAIL_WINDOW);
      let trailing = match trailing_silence(&mut vad, &audio[tail_start..]) {
        Some(t) => t,
        None => continue,
      };
      let cap = audio.len() >= MAX_UTTERANCE;
      if trailing < END_SILENCE && !cap {
        // Utterance still in progress: optionally transcribe a snapshot of it.
        if config.interim && since_interim >= INTERIM_INTERVAL {
          since_interim = 0;
          match transcribe(&mut state, &audio, &config.language) {
            Ok(text) => {
              let text = text.trim().to_string();
              if !text.is_empty() && events_tx.send(RecognizerEvent::Interim(text)).is_err() {
                return;
              }
            }
            Err(e) => return fail(format!("transcription failed: {e}")),
          }
        }
        continue;
      }
      // SpeechEnd first, so consumers can show "processing" while the final
      // transcription runs.
      if events_tx.send(RecognizerEvent::SpeechEnd).is_err() {
        return;
      }
      if trailing > KEEP_PAD {
        audio.truncate(audio.len() - (trailing - KEEP_PAD));
      }
      match transcribe(&mut state, &audio, &config.language) {
        Ok(text) => {
          let text = text.trim();
          if !text.is_empty() {
            if events_tx.send(RecognizerEvent::Final(text.to_string())).is_err() {
              return;
            }
            // One command per wake: go back to sleep until the next wake word.
            if config.single_utterance && wake.is_some() {
              armed = true;
            }
          }
        }
        Err(e) => return fail(format!("transcription failed: {e}")),
      }
      audio.clear();
      speaking = false;
    }
  }
}

/// Stage model bytes through a temp file for loaders that only take paths,
/// then build with `load` and clean up.
fn via_temp_file<T>(bytes: &[u8], load: impl FnOnce(&std::path::Path) -> Result<T, String>) -> Result<T, String> {
  use std::sync::atomic::{AtomicU64, Ordering};
  static UNIQUE: AtomicU64 = AtomicU64::new(0);
  let path = std::env::temp_dir().join(format!(
    "srt-model-{}-{}.bin",
    std::process::id(),
    UNIQUE.fetch_add(1, Ordering::Relaxed)
  ));
  std::fs::write(&path, bytes).map_err(|e| format!("staging to {}: {e}", path.display()))?;
  let result = load(&path);
  let _ = std::fs::remove_file(&path);
  result
}

/// Load the Silero VAD context from model bytes (whisper-rs wraps no
/// buffer-loading VAD init; whisper.cpp's loader variant is unexposed).
fn vad_from_bytes(bytes: &[u8]) -> Result<WhisperVadContext, String> {
  via_temp_file(bytes, |path| {
    WhisperVadContext::new(&path.to_string_lossy(), WhisperVadContextParams::default()).map_err(|e| e.to_string())
  })
}

/// Load a wake word classifier from ONNX model bytes (livekit-wakeword only
/// loads classifiers from paths; its mel/embedding models are built in).
fn wake_from_bytes(bytes: &[u8]) -> Result<WakeWordModel, String> {
  via_temp_file(bytes, |path| WakeWordModel::new(&[path], SAMPLE_RATE).map_err(|e| e.to_string()))
}

/// Speech segments in `samples` (timestamps in centiseconds), or None when
/// the VAD call fails (logged; the caller just retries on the next check).
fn vad_segments(vad: &mut WhisperVadContext, samples: &[f32]) -> Option<Vec<whisper_rs::WhisperVadSegment>> {
  match vad.segments_from_samples(WhisperVadParams::default(), samples) {
    Ok(segments) => Some(segments.collect()),
    Err(e) => {
      log::warn!("[speech] VAD failed: {e}");
      None
    }
  }
}

/// Samples of silence at the end of `tail` according to VAD.
fn trailing_silence(vad: &mut WhisperVadContext, tail: &[f32]) -> Option<usize> {
  let segments = vad_segments(vad, tail)?;
  // Centiseconds at 16 kHz: 160 samples per unit.
  let last_end = segments.iter().fold(0usize, |acc, s| acc.max((s.end * 160.0) as usize));
  Some(tail.len().saturating_sub(last_end))
}

fn transcribe(state: &mut WhisperState, audio: &[f32], language: &str) -> Result<String, whisper_rs::WhisperError> {
  let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
  params.set_language(Some(language));
  params.set_print_special(false);
  params.set_print_progress(false);
  params.set_print_realtime(false);
  params.set_print_timestamps(false);
  state.full(params, audio)?;
  let mut text = String::new();
  for i in 0..state.full_n_segments() {
    if let Some(segment) = state.get_segment(i) {
      text.push_str(segment.to_str()?);
    }
  }
  Ok(text)
}
