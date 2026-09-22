//! JS bindings for video playback: a thin marshaling layer over
//! forge::video (the reader, the decoders, the playback worker) and alloy
//! (the YUV texture's frame latch, the PCM sink, the Android video plane).
//! There is NO video primitive: `open()` resolves to a player handle whose
//! `texture` id is displayed with `<texture>`/`<d-texture>`, and a richer
//! Video component composes in a higher layer.
//!
//! Two players, chosen at open (`present`): the TEXTURE player, whose
//! decoded frames the worker pushes into alloy's YUV texture latch with the
//! time each is due on alloy's clock, and on Android the PLANE player -
//! decoder output on its own surface beneath a translucent UI, composited
//! by the platform (okf/plans/android-video-punch-through.md). Neither runs
//! anything on the JS thread or in the frame loop: demux, decode, the
//! clock, frame selection and audio are the worker's, and the raster thread
//! latches the texture player's due frame at each frame's presentation
//! deadline (okf/plans/video-texture-off-frame-loop.md). This module joins
//! the pieces: a plane open creates the plane and hands it to the player,
//! whose worker owns it from then on and drops it after the codec has
//! released the surface; a texture open creates the texture and hands the
//! worker a sink on its latch. Close sends the worker its close and
//! returns; nothing joins. The next plane open awaits the closing worker's
//! exit before it creates its own plane, so the platform's one-plane check
//! holds.
//!
//! Both players stream: the source (a path, an http(s) URL, a `file()`) is
//! read by forge's reader thread, and `open` is asynchronous - the header
//! comes off the JS thread, and the promise settles when the player exists,
//! the open fails, the `signal` aborts or `OPEN_TIMEOUT_MS` passes.
//! Failures, at open and mid-stream, are `VideoError`s with a `kind` the
//! app keys on (okf/plans/video-streaming.md).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use forge::source::Source;
use forge::video::reader::{Opener, Reader};
use forge::video::transport::Shared;
use forge::video::{ErrorKind, MediaInfo, PixelLayout, Player, StreamError, YuvFrame};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promise;
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value};

use crate::plugins::marshal::OptArg;
use crate::plugins::seekable::{SeekableOpener, SeekableSource};
use crate::standards_plugins::abort::AbortSignal;

// How long an open may wait for the source to answer with the header and
// the first keyframe before it rejects (kind "network"): a server that
// never answers must not hold an app's open forever. Generous, since the
// reader reconnects through short outages itself.
const OPEN_TIMEOUT_MS: u64 = 15_000;

/// One open player: the worker's handle, its audio sink (a registry id,
/// destroyed at close; the worker's handle on it outlives the entry), and
/// how it presents.
struct Entry {
  player: Player,
  sink: Option<u64>,
  kind: Kind,
}

enum Kind {
  /// The YUV texture the frames land in (borrowed: freed by close, never
  /// by the app) and a handle on its latch, what `currentTime` reads and
  /// `play` wakes.
  Texture { texture: u64, frames: alloy::YuvFrameSink },
  /// The platform's plane; owned by the worker from open to its exit.
  #[cfg(target_os = "android")]
  Plane,
}

// alloy's cross-thread PCM sink handle under forge's audio sink contract:
// pure forwarding, the marshalling between the two crates.
struct SinkAdapter(alloy::audio::PcmSinkHandle);

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

// alloy's YUV latch handle under forge's frame sink contract: the same
// forwarding for the picture.
struct FrameSinkAdapter(alloy::YuvFrameSink);

impl forge::video::FrameSink for FrameSinkAdapter {
  fn push(&mut self, frame: YuvFrame, due_ns: i64) {
    if let Err(e) = self.0.push(frame.data, frame.pts_us, due_ns) {
      log::warn!("[video] {e}");
    }
  }
  fn shown_pts_us(&self) -> Option<i64> {
    self.0.shown_pts_us()
  }
  fn set_playing(&mut self, playing: bool) {
    self.0.set_playing(playing)
  }
  fn period_ns(&self) -> Option<i64> {
    self.0.period_ns()
  }
  fn end(&mut self) {
    self.0.end()
  }
  fn flush(&mut self) {
    self.0.flush()
  }
}

