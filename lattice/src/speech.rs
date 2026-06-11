//! Whisper speech recognition: utterance segmentation (Silero VAD) and
//! transcription (whisper.cpp via whisper-rs) on a dedicated worker thread.
//!
//! The owner feeds mono f32 16 kHz samples through `feed` (e.g. once per
//! frame from a microphone session) and polls `take_events`. The worker
//! keeps a rolling pre-roll while idle; when VAD detects speech it
//! accumulates the utterance, and once the tail goes silent (or the
//! utterance hits the length cap) it transcribes the whole utterance and
//! emits `Final`. Dropping the `Recognizer` disconnects the sample channel
//! and the worker exits.

use std::sync::mpsc;

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

pub struct RecognizerConfig {
  /// A ggml Whisper model (the file's bytes; callers fetch/read it themselves).
  pub model: Vec<u8>,
  /// A ggml Silero VAD model (the file's bytes).
  pub vad_model: Vec<u8>,
  /// Whisper language code (e.g. "en", "auto" to detect).
  pub language: String,
  /// Also transcribe utterances while they are still being spoken (`Interim`).
  pub interim: bool,
  /// Start armed: drop transcripts until an utterance contains this phrase,
  /// then emit `Wake` (plus the text following the phrase as a `Final`) and
  /// deliver normally. Matched on normalized words, so transcription
  /// punctuation and casing do not matter.
  pub wake_word: Option<String>,
  /// With a wake word: re-arm after each delivered `Final` (one command per
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
  if events_tx.send(RecognizerEvent::Ready).is_err() {
    return;
  }

  let mut audio: Vec<f32> = Vec::new();
  let mut queue: std::collections::VecDeque<f32> = std::collections::VecDeque::new();
  let mut speaking = false;
  let mut since_interim: usize = 0;
  let wake_words: Option<Vec<String>> = config
    .wake_word
    .as_deref()
    .map(|w| norm_words(w).into_iter().map(|(word, _)| word).collect())
    .filter(|w: &Vec<String>| !w.is_empty());
  let mut armed = wake_words.is_some();

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
        // Utterance still in progress: optionally transcribe a snapshot of it
        // (suppressed while armed; pre-wake speech must not leak).
        if config.interim && !armed && since_interim >= INTERIM_INTERVAL {
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
          // While armed, utterances only count if they contain the wake
          // phrase; the text after it (if any) becomes the first result.
          let deliver = match (&wake_words, armed) {
            (Some(wake), true) => match match_wake(text, wake) {
              Some(remainder) => {
                armed = false;
                if events_tx.send(RecognizerEvent::Wake).is_err() {
                  return;
                }
                remainder.to_string()
              }
              None => String::new(),
            },
            _ => text.to_string(),
          };
          if !deliver.is_empty() {
            if events_tx.send(RecognizerEvent::Final(deliver)).is_err() {
              return;
            }
            // One command per wake: go back to sleep until the next wake word.
            if config.single_utterance && wake_words.is_some() {
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

/// Lowercased alphanumeric words of `text`, each with the byte offset just
/// past the word in the original string.
fn norm_words(text: &str) -> Vec<(String, usize)> {
  let mut words = Vec::new();
  let mut current = String::new();
  let mut end = 0;
  for (i, c) in text.char_indices() {
    if c.is_alphanumeric() {
      current.extend(c.to_lowercase());
      end = i + c.len_utf8();
    } else if !current.is_empty() {
      words.push((std::mem::take(&mut current), end));
    }
  }
  if !current.is_empty() {
    words.push((current, end));
  }
  words
}

/// If `text` contains `wake` as a contiguous normalized word sequence, the
/// text following the match (possibly empty), with leading punctuation
/// trimmed; None when the wake phrase does not occur.
fn match_wake<'a>(text: &'a str, wake: &[String]) -> Option<&'a str> {
  let words = norm_words(text);
  if words.len() < wake.len() {
    return None;
  }
  for start in 0..=(words.len() - wake.len()) {
    if (0..wake.len()).all(|i| words[start + i].0 == wake[i]) {
      let end = words[start + wake.len() - 1].1;
      return Some(
        text[end..].trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ',' | '.' | '!' | '?' | ':' | ';')),
      );
    }
  }
  None
}

/// Load the Silero VAD context from model bytes. whisper-rs wraps no
/// buffer-loading VAD init (whisper.cpp's loader variant is unexposed), so
/// stage the small model through a temp file.
fn vad_from_bytes(bytes: &[u8]) -> Result<WhisperVadContext, String> {
  use std::sync::atomic::{AtomicU64, Ordering};
  static UNIQUE: AtomicU64 = AtomicU64::new(0);
  let path =
    std::env::temp_dir().join(format!("srt-vad-{}-{}.bin", std::process::id(), UNIQUE.fetch_add(1, Ordering::Relaxed)));
  std::fs::write(&path, bytes).map_err(|e| format!("staging to {}: {e}", path.display()))?;
  let vad = WhisperVadContext::new(&path.to_string_lossy(), WhisperVadContextParams::default());
  let _ = std::fs::remove_file(&path);
  vad.map_err(|e| e.to_string())
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
