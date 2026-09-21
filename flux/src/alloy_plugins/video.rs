//! JS bindings for video playback: a thin marshaling layer over
//! forge::video (demux, decode, sync decisions) and alloy (YUV texture,
//! PCM sink, the Android video plane). A plane player's audio is pushed
//! from its own worker through alloy's cross-thread sink handle. There is NO video primitive: `open()`
//! resolves to a player handle whose `texture` id is displayed with
//! `<texture>`/`<d-texture>`, and a richer Video component composes in a
//! higher layer.
//!
//! Two players, chosen at open (`present`): the texture player below, and
//! on Android the PLANE player - decoder output on its own surface beneath
//! a translucent UI, composited by the platform, no texture, no `tick`, no
//! frame demand (okf/plans/android-video-punch-through.md). This module
//! joins alloy's plane (the surface) and forge's plane player (the codec):
//! open creates the plane then the player on its window, close drops the
//! player first so the codec has released the surface before the plane goes.
//!
//! A plane player streams: its source (a path, an http(s) URL, a `file()`)
//! is read by forge's reader thread, and `open` is asynchronous - the
//! one-plane slot is reserved at the call, the header comes off the JS
//! thread, and the promise settles when the plane exists, the open fails,
//! the `signal` aborts or `OPEN_TIMEOUT_MS` passes. Failures, at open and
//! mid-stream, are `VideoError`s with a `kind` the app keys on
//! (okf/plans/video-streaming.md). The texture player keeps reading local
//! files inline on the frame loop; a URL rejects as unsupported there.
//!
//! `tick`, the per-frame hook driven by the FrameRendered handler (the
//! camera precedent), does the plumbing per player: feed the PCM sink up to
//! a lookahead, read the master clock (the sink position when the stream
//! has audio, an engine-timeline accumulator otherwise), ask the forge
//! player for the frame due, and upload it into the YUV texture.
//!
//! Three properties make the schedule hold (see okf/backlog/video-playback.md):
//! silent streams clock on the engine timeline (`timeline_now_ms`: the paced
//! frame clock in a lattice run) rather than wall time, frame selection gets
//! a half-period lookahead, and a mid-playback player reports standing frame
//! demand so the loop runs on the vsync grid instead of free-running on its
//! own uploads. Wall time is the wrong master clock because the tick's JS
//! work executes at jittery wall moments even when presents are metronomic -
//! a wall read inside the tick inherits that jitter and frame selection holds
//! and double-steps. The paced timeline advances by the display refreshes
//! each frame covered, whatever the execution jitter. The lookahead keeps comparisons off the pts
//! boundary that play() anchors the grids in phase on; without it sub-ms
//! timeline noise flips them. Measured on the 50 Hz TV, 50 fps content:
//! 2.8% of steps held or double-stepped without these, 0.07% with.

#[cfg(target_os = "android")]
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::rc::Rc;

use forge::source::{ByteSource, Source};
use forge::video::reader::Opener;
use forge::video::{ErrorKind, PixelLayout, StreamError, VideoPlayer, WebmDemuxer};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promise;
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value};

use crate::plugins::marshal::OptArg;
use crate::plugins::seekable::{SeekableOpener, SeekableSource};
use crate::standards_plugins::abort::AbortSignal;

// How long a plane open may wait for the source to answer with the header
// and the first keyframe before it rejects (kind "network"): a server that
// never answers must not hold an app's open forever. Generous, since the
// reader reconnects through short outages itself.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
const OPEN_TIMEOUT_MS: u64 = 15_000;

