//! Sound playback via SDL_mixer.
//!
//! `play_audio` decodes an encoded clip (Ogg/Vorbis or WAV) and starts it on a
//! fresh mixer track, returning an id the caller can later `stop`. Tracks are
//! retained in the registry so a sound keeps playing after the call returns and
//! can be stopped individually; finished tracks are swept on the next play so a
//! stream of fire-and-forget sounds does not accumulate.
//!
//! There is no per-frame pump: the SDL_mixer device runs its own audio thread.
//! Everything here is touched only from the UI/JS thread, like the microphone.

use std::cell::RefCell;
use std::collections::HashMap;

use sdl3::iostream::IOStream;
use sdl3::mixer::{Mixer, Track};

use crate::sdl_utils;

#[derive(Default)]
pub struct AudioRegistry {
  // The mixer (and its MIX_Init guard) are opened once and leaked to `'static`
  // so tracks, which borrow the mixer, can be stored here. The device stays
  // open for the process lifetime; leaking is a one-time cost guarded by the
  // `Some` check in `mixer`.
  mixer: RefCell<Option<&'static Mixer>>,
  tracks: RefCell<HashMap<u64, Track<'static>>>,
  next_id: RefCell<u64>,
}

impl AudioRegistry {
  /// Lazily open the playback device, returning the shared `'static` mixer.
  fn mixer(&self) -> Result<&'static Mixer, String> {
    if let Some(mixer) = *self.mixer.borrow() {
      return Ok(mixer);
    }
    if !sdl_utils::audio_subsystem_init() {
      return Err(format!("audio subsystem init failed: {}", sdl_utils::sdl_error()));
    }
    // Forget the MIX_Init guard so the library stays initialized for the process
    // lifetime; its Drop would call MIX_Quit and tear the mixer down.
    let ctx = sdl3::mixer::init().map_err(|e| format!("mixer init failed: {e}"))?;
    std::mem::forget(ctx);
    let mixer = Mixer::open_device(None).map_err(|e| format!("failed to open audio device: {e}"))?;
    let mixer: &'static Mixer = Box::leak(Box::new(mixer));
    *self.mixer.borrow_mut() = Some(mixer);
    Ok(mixer)
  }

  /// Drop tracks that have finished playing so the map does not grow with each
  /// fire-and-forget sound. A track that is playing or paused is kept.
  fn sweep_finished(&self) {
    self.tracks.borrow_mut().retain(|_, t| t.is_playing() || t.is_paused());
  }
}

impl crate::context::Context {
  /// Decode and start an encoded audio clip (Ogg/Vorbis or WAV). `looping`
  /// repeats it until stopped; `gain` scales volume (1.0 = unchanged). Returns
  /// the track id, usable with `stop_audio`.
  pub fn play_audio(&self, bytes: &[u8], looping: bool, gain: f32) -> Result<u64, String> {
    self.audio.sweep_finished();
    let mixer = self.audio.mixer()?;
    // predecode=true fully decodes into the Audio at load time, so neither the
    // IOStream nor `bytes` needs to outlive this call.
    let io = IOStream::from_bytes(bytes).map_err(|e| format!("audio read failed: {e}"))?;
    let audio = mixer.load_audio_io(&io, true).map_err(|e| format!("audio decode failed: {e}"))?;
    let track = mixer.create_track().map_err(|e| format!("failed to create audio track: {e}"))?;
    track.set_audio(&audio).map_err(|e| format!("failed to assign audio: {e}"))?;
    if looping {
      track.set_loops(-1).map_err(|e| format!("failed to set loop: {e}"))?;
    }
    track.set_gain(gain).map_err(|e| format!("failed to set gain: {e}"))?;
    track.play().map_err(|e| format!("failed to play audio: {e}"))?;
    // The mixer ref-counts audio assigned to a track, so dropping `audio` here
    // is safe: the track keeps the decoded data alive until it stops.
    let id = {
      let mut next = self.audio.next_id.borrow_mut();
      *next += 1;
      *next
    };
    self.audio.tracks.borrow_mut().insert(id, track);
    Ok(id)
  }

  /// Stop and release a single track. A no-op if it already finished.
  pub fn stop_audio(&self, id: u64) {
    // Dropping the Track destroys it (MIX_DestroyTrack), which stops playback.
    self.audio.tracks.borrow_mut().remove(&id);
  }

  /// Stop and release every playing track.
  pub fn stop_all_audio(&self) {
    self.audio.tracks.borrow_mut().clear();
  }

  /// Release every track. Called between engine runs so a reloaded app never
  /// inherits (or leaks) a sound left playing. The device itself stays open.
  pub fn close_all_audio(&self) {
    self.audio.tracks.borrow_mut().clear();
  }
}