// A paused sink for the stream's audio track and the worker's handle on it
// under forge's contract, None when the stream has no audio or no output
// device could be opened (the video then plays silent rather than
// failing).
fn open_sink(gui: &super::Gui, info: &MediaInfo) -> (Option<u64>, Option<Box<dyn forge::video::AudioSink>>) {
  let Some(audio) = info.audio.as_ref() else {
    return (None, None);
  };
  let sink = match gui.alloy.create_pcm_sink(audio.sample_rate, audio.channels) {
    Ok(sink) => sink,
    Err(e) => {
      log::warn!("[video] no audio sink, playing silent: {e}");
      return (None, None);
    }
  };
  if let Err(e) = gui.alloy.set_pcm_sink_paused(sink, true) {
    log::warn!("[video] {e}");
  }
  match gui.alloy.pcm_sink_handle(sink) {
    Ok(handle) => (Some(sink), Some(Box::new(SinkAdapter(handle)))),
    Err(e) => {
      log::warn!("[video] no audio sink handle, playing silent: {e}");
      gui.alloy.destroy_pcm_sink(sink);
      (None, None)
    }
  }
}

struct Inner {
  // The shared host handles; alloy also takes the teardown release in Drop.
  gui: Rc<super::Gui>,
  players: RefCell<HashMap<u64, Entry>>,
  // A plane open in flight holds the platform's one plane slot too, so two
  // concurrent opens cannot both pass.
  #[cfg(target_os = "android")]
  plane_pending: Cell<bool>,
  // The last closed plane player's worker, still releasing its surface: a
  // new plane open awaits its exit before creating its plane, so the old
  // view's removal is posted before the new one's creation and the
  // platform's one-plane check holds.
  #[cfg(target_os = "android")]
  closing: RefCell<Option<Arc<Shared>>>,
  next_id: Cell<u64>,
}

impl Drop for Inner {
  // Engine teardown: release what the players hold in alloy; their
  // workers exit on the close each player's drop sends, nothing waits.
  fn drop(&mut self) {
    for (_, entry) in self.players.borrow_mut().drain() {
      close_entry(&self.gui, entry);
    }
  }
}

// Tear an entry down: the player (its close), the texture's borrow (its
// latch closes with it, so a worker still pushing hits a no-op), then the
// sink's registry entry. Returns the worker's state, for a plane's
// successor to await its exit.
fn close_entry(gui: &super::Gui, entry: Entry) -> Arc<Shared> {
  let Entry { player, sink, kind } = entry;
  let shared = player.shared();
  drop(player);
  match kind {
    Kind::Texture { texture, .. } => gui.alloy.release_borrowed(texture),
    #[cfg(target_os = "android")]
    Kind::Plane => {}
  }
  if let Some(sink) = sink {
    gui.alloy.destroy_pcm_sink(sink);
  }
  shared
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
      plane_pending: Cell::new(false),
      #[cfg(target_os = "android")]
      closing: RefCell::new(None),
      next_id: Cell::new(0),
    })))
    .expect("store video state");
}