// The silent-stream master clock reading in us: the engine timeline, which
// advances one refresh period per frame however jittery the execution is.
fn clock_now_us(ctx: &Ctx<'_>) -> i64 {
  (crate::standards_plugins::time::timeline_now_ms(ctx) * 1000.0) as i64
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

// A plane player, its plane and its audio sink. Field order is drop order:
// the player (whose drop joins the codec worker, releasing the surface and
// the sink handle) before the plane (which removes the view); the sink is
// destroyed after both by `close_plane_entry`.
#[cfg(target_os = "android")]
struct PlaneEntry {
  player: forge::video::PlanePlayer,
  plane: alloy::video_plane::VideoPlane,
  sink: Option<u64>,
}

// Tear a plane entry down in order: player, plane, then the sink the
// player's worker was pushing into.
#[cfg(target_os = "android")]
fn close_plane_entry(gui: &super::Gui, entry: PlaneEntry) {
  let PlaneEntry { player, plane, sink } = entry;
  drop(player);
  drop(plane);
  if let Some(sink) = sink {
    gui.alloy.destroy_pcm_sink(sink);
  }
}

// alloy's cross-thread sink handle under forge's sink contract: pure
// forwarding, the marshalling between the two crates.
#[cfg(target_os = "android")]
struct SinkAdapter(alloy::audio::PcmSinkHandle);

#[cfg(target_os = "android")]
impl forge::video::AudioSink for SinkAdapter {
  fn push(&mut self, samples: &[f32]) -> Result<(), String> {
    self.0.push(samples)
  }
  fn queued_us(&self) -> i64 {
    self.0.queued_us()
  }
  fn position_us(&self) -> i64 {
    self.0.position_us()
  }
  fn set_paused(&mut self, paused: bool) {
    self.0.set_paused(paused)
  }
  fn clear(&mut self) {
    self.0.clear()
  }
}

// A paused sink for the stream's audio track, None when the stream has no
// audio or no output device could be opened (the video then plays silent
// rather than failing).
#[cfg(target_os = "android")]
fn open_sink(gui: &super::Gui, info: &forge::video::MediaInfo) -> Option<u64> {
  let audio = info.audio.as_ref()?;
  match gui.alloy.create_pcm_sink(audio.sample_rate, audio.channels) {
    Ok(sink) => {
      if let Err(e) = gui.alloy.set_pcm_sink_paused(sink, true) {
        log::warn!("[video] {e}");
      }
      Some(sink)
    }
    Err(e) => {
      log::warn!("[video] no audio sink, playing silent: {e}");
      None
    }
  }
}

struct Inner {
  // The shared host handles; alloy also takes the teardown release in Drop.
  gui: Rc<super::Gui>,
  players: RefCell<HashMap<u64, PlayerEntry>>,
  // Plane players, keyed in the same id space. One at a time (the plane is
  // fullscreen; the platform side refuses a second as well).
  #[cfg(target_os = "android")]
  planes: RefCell<HashMap<u64, PlaneEntry>>,
  // A plane open in flight holds the slot too, so two concurrent opens
  // cannot both pass.
  #[cfg(target_os = "android")]
  plane_pending: Cell<bool>,
  next_id: RefCell<u64>,
}

impl Drop for Inner {
  // Engine teardown: release what the players hold in alloy (their decode
  // workers exit when the VideoPlayer drops its queue receivers; plane
  // entries tear down in their own drop).
  fn drop(&mut self) {
    for (_, entry) in self.players.borrow_mut().drain() {
      self.gui.alloy.destroy_texture(entry.texture);
      if let Some(sink) = entry.sink {
        self.gui.alloy.destroy_pcm_sink(sink);
      }
    }
    #[cfg(target_os = "android")]
    for (_, entry) in self.planes.borrow_mut().drain() {
      close_plane_entry(&self.gui, entry);
    }
  }
}

#[derive(Clone, JsLifetime)]
struct VideoPluginState(#[qjs(skip_trace)] Rc<Inner>);

/// Store the video plugin state in userdata. Runs at engine init, before any
/// module import; the `flux:video` module surface reads it in `evaluate`.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx
    .store_userdata(VideoPluginState(Rc::new(Inner {
      gui: super::gui(ctx),
      players: RefCell::new(HashMap::new()),
      #[cfg(target_os = "android")]
      planes: RefCell::new(HashMap::new()),
      #[cfg(target_os = "android")]
      plane_pending: Cell::new(false),
      next_id: RefCell::new(0),
    })))
    .expect("store video state");
}

/// The `flux:video` module: `open(source, options?)` resolves to a bound
/// player object (`{ texture, width, height, duration, hasAudio, seekable,
/// play, pause, seek, playing, currentTime, finished, buffering, error,
/// failed, close }`; a plane player has no `texture`), so the raw handle
/// never leaves Rust.
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

/// How a player presents, from the `present` option: through its texture
/// (default) or on the platform's video plane.
enum Present {
  Texture,
  Plane { fit: PlaneFit },
}

enum PlaneFit {
  Contain,
  Cover,
}

fn read_present(ctx: &Ctx<'_>, options: &OptArg<Object<'_>>) -> rquickjs::Result<Present> {
  let Some(opts) = options.0.as_ref() else {
    return Ok(Present::Texture);
  };
  let present = opts.get::<_, Option<String>>("present")?;
  match present.as_deref() {
    None | Some("texture") => Ok(Present::Texture),
    Some("plane") => {
      let fit = match opts.get::<_, Option<String>>("fit")?.as_deref() {
        None | Some("contain") => PlaneFit::Contain,
        Some("cover") => PlaneFit::Cover,
        Some(other) => {
          return Err(Exception::throw_type(
            ctx,
            &format!("openVideo: fit must be \"contain\" or \"cover\", got \"{other}\""),
          ))
        }
      };
      Ok(Present::Plane { fit })
    }
    Some(other) => {
      Err(Exception::throw_type(ctx, &format!("openVideo: present must be \"texture\" or \"plane\", got \"{other}\"")))
    }
  }
}

/// What `open` was given: a path or URL under the one source rule, or a
/// `file()` object's own opener (a packed asset reads out of the exe).
enum MediaSource {
  Spec(Source),
  File(SeekableOpener),
}

impl MediaSource {
  fn is_url(&self) -> bool {
    matches!(self, MediaSource::Spec(Source::Http(_)))
  }

  /// A name for the texture label.
  fn label(&self) -> String {
    match self {
      MediaSource::Spec(Source::Path(path)) | MediaSource::Spec(Source::Http(path)) => path.clone(),
      MediaSource::File(_) => "file".to_string(),
    }
  }

  /// The opener the reader thread runs (or the texture player, inline).
  fn into_opener(self, user_agent: String) -> Opener {
    match self {
      MediaSource::Spec(source) => Box::new(move || source.open(&user_agent)),
      MediaSource::File(open) => Box::new(move || {
        let reader = open().map_err(io::Error::other)?;
        forge::source::from_seekable(reader)
      }),
    }
  }
}

// A string goes through the source rule (a bad scheme is a programming
// error and throws); an object must be a file().
fn read_source<'js>(ctx: &Ctx<'js>, source: Value<'js>) -> rquickjs::Result<MediaSource> {
  if let Some(spec) = source.as_string() {
    let spec = spec.to_string()?;
    return Source::parse(&spec)
      .map(MediaSource::Spec)
      .map_err(|e| Exception::throw_type(ctx, &format!("openVideo: {e}")));
  }
  if let Some(obj) = source.as_object() {
    return SeekableSource::opener(obj).map(MediaSource::File).map_err(|e| {
      Exception::throw_type(ctx, &format!("openVideo: source must be a path, an http(s) URL or a file() ({e})"))
    });
  }
  Err(Exception::throw_type(ctx, "openVideo: source must be a path, an http(s) URL or a file()"))
}

