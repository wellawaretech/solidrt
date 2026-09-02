//! JS bindings for video playback: a thin marshaling layer over
//! forge::video (demux, decode, sync decisions) and alloy (YUV texture,
//! PCM sink). There is NO video primitive: `open()` resolves to a player
//! handle whose `texture` id is displayed with `<texture>`/`<d-texture>`,
//! and a richer Video component composes in a higher layer.
//!
//! `tick`, the per-frame hook driven by the FrameRendered handler (the
//! camera precedent), does the plumbing per player: feed the PCM sink up to
//! a lookahead, read the master clock (the sink position when the stream
//! has audio, an engine-timeline accumulator otherwise), ask the forge
//! player for the frame due, and upload it into the YUV texture.
//!
//! Behind `video-timeline-pacing` (default OFF, see the flux Cargo.toml and
//! okf/backlog/video-playback.md): silent streams clock on the engine
//! timeline (`timeline_now_ms`: the paced frame clock in a lattice run)
//! instead of wall time, frame selection gets a half-period lookahead, and a
//! mid-playback player reports standing frame demand. Wall time is the wrong
//! master clock because the tick's JS work executes at jittery wall moments
//! even when presents are metronomic - a wall read inside the tick inherits
//! that jitter and frame selection holds and double-steps (measured ~11% of
//! frames on a 50 Hz TV while presents were clean). The paced timeline
//! advances one period per frame whatever the execution jitter. The
//! lookahead keeps comparisons off the pts boundary that play() anchors the
//! grids in phase on; without it sub-ms timeline noise flips them. With the
//! feature off, behavior is the previous one: wall-clock selection, frame
//! demand only on upload.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use forge::video::{PixelLayout, VideoPlayer};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promise;
use rquickjs::{Ctx, Exception, Function, JsLifetime, Object};

use super::AlloyContext;

// The silent-stream master clock reading in us: the engine timeline behind
// `video-timeline-pacing`, a process-monotonic wall origin otherwise.
#[cfg(feature = "video-timeline-pacing")]
fn clock_now_us(ctx: &Ctx<'_>) -> i64 {
  (crate::standards_plugins::time::timeline_now_ms(ctx) * 1000.0) as i64
}

#[cfg(not(feature = "video-timeline-pacing"))]
fn clock_now_us(_ctx: &Ctx<'_>) -> i64 {
  use std::sync::OnceLock;
  use std::time::Instant;
  static ORIGIN: OnceLock<Instant> = OnceLock::new();
  ORIGIN.get_or_init(Instant::now).elapsed().as_micros() as i64
}

// How much decoded audio the sink holds ahead of the device. Small enough
// to keep pause latency low (queued audio still plays out after the video
// pauses its clock reads, but the sink is paused with it), large enough to
// ride out a slow tick.
const PCM_LOOKAHEAD_US: i64 = 500_000;

struct PlayerEntry {
  player: VideoPlayer,
  texture: u64,
  sink: Option<u64>,
  // Master clock for silent streams: played time accumulated in `base_us`,
  // running since the `clock_now_us` instant `origin_us` while playing.
  // Audio streams read the sink position instead and never touch these.
  base_us: i64,
  origin_us: Option<i64>,
}

struct Inner {
  atx: AlloyContext,
  players: RefCell<HashMap<u64, PlayerEntry>>,
  next_id: RefCell<u64>,
}

impl Drop for Inner {
  // Engine teardown: release what the players hold in alloy (their decode
  // workers exit when the VideoPlayer drops its queue receivers).
  fn drop(&mut self) {
    for (_, entry) in self.players.borrow_mut().drain() {
      self.atx.destroy_texture(entry.texture);
      if let Some(sink) = entry.sink {
        self.atx.destroy_pcm_sink(sink);
      }
    }
  }
}