fn state(ctx: &Ctx<'_>) -> VideoPluginState {
  ctx.userdata::<VideoPluginState>().expect("video state").clone()
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

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
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
  /// A name for the texture label.
  fn label(&self) -> String {
    match self {
      MediaSource::Spec(Source::Path(path)) | MediaSource::Spec(Source::Http(path)) => path.clone(),
      MediaSource::File(_) => "file".to_string(),
    }
  }

  /// The opener the reader thread runs.
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

/// The open: the reader thread reads the header, and the promise settles
/// from a task racing that answer against the abort signal and the
/// timeout. The player is built on this thread once the header is in. A
/// plane open takes the platform's one plane slot at the call.
fn open_impl<'js>(ctx: Ctx<'js>, source: Value<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<Promise<'js>> {
  // A malformed option or source is a programming error and throws; an
  // unreadable or unsupported stream, or a platform without a plane, is
  // environmental: reject, never throw (the async-binding contract).
  let present = read_present(&ctx, &options)?;
  let source = read_source(&ctx, source)?;
  let signal = read_signal(&options)?;
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  let state = state(&ctx);
  let plane = matches!(present, Present::Plane { .. });
  if plane {
    if let Some(error) = plane_unavailable(&state) {
      reject.call::<_, ()>((video_error(&ctx, &error, "openVideo: ")?,))?;
      return Ok(promise);
    }
  }
  if let Some(sig) = &signal {
    if sig.borrow().aborted() {
      let reason = sig.borrow().reason(ctx.clone());
      reject.call::<_, ()>((reason,))?;
      return Ok(promise);
    }
  }
  #[cfg(target_os = "android")]
  if plane {
    state.0.plane_pending.set(true);
  }
  let label = source.label();
  let user_agent = crate::standards_plugins::http::user_agent(&ctx);
  let (reader, opened) = Reader::open(source.into_opener(user_agent));
  let aborted = signal.as_ref().map(|sig| sig.borrow().subscribe());
  let pending = ctx.userdata::<crate::pending::PendingOps>().expect("pending ops").clone();
  let task_ctx = ctx.clone();
  ctx.spawn(async move {
    enum Outcome {
      Opened(Result<MediaInfo, StreamError>),
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
    // A predecessor still closing holds the platform's one plane until its
    // worker has dropped the view; its removal must be posted before this
    // open's creation is.
    #[cfg(target_os = "android")]
    if plane {
      let closing = state.0.closing.borrow_mut().take();
      if let Some(closing) = closing {
        closing.exited().await;
      }
      state.0.plane_pending.set(false);
    }
    #[cfg(not(target_os = "android"))]
    let _ = plane;
    pending.release();
    let ctx = task_ctx;
    let settled = match outcome {
      Outcome::Opened(Ok(info)) => match build_player(ctx.clone(), &state, reader, info, present, label) {
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
  Ok(promise)
}

// Why a plane cannot be opened now, if it cannot: no plane on this
// platform, or one is open or opening (one at a time; the platform side
// refuses a second as well).
#[cfg(target_os = "android")]
fn plane_unavailable(state: &VideoPluginState) -> Option<StreamError> {
  let open = state.0.players.borrow().values().any(|e| matches!(e.kind, Kind::Plane));
  (open || state.0.plane_pending.get())
    .then(|| StreamError::new(ErrorKind::NoPlane, "a video plane is already open (one at a time; close it first)"))
}

#[cfg(not(target_os = "android"))]
fn plane_unavailable(_state: &VideoPluginState) -> Option<StreamError> {
  Some(StreamError::new(ErrorKind::NoPlane, "no video plane on this platform (present: \"plane\" is Android only)"))
}

/// The player over the opened stream: its audio sink, its presenter
/// (the texture and a sink on its latch, or the plane) and the worker, then
/// the bound object.
fn build_player<'js>(
  ctx: Ctx<'js>,
  state: &VideoPluginState,
  reader: Reader,
  info: MediaInfo,
  present: Present,
  label: String,
) -> Result<Object<'js>, StreamError> {
  let gui = &state.0.gui;
  let (sink, audio) = open_sink(gui, &info);
  let has_audio = sink.is_some();
  let opened = match present {
    Present::Texture => open_texture_player(gui, reader, &info, audio, &label),
    Present::Plane { fit } => open_plane_player(gui, reader, &info, audio, fit),
  };
  let (player, kind) = match opened {
    Ok(opened) => opened,
    Err(e) => {
      if let Some(sink) = sink {
        gui.alloy.destroy_pcm_sink(sink);
      }
      return Err(e);
    }
  };
  let texture = match &kind {
    Kind::Texture { texture, .. } => Some(*texture),
    #[cfg(target_os = "android")]
    Kind::Plane => None,
  };
  let shared = player.shared();
  let id = state.0.next_id.get() + 1;
  state.0.next_id.set(id);
  state.0.players.borrow_mut().insert(id, Entry { player, sink, kind });
  build_object(ctx, id, &info, has_audio, texture, shared)
}

// The texture player: a YUV texture for the decoder's layout and the
// stream's color facts, borrowed by the player, and the worker over a sink
// on its latch, scheduling on alloy's clock (the raster thread's deadlines
// are on the same one).
fn open_texture_player(
  gui: &super::Gui,
  reader: Reader,
  info: &MediaInfo,
  audio: Option<Box<dyn forge::video::AudioSink>>,
  label: &str,
) -> Result<(Player, Kind), StreamError> {
  let layout = match forge::video::decoded_layout() {
    PixelLayout::Nv12 => alloy::YuvLayout::Nv12,
    PixelLayout::I420 => alloy::YuvLayout::I420,
  };
  let matrix = if info.bt709 { alloy::YuvMatrix::Bt709 } else { alloy::YuvMatrix::Bt601 };
  let range = if info.full_range { alloy::YuvRange::Full } else { alloy::YuvRange::Limited };
  let texture = gui
    .alloy
    .create_yuv_texture(
      info.width,
      info.height,
      layout,
      matrix,
      range,
      alloy::SamplerState::default(),
      Some(format!("video:{label}")),
    )
    .map_err(StreamError::decode)?;
  // The player owns its texture: freed by close(), never by the app.
  gui.alloy.borrow_texture(texture);
  let opened = (|| {
    let request = gui.platform.frame_request_handle();
    let frames = gui.alloy.yuv_frame_sink(texture, request.clone()).map_err(StreamError::decode)?;
    let worker_sink = gui.alloy.yuv_frame_sink(texture, request).map_err(StreamError::decode)?;
    let clock = forge::video::Clock { now_ns: Box::new(alloy::clock::now_ns), stepped: alloy::clock::stepped() };
    let player = forge::video::open_texture(reader, Box::new(FrameSinkAdapter(worker_sink)), clock, audio)
      .map_err(StreamError::decode)?;
    Ok((player, Kind::Texture { texture, frames }))
  })();
  if opened.is_err() {
    gui.alloy.release_borrowed(texture);
  }
  opened
}

// The plane player: the platform's plane for the picture's size, its vsync
// grid for the release snap, and the worker over the codec on its surface,
// which owns the plane from here.
#[cfg(target_os = "android")]
fn open_plane_player(
  _gui: &super::Gui,
  reader: Reader,
  info: &MediaInfo,
  audio: Option<Box<dyn forge::video::AudioSink>>,
  fit: PlaneFit,
) -> Result<(Player, Kind), StreamError> {
  use alloy::video_plane::{PlaneFit as AlloyFit, VideoPlane};
  use forge::video::transport::VsyncGrid;

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
  let lost = Box::new(alloy::video_plane::lost);
  let window = plane.native_window().clone();
  let player =
    forge::video::open_plane(reader, window, Box::new(plane), vsync, audio, lost).map_err(StreamError::decode)?;
  Ok((player, Kind::Plane))
}

#[cfg(not(target_os = "android"))]
fn open_plane_player(
  _gui: &super::Gui,
  _reader: Reader,
  _info: &MediaInfo,
  _audio: Option<Box<dyn forge::video::AudioSink>>,
  _fit: PlaneFit,
) -> Result<(Player, Kind), StreamError> {
  // Refused at the call (see `plane_unavailable`); never reached.
  Err(StreamError::new(ErrorKind::NoPlane, "no video plane on this platform"))
}

// The bound object both players share; `texture` is the texture player's
// output id.
fn build_object<'js>(
  ctx: Ctx<'js>,
  id: u64,
  info: &MediaInfo,
  has_audio: bool,
  texture: Option<u64>,
  shared: Arc<Shared>,
) -> Result<Object<'js>, StreamError> {
  let build = || -> rquickjs::Result<Object<'js>> {
    let obj = Object::new(ctx.clone())?;
    if let Some(texture) = texture {
      obj.set("texture", texture)?;
    }
    obj.set("width", info.width)?;
    obj.set("height", info.height)?;
    // None (undefined in JS) when the source has no duration: a live stream.
    obj.set("duration", info.duration_us.map(|d| d as f64 / 1_000_000.0))?;
    obj.set("hasAudio", has_audio)?;
    obj.set("seekable", info.seekable)?;
    obj.set("play", Function::new(ctx.clone(), move |ctx: Ctx<'_>| play_impl(ctx, id))?)?;
    obj.set("pause", Function::new(ctx.clone(), move |ctx: Ctx<'_>| pause_impl(ctx, id))?)?;
    obj.set("seek", Function::new(ctx.clone(), move |ctx: Ctx<'_>, seconds: f64| seek_impl(ctx, id, seconds))?)?;
    obj.set("playing", Function::new(ctx.clone(), move |ctx: Ctx<'_>| playing_impl(ctx, id))?)?;
    obj.set("currentTime", Function::new(ctx.clone(), move |ctx: Ctx<'_>| current_time_impl(ctx, id))?)?;
    obj.set("finished", Function::new(ctx.clone(), move |ctx: Ctx<'_>| finished_impl(ctx, id))?)?;
    obj.set("buffering", Function::new(ctx.clone(), move |ctx: Ctx<'_>| buffering_impl(ctx, id))?)?;
    obj.set("error", Function::new(ctx.clone(), value_builder(move |ctx| error_impl(ctx, id)))?)?;
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
    obj.set("close", Function::new(ctx.clone(), move |ctx: Ctx<'_>| close_impl(ctx, id))?)?;
    Ok(obj)
  };
  build().map_err(|e| StreamError::decode(format!("build player object: {e}")))
}

/// Force the `for<'js>` HRTB on a capturing closure that returns a `'js`-bound
/// `Value` (see flux/CLAUDE.md "Ctx and the 'js lifetime").
fn value_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>) -> rquickjs::Result<Value<'js>>,
{
  f
}

// Run `f` on the entry `id`, None once closed (a late call on a closed
// player is a no-op, not an error).
fn with_entry<T>(ctx: &Ctx<'_>, id: u64, f: impl FnOnce(&Entry) -> T) -> Option<T> {
  let state = state(ctx);
  let players = state.0.players.borrow();
  players.get(&id).map(f)
}

fn play_impl(ctx: Ctx<'_>, id: u64) {
  with_entry(&ctx, id, |e| {
    e.player.play();
    // The texture's standing demand starts with the call, not with the
    // worker's first pass: the frame after play is built at once, and a
    // stepped consumer waits for the first frame instead of capturing
    // without it.
    #[allow(irrefutable_let_patterns)]
    if let Kind::Texture { frames, .. } = &e.kind {
      frames.set_playing(true);
    }
  });
}

fn pause_impl(ctx: Ctx<'_>, id: u64) {
  with_entry(&ctx, id, |e| e.player.pause());
}

fn seek_impl(ctx: Ctx<'_>, id: u64, seconds: f64) {
  with_entry(&ctx, id, |e| e.player.seek((seconds.max(0.0) * 1_000_000.0) as i64));
}

fn playing_impl(ctx: Ctx<'_>, id: u64) -> bool {
  with_entry(&ctx, id, |e| e.player.playing()).unwrap_or(false)
}

// The frame on screen for a texture player (the latch's last take), the
// frame last released for a plane.
fn current_time_impl(ctx: Ctx<'_>, id: u64) -> f64 {
  let position_us = with_entry(&ctx, id, |e| match &e.kind {
    Kind::Texture { frames, .. } => frames.shown_pts_us().unwrap_or(0),
    #[cfg(target_os = "android")]
    Kind::Plane => e.player.position_us(),
  });
  position_us.unwrap_or(0) as f64 / 1_000_000.0
}

// A plane whose surface the platform took (backgrounded) is finished too:
// the codec has nothing to render into.
fn finished_impl(ctx: Ctx<'_>, id: u64) -> bool {
  with_entry(&ctx, id, |e| e.player.finished() || plane_lost(&e.kind)).unwrap_or(true)
}

#[cfg(target_os = "android")]
fn plane_lost(kind: &Kind) -> bool {
  matches!(kind, Kind::Plane) && alloy::video_plane::lost()
}

#[cfg(not(target_os = "android"))]
fn plane_lost(_kind: &Kind) -> bool {
  false
}

fn buffering_impl(ctx: Ctx<'_>, id: u64) -> bool {
  with_entry(&ctx, id, |e| e.player.buffering()).unwrap_or(false)
}

fn error_impl<'js>(ctx: Ctx<'js>, id: u64) -> rquickjs::Result<Value<'js>> {
  match with_entry(&ctx, id, |e| e.player.error()).flatten() {
    Some(error) => video_error(&ctx, &error, ""),
    None => Ok(Value::new_undefined(ctx)),
  }
}

fn close_impl(ctx: Ctx<'_>, id: u64) {
  let state = state(&ctx);
  // Take it out first, then tear down outside the borrow.
  let entry = state.0.players.borrow_mut().remove(&id);
  if let Some(entry) = entry {
    #[cfg(target_os = "android")]
    let plane = matches!(entry.kind, Kind::Plane);
    let closing = close_entry(&state.0.gui, entry);
    #[cfg(target_os = "android")]
    if plane {
      *state.0.closing.borrow_mut() = Some(closing);
    }
    #[cfg(not(target_os = "android"))]
    drop(closing);
  }
}