fn read_signal<'js>(options: &OptArg<Object<'js>>) -> rquickjs::Result<Option<Class<'js, AbortSignal<'js>>>> {
  match options.0.as_ref() {
    Some(opts) => opts.get::<_, Option<Class<AbortSignal>>>("signal"),
    None => Ok(None),
  }
}

/// A `VideoError`: an Error whose `kind` names what went wrong, for the app
/// to key on (a plane-to-texture fallback on "no-plane" only).
fn video_error<'js>(ctx: &Ctx<'js>, error: &StreamError, prefix: &str) -> rquickjs::Result<Value<'js>> {
  let err = Exception::from_message(ctx.clone(), &format!("{prefix}{}", error.message))?;
  err.set("kind", error.kind.name())?;
  Ok(err.into_value())
}

fn open_impl<'js>(ctx: Ctx<'js>, source: Value<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<Promise<'js>> {
  // A malformed option or source is a programming error and throws; an
  // unreadable or unsupported stream, or a platform without a plane, is
  // environmental: reject, never throw (the async-binding contract).
  let present = read_present(&ctx, &options)?;
  let source = read_source(&ctx, source)?;
  let signal = read_signal(&options)?;
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  match present {
    Present::Texture => match build_player(ctx.clone(), source) {
      Ok(obj) => resolve.call::<_, ()>((obj,))?,
      Err(e) => reject.call::<_, ()>((video_error(&ctx, &e, "openVideo: ")?,))?,
    },
    Present::Plane { fit } => open_plane(ctx.clone(), source, fit, signal, resolve, reject)?,
  }
  Ok(promise)
}