#[derive(Clone, JsLifetime)]
struct VideoPluginState(#[qjs(skip_trace)] Rc<Inner>);

/// Store the video plugin state in userdata. Runs at engine init, before any
/// module import; the `flux:video` module surface reads it in `evaluate`.
pub(crate) fn store_state(ctx: &Ctx<'_>, atx: AlloyContext) {
  ctx
    .store_userdata(VideoPluginState(Rc::new(Inner {
      atx,
      players: RefCell::new(HashMap::new()),
      next_id: RefCell::new(0),
    })))
    .expect("store video state");
}

/// The `flux:video` module: `open(path)` resolves to a bound player object
/// (`{ texture, width, height, duration, hasAudio, play, pause, playing,
/// currentTime, finished, close }`), so the raw handle never leaves Rust.
pub struct VideoModule;

impl ModuleDef for VideoModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("open")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("open", Function::new(ctx.clone(), open_impl)?)?;
    Ok(())
  }
}

fn open_impl<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<Promise<'js>> {
  // An unreadable or unsupported file is environmental: reject, never throw
  // (the async-binding contract). The header read is synchronous but small.
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  match build_player(ctx.clone(), &path) {
    Ok(obj) => resolve.call::<_, ()>((obj,))?,
    Err(e) => {
      let error = Exception::from_message(ctx.clone(), &format!("openVideo: {e}"))?;
      reject.call::<_, ()>((error,))?;
    }
  }
  Ok(promise)
}