#[cfg(not(target_os = "android"))]
fn open_plane<'js>(
  ctx: Ctx<'js>,
  _source: MediaSource,
  _fit: PlaneFit,
  _signal: Option<Class<'js, AbortSignal<'js>>>,
  _resolve: Function<'js>,
  reject: Function<'js>,
) -> rquickjs::Result<()> {
  let error =
    StreamError::new(ErrorKind::NoPlane, "no video plane on this platform (present: \"plane\" is Android only)");
  reject.call::<_, ()>((video_error(&ctx, &error, "openVideo: ")?,))
}

/// The plane open: the slot is taken now, the reader thread reads the
/// header, and the promise settles from a task racing that answer against
/// the abort signal and the timeout. The plane and the codec are created
/// on this thread once the header is in, as before.
#[cfg(target_os = "android")]
fn open_plane<'js>(
  ctx: Ctx<'js>,
  source: MediaSource,
  fit: PlaneFit,
  signal: Option<Class<'js, AbortSignal<'js>>>,
  resolve: Function<'js>,
  reject: Function<'js>,
) -> rquickjs::Result<()> {
  use forge::video::reader::Reader;
  use std::time::Duration;

  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  if !state.0.planes.borrow().is_empty() || state.0.plane_pending.get() {
    let error = StreamError::new(ErrorKind::NoPlane, "a video plane is already open (one at a time; close it first)");
    return reject.call::<_, ()>((video_error(&ctx, &error, "openVideo: ")?,));
  }
  if let Some(sig) = &signal {
    if sig.borrow().aborted() {
      let reason = sig.borrow().reason(ctx.clone());
      return reject.call::<_, ()>((reason,));
    }
  }
  state.0.plane_pending.set(true);
  let user_agent = crate::standards_plugins::http::user_agent(&ctx);
  let (reader, opened) = Reader::open(source.into_opener(user_agent));
  let aborted = signal.as_ref().map(|sig| sig.borrow().subscribe());
  let pending = ctx.userdata::<crate::pending::PendingOps>().expect("pending ops").clone();
  let task_ctx = ctx.clone();
  ctx.spawn(async move {
    enum Outcome {
      Opened(Result<forge::video::MediaInfo, StreamError>),
      Aborted,
      TimedOut,
    }
    pending.hold();
    let timeout = tokio::time::sleep(Duration::from_millis(OPEN_TIMEOUT_MS));
    tokio::pin!(timeout);
    let outcome = tokio::select! {
      answer = opened => Outcome::Opened(answer.unwrap_or_else(|_| Err(StreamError::network("the reader stopped before answering")))),
      _ = async { match aborted { Some(rx) => { let _ = rx.await; } None => std::future::pending::<()>().await } } => Outcome::Aborted,
      _ = &mut timeout => Outcome::TimedOut,
    };
    pending.release();
    let ctx = task_ctx;
    let state = ctx.userdata::<VideoPluginState>().expect("video state");
    state.0.plane_pending.set(false);
    let settled = match outcome {
      Outcome::Opened(Ok(info)) => match build_plane_player(ctx.clone(), reader, info, fit) {
        Ok(obj) => resolve.call::<_, ()>((obj,)),
        Err(e) => video_error(&ctx, &e, "openVideo: ").and_then(|err| reject.call::<_, ()>((err,))),
      },
      Outcome::Opened(Err(e)) => {
        drop(reader);
        video_error(&ctx, &e, "openVideo: ").and_then(|err| reject.call::<_, ()>((err,)))
      }
      Outcome::Aborted => {
        // Dropping the reader interrupts its read and cancels the source.
        drop(reader);
        let reason = signal.as_ref().map(|sig| sig.borrow().reason(ctx.clone()));
        reject.call::<_, ()>((reason,))
      }
      Outcome::TimedOut => {
        drop(reader);
        let error = StreamError::network(format!("no answer from the source within {OPEN_TIMEOUT_MS} ms"));
        video_error(&ctx, &error, "openVideo: ").and_then(|err| reject.call::<_, ()>((err,)))
      }
    };
    if let Err(e) = settled {
      log::warn!("[video] settle open: {e}");
    }
  });
  Ok(())
}

#[cfg(target_os = "android")]
fn build_plane_player<'js>(
  ctx: Ctx<'js>,
  reader: forge::video::reader::Reader,
  info: forge::video::MediaInfo,
  fit: PlaneFit,
) -> Result<Object<'js>, StreamError> {
  use alloy::video_plane::{PlaneFit as AlloyFit, VideoPlane};
  use forge::video::transport::VsyncGrid;
  use forge::video::{AudioSink, PlanePlayer};

  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  let fit = match fit {
    PlaneFit::Contain => AlloyFit::Contain,
    PlaneFit::Cover => AlloyFit::Cover,
  };
  let plane = VideoPlane::create(info.width, info.height, fit).map_err(|e| StreamError::new(ErrorKind::NoPlane, e))?;
  // The display's vsync grid: the period the plane read from the display,
  // the phase from the samples its view keeps reporting.
  let vsync = plane
    .refresh_period_ns()
    .map(|period_ns| VsyncGrid { period_ns, sample_ns: Box::new(alloy::video_plane::vsync_ns) });
  let sink = open_sink(&state.0.gui, &info);
  let handle = match sink.map(|id| state.0.gui.alloy.pcm_sink_handle(id)).transpose() {
    Ok(handle) => handle.map(|h| Box::new(SinkAdapter(h)) as Box<dyn AudioSink>),
    Err(e) => {
      if let Some(sink) = sink {
        state.0.gui.alloy.destroy_pcm_sink(sink);
      }
      return Err(StreamError::decode(e));
    }
  };
  let lost = Box::new(alloy::video_plane::lost);
  let player = match PlanePlayer::open(reader, plane.native_window().clone(), vsync, handle, lost) {
    Ok(player) => player,
    Err(e) => {
      if let Some(sink) = sink {
        state.0.gui.alloy.destroy_pcm_sink(sink);
      }
      return Err(StreamError::decode(e));
    }
  };
  let has_audio = sink.is_some();
  let shared = player.shared();

  let id = {
    let mut next = state.0.next_id.borrow_mut();
    *next += 1;
    *next
  };
  state.0.planes.borrow_mut().insert(id, PlaneEntry { player, plane, sink });

  let build = || -> rquickjs::Result<Object<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("width", info.width)?;
    obj.set("height", info.height)?;
    obj.set("duration", info.duration_us.map(|d| d as f64 / 1_000_000.0))?;
    obj.set("hasAudio", has_audio)?;
    obj.set("seekable", info.seekable)?;
    obj.set("play", Function::new(ctx.clone(), move |ctx: Ctx<'_>| with_plane(&ctx, id, |e| e.player.play()))?)?;
    obj.set("pause", Function::new(ctx.clone(), move |ctx: Ctx<'_>| with_plane(&ctx, id, |e| e.player.pause()))?)?;
    obj.set(
      "seek",
      Function::new(ctx.clone(), move |ctx: Ctx<'_>, seconds: f64| {
        with_plane(&ctx, id, |e| e.player.seek((seconds.max(0.0) * 1_000_000.0) as i64))
      })?,
    )?;
    obj.set(
      "playing",
      Function::new(ctx.clone(), move |ctx: Ctx<'_>| with_plane(&ctx, id, |e| e.player.playing()).unwrap_or(false))?,
    )?;
    obj.set(
      "currentTime",
      Function::new(ctx.clone(), move |ctx: Ctx<'_>| {
        with_plane(&ctx, id, |e| e.player.position_us()).unwrap_or(0) as f64 / 1_000_000.0
      })?,
    )?;
    // A plane whose surface the platform took (backgrounded) is finished
    // too: the codec has nothing to render into.
    obj.set(
      "finished",
      Function::new(ctx.clone(), move |ctx: Ctx<'_>| {
        with_plane(&ctx, id, |e| e.player.finished() || e.plane.lost()).unwrap_or(true)
      })?,
    )?;
    obj.set(
      "buffering",
      Function::new(ctx.clone(), move |ctx: Ctx<'_>| with_plane(&ctx, id, |e| e.player.buffering()).unwrap_or(false))?,
    )?;
    obj.set("error", Function::new(ctx.clone(), value_builder(move |ctx| plane_error_impl(ctx, id)))?)?;
    // The one-shot: resolves with the failure that stops playback, with
    // undefined when the player closes without one.
    let (failed, resolve_failed, _reject) = Promise::new(&ctx)?;
    let failed_ctx = ctx.clone();
    ctx.spawn(async move {
      let outcome = shared.failed().await;
      let value = match outcome.as_ref() {
        Some(error) => video_error(&failed_ctx, error, ""),
        None => Ok(Value::new_undefined(failed_ctx.clone())),
      };
      if let Err(e) = value.and_then(|v| resolve_failed.call::<_, ()>((v,))) {
        log::warn!("[video] settle failed: {e}");
      }
    });
    obj.set("failed", failed)?;
    obj.set("close", Function::new(ctx.clone(), move |ctx: Ctx<'_>| close_plane(&ctx, id))?)?;
    Ok(obj)
  };
  build().map_err(|e| StreamError::decode(format!("build plane player object: {e}")))
}

#[cfg(target_os = "android")]
fn plane_error_impl<'js>(ctx: Ctx<'js>, id: u64) -> rquickjs::Result<Value<'js>> {
  match with_plane(&ctx, id, |e| e.player.error()).flatten() {
    Some(error) => video_error(&ctx, &error, ""),
    None => Ok(Value::new_undefined(ctx)),
  }
}

/// Force the `for<'js>` HRTB on a capturing closure that returns a `'js`-bound
/// `Value` (see flux/CLAUDE.md "Ctx and the 'js lifetime").
#[cfg(target_os = "android")]
fn value_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>) -> rquickjs::Result<Value<'js>>,
{
  f
}