fn build_player<'js>(ctx: Ctx<'js>, path: &str) -> Result<Object<'js>, String> {
  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  let player = VideoPlayer::open(path)?;
  let info = player.info();
  let (width, height) = (info.width, info.height);
  let duration_s = info.duration_us as f64 / 1_000_000.0;

  let layout = match player.layout() {
    PixelLayout::Nv12 => alloy::YuvLayout::Nv12,
    PixelLayout::I420 => alloy::YuvLayout::I420,
  };
  let matrix = if player.color_is_bt709() { alloy::YuvMatrix::Bt709 } else { alloy::YuvMatrix::Bt601 };
  let texture = state.0.atx.create_yuv_texture(
    width,
    height,
    layout,
    matrix,
    // Container metadata carries no range signal (see the demuxer); H.264
    // video is studio range in practice.
    alloy::YuvRange::Limited,
    alloy::SamplerState::default(),
    Some(format!("video:{path}")),
  )?;
  // The player owns its texture: freed by close(), never by the app.
  state.0.atx.borrow_texture(texture);

  // A sink that cannot open (headless box, no output device) plays silent
  // on the wall clock instead of failing the video - and starts paused so
  // prefetched audio waits for play().
  let sink = match info.audio.as_ref() {
    Some(audio) => match state.0.atx.create_pcm_sink(audio.sample_rate, audio.channels) {
      Ok(sink) => {
        if let Err(e) = state.0.atx.set_pcm_sink_paused(sink, true) {
          log::warn!("[video] {e}");
        }
        Some(sink)
      }
      Err(e) => {
        log::warn!("[video] no audio sink, playing silent: {e}");
        None
      }
    },
    None => None,
  };
  let has_audio = sink.is_some();

  let id = {
    let mut next = state.0.next_id.borrow_mut();
    *next += 1;
    *next
  };
  state.0.players.borrow_mut().insert(id, PlayerEntry { player, texture, sink, base_us: 0, origin_us: None });

  let build = || -> rquickjs::Result<Object<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("texture", texture)?;
    obj.set("width", width)?;
    obj.set("height", height)?;
    obj.set("duration", duration_s)?;
    obj.set("hasAudio", has_audio)?;
    obj.set("play", Function::new(ctx.clone(), move |ctx: Ctx<'_>| play_impl(ctx, id, true))?)?;
    obj.set("pause", Function::new(ctx.clone(), move |ctx: Ctx<'_>| play_impl(ctx, id, false))?)?;
    obj.set("playing", Function::new(ctx.clone(), move |ctx: Ctx<'_>| playing_impl(ctx, id))?)?;
    obj.set("currentTime", Function::new(ctx.clone(), move |ctx: Ctx<'_>| current_time_impl(ctx, id))?)?;
    obj.set("finished", Function::new(ctx.clone(), move |ctx: Ctx<'_>| finished_impl(ctx, id))?)?;
    obj.set("close", Function::new(ctx.clone(), move |ctx: Ctx<'_>| close_impl(ctx, id))?)?;
    Ok(obj)
  };
  build().map_err(|e| format!("build player object: {e}"))
}

fn play_impl(ctx: Ctx<'_>, id: u64, play: bool) {
  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  let mut players = state.0.players.borrow_mut();
  let Some(entry) = players.get_mut(&id) else {
    return; // Closed; a late play/pause is a no-op, not an error.
  };
  if play == entry.player.playing() {
    return;
  }
  let now_us = clock_now_us(&ctx);
  if play {
    entry.player.play();
    entry.origin_us = Some(now_us);
  } else {
    entry.player.pause();
    if let Some(origin) = entry.origin_us.take() {
      entry.base_us += now_us - origin;
    }
  }
  if let Some(sink) = entry.sink {
    if let Err(e) = state.0.atx.set_pcm_sink_paused(sink, !play) {
      log::warn!("[video] {e}");
    }
  }
}

fn playing_impl(ctx: Ctx<'_>, id: u64) -> bool {
  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  let playing = state.0.players.borrow().get(&id).map(|e| e.player.playing());
  playing.unwrap_or(false)
}

fn current_time_impl(ctx: Ctx<'_>, id: u64) -> f64 {
  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  let position = state.0.players.borrow().get(&id).map(|e| e.player.position_us());
  position.unwrap_or(0) as f64 / 1_000_000.0
}

fn finished_impl(ctx: Ctx<'_>, id: u64) -> bool {
  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  let finished = state.0.players.borrow().get(&id).map(|e| e.player.finished());
  finished.unwrap_or(true)
}

fn close_impl(ctx: Ctx<'_>, id: u64) {
  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  let Some(entry) = state.0.players.borrow_mut().remove(&id) else {
    return;
  };
  state.0.atx.release_borrowed(entry.texture);
  if let Some(sink) = entry.sink {
    state.0.atx.destroy_pcm_sink(sink);
  }
}

/// What one tick did, for the caller's frame-demand decision.
pub struct VideoTick {
  /// A player uploaded a new frame into its texture: the screen content
  /// changed and a redraw is needed.
  pub uploaded: bool,
  /// A player is mid-playback (playing and not finished): the next frame's
  /// tick is needed even if this one uploaded nothing, so playback acts as
  /// standing frame demand instead of free-running on its own uploads.
  pub playing: bool,
}

/// Per-frame hook, called from the FrameRendered handler alongside
/// `camera::tick`. `period_us` is the display refresh period (0 when the
/// caller has none); silent-stream frame selection looks ahead half of it.
pub fn tick(ctx: &Ctx<'_>, period_us: i64) -> VideoTick {
  let mut result = VideoTick { uploaded: false, playing: false };
  let Some(state) = ctx.userdata::<VideoPluginState>() else {
    return result;
  };
  for entry in state.0.players.borrow_mut().values_mut() {
    // Keep the sink fed up to the lookahead whatever the play state (a
    // paused sink holds its queue), so play() starts with audio ready.
    if let Some(sink) = entry.sink {
      while state.0.atx.pcm_sink_queued_us(sink).unwrap_or(i64::MAX) < PCM_LOOKAHEAD_US {
        let Some(chunk) = entry.player.next_pcm() else {
          break;
        };
        if let Err(e) = state.0.atx.pcm_sink_push(sink, &chunk.samples) {
          log::warn!("[video] {e}");
          break;
        }
      }
    }
    if !entry.player.playing() {
      continue;
    }
    if cfg!(feature = "video-timeline-pacing") {
      result.playing = result.playing || !entry.player.finished();
    }
    let lookahead_us = if cfg!(feature = "video-timeline-pacing") { period_us / 2 } else { 0 };
    let clock_us = match entry.sink {
      Some(sink) => state.0.atx.pcm_sink_position_us(sink).unwrap_or(0),
      None => {
        let now_us = clock_now_us(ctx);
        entry.base_us + entry.origin_us.map(|o| now_us - o).unwrap_or(0) + lookahead_us
      }
    };
    if let Some(frame) = entry.player.advance(clock_us) {
      match state.0.atx.update_yuv(entry.texture, frame.data) {
        Ok(()) => result.uploaded = true,
        Err(e) => log::warn!("[video] {e}"),
      }
    }
  }
  result
}