// Run `f` on the plane entry `id`, None once closed (a late call on a closed
// player is a no-op, not an error).
#[cfg(target_os = "android")]
fn with_plane<T>(ctx: &Ctx<'_>, id: u64, f: impl FnOnce(&PlaneEntry) -> T) -> Option<T> {
  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  let planes = state.0.planes.borrow();
  planes.get(&id).map(f)
}

#[cfg(target_os = "android")]
fn close_plane(ctx: &Ctx<'_>, id: u64) {
  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  // Take it out first, then tear down outside the borrow: the drop joins
  // the codec worker and removes the view, neither of which should hold
  // the map.
  let entry = state.0.planes.borrow_mut().remove(&id);
  if let Some(entry) = entry {
    close_plane_entry(&state.0.gui, entry);
  }
}

// The texture player reads inline on the frame loop, so it takes local
// sources only: a URL rejects until its browser-style rework
// (okf/plans/android-video-punch-through.md).
fn build_player<'js>(ctx: Ctx<'js>, source: MediaSource) -> Result<Object<'js>, StreamError> {
  let state = ctx.userdata::<VideoPluginState>().expect("video state");
  if source.is_url() {
    return Err(StreamError::unsupported(
      "streaming plays on a plane until the texture player moves off the frame loop",
    ));
  }
  let label = source.label();
  let bytes: ByteSource = source.into_opener(crate::standards_plugins::http::user_agent(&ctx))()
    .map_err(|e| StreamError::network(e.to_string()))?;
  let facts = bytes.facts().map_err(|e| StreamError::network(e.to_string()))?;
  let demux = WebmDemuxer::open(bytes.into_reader(), facts)?;
  let player = VideoPlayer::open(demux)?;
  let info = player.info();
  let (width, height) = (info.width, info.height);
  // None (undefined in JS) when the source has no duration: a live stream.
  let duration_s = info.duration_us.map(|d| d as f64 / 1_000_000.0);

  let layout = match player.layout() {
    PixelLayout::Nv12 => alloy::YuvLayout::Nv12,
    PixelLayout::I420 => alloy::YuvLayout::I420,
  };
  let matrix = if player.color_is_bt709() { alloy::YuvMatrix::Bt709 } else { alloy::YuvMatrix::Bt601 };
  // Both from the container's vpcC box (the matrix falls back to the
  // resolution default when it is unspecified there).
  let range = if player.color_is_full_range() { alloy::YuvRange::Full } else { alloy::YuvRange::Limited };
  let texture = state
    .0
    .gui
    .alloy
    .create_yuv_texture(
      width,
      height,
      layout,
      matrix,
      range,
      alloy::SamplerState::default(),
      Some(format!("video:{label}")),
    )
    .map_err(StreamError::decode)?;
  // The player owns its texture: freed by close(), never by the app.
  state.0.gui.alloy.borrow_texture(texture);

  // A sink that cannot open (headless box, no output device) plays silent
  // on the wall clock instead of failing the video - and starts paused so
  // prefetched audio waits for play().
  let sink = match info.audio.as_ref() {
    Some(audio) => match state.0.gui.alloy.create_pcm_sink(audio.sample_rate, audio.channels) {
      Ok(sink) => {
        if let Err(e) = state.0.gui.alloy.set_pcm_sink_paused(sink, true) {
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
    // The same shape as a plane player, with the transport this player
    // does not have yet: no seek, never buffering, no mid-stream failure.
    obj.set("seekable", false)?;
    obj.set("play", Function::new(ctx.clone(), move |ctx: Ctx<'_>| play_impl(ctx, id, true))?)?;
    obj.set("pause", Function::new(ctx.clone(), move |ctx: Ctx<'_>| play_impl(ctx, id, false))?)?;
    obj.set("seek", Function::new(ctx.clone(), |_seconds: f64| ())?)?;
    obj.set("playing", Function::new(ctx.clone(), move |ctx: Ctx<'_>| playing_impl(ctx, id))?)?;
    obj.set("currentTime", Function::new(ctx.clone(), move |ctx: Ctx<'_>| current_time_impl(ctx, id))?)?;
    obj.set("finished", Function::new(ctx.clone(), move |ctx: Ctx<'_>| finished_impl(ctx, id))?)?;
    obj.set("buffering", Function::new(ctx.clone(), || false)?)?;
    obj.set("error", Function::new(ctx.clone(), undefined)?)?;
    let (never, _resolve, _reject) = Promise::new(&ctx)?;
    obj.set("failed", never)?;
    obj.set("close", Function::new(ctx.clone(), move |ctx: Ctx<'_>| close_impl(ctx, id))?)?;
    Ok(obj)
  };
  build().map_err(|e| StreamError::decode(format!("build player object: {e}")))
}

fn undefined(ctx: Ctx<'_>) -> Value<'_> {
  Value::new_undefined(ctx)
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
    if let Err(e) = state.0.gui.alloy.set_pcm_sink_paused(sink, !play) {
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
  state.0.gui.alloy.release_borrowed(entry.texture);
  if let Some(sink) = entry.sink {
    state.0.gui.alloy.destroy_pcm_sink(sink);
  }
}

/// What one tick did, for the caller's frame-demand decision.
pub(crate) struct VideoTick {
  /// A player uploaded a new frame into its texture: the screen content
  /// changed and a redraw is needed.
  pub uploaded: bool,
  /// A player is mid-playback (playing and not finished): the next frame's
  /// tick is needed even if this one uploaded nothing, so playback acts as
  /// standing frame demand instead of free-running on its own uploads.
  pub playing: bool,
}

/// Per-frame hook (see `frame::advance`). `period_us` is the display
/// refresh period (0 when the caller has none); silent-stream frame
/// selection looks ahead half of it.
pub(crate) fn tick(ctx: &Ctx<'_>, period_us: i64) -> VideoTick {
  let mut result = VideoTick { uploaded: false, playing: false };
  let Some(state) = ctx.userdata::<VideoPluginState>() else {
    return result;
  };
  for entry in state.0.players.borrow_mut().values_mut() {
    // Keep the sink fed up to the lookahead whatever the play state (a
    // paused sink holds its queue), so play() starts with audio ready.
    if let Some(sink) = entry.sink {
      while state.0.gui.alloy.pcm_sink_queued_us(sink).unwrap_or(i64::MAX) < PCM_LOOKAHEAD_US {
        let Some(chunk) = entry.player.next_pcm() else {
          break;
        };
        if let Err(e) = state.0.gui.alloy.pcm_sink_push(sink, &chunk.samples) {
          log::warn!("[video] {e}");
          break;
        }
      }
    }
    if !entry.player.playing() {
      continue;
    }
    result.playing = result.playing || !entry.player.finished();
    // Half a period: play() anchors the pts grid in phase with the tick
    // grid, so without the offset every comparison sits on a boundary that
    // sub-ms timeline noise flips.
    let lookahead_us = period_us / 2;
    let clock_us = match entry.sink {
      Some(sink) => state.0.gui.alloy.pcm_sink_position_us(sink).unwrap_or(0),
      None => {
        let now_us = clock_now_us(ctx);
        entry.base_us + entry.origin_us.map(|o| now_us - o).unwrap_or(0) + lookahead_us
      }
    };
    if let Some(frame) = entry.player.advance(clock_us) {
      match state.0.gui.alloy.update_yuv(entry.texture, frame.data) {
        Ok(()) => result.uploaded = true,
        Err(e) => log::warn!("[video] {e}"),
      }
    }
  }
  result
}
