//! JS bindings for recording, `record` of `flux:video`: a thin layer over
//! forge::video::record (the encoders, the WebM writer, the recorder's
//! thread) and alloy's taps (the camera's frames, the window tap's frames,
//! the microphone's ring). Two feeder threads per recording carry the taps'
//! plain values (an RGBA frame with its SDL tick timestamp, a block of PCM
//! with the time of its first sample on the same ticks) to the recorder's
//! channel; the JS thread only opens, pauses, resumes and stops. A
//! recording is standing work in flight until the app stops it. Engine
//! teardown stops every recording the app left open and hands the join to
//! the host.
//!
//! A node source (`{ node }`) records the window: the whole of it for the
//! window node, else the node's painted box cut out of the frame as it is
//! on screen, through a window tap alloy reads back after each drawn frame.
//! The box is re-read every frame (`track_rects`) and the tap's cut-out
//! moved when it changed; a node with no box (hidden, not laid out) records
//! nothing until it has one again.
//!
//! The live form (`to` omitted): the container goes into a bounded channel
//! instead of a file, in the writer's stream form under the live encoder
//! settings, and the handle's `readable` pulls it chunk by chunk (one per
//! block). A consumer that stops pulling blocks the recorder within the
//! channel, and the feeders drop frames meanwhile; a consumer that goes
//! away fails the recorder's next write, which ends the recording.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use alloy::camera::{CameraFrame, FrameTap, TapWait};
use alloy::microphone::{MicWait, MicrophoneTap};
use alloy::rendertree::{ElementKind, RenderTree};
use alloy::{TapRect, TapSize};
use forge::audio::OpusApplication;
use forge::stream::ByteStream;
use forge::video::record::{
  default_video_bitrate, DEFAULT_AUDIO_BITRATE_BPS, DEFAULT_FRAME_RATE, KEYFRAME_INTERVAL_US,
};
use forge::video::{
  live_sink, AudioConfig, EncodeMode, EncoderConfig, Feeder, LiveSink, Push, Recorder, RecorderConfig, RecorderShared,
  StreamError, WebmSink,
};
use rquickjs::promise::Promise;
use rquickjs::{Ctx, Exception, Function, JsLifetime, Object, Value};

use super::video::video_error;
use crate::pending::{Hold, PendingOps};
use crate::plugins::marshal::OptArg;
use crate::standards_plugins::body::byte_stream_readable;

// How long a feeder waits on its tap before looking at the stop flag.
const FEEDER_WAIT: Duration = Duration::from_millis(100);
// How long a feeder waits at the stop for the frame still in flight: a
// window tap's last readback lands at the next frame (or after the raster
// loop's idle timeout) and is delivered before the tap closes.
const STOP_DRAIN: Duration = Duration::from_millis(100);
// The audio feeder forwards once this much is buffered: one Opus frame,
// so the recorder never waits on a half frame.
const AUDIO_BLOCK_MS: u32 = 20;
const NS_PER_S: u64 = 1_000_000_000;

/// A recording's window tap: the tap's id, the node whose box it cuts out
/// (None for the window node: the whole window, never moved) and the rect
/// last sent, so a frame whose box did not move sends nothing.
struct Screen {
  tap: u64,
  node: Option<u64>,
  rect: Option<TapRect>,
}

struct Entry {
  recorder: Option<Recorder>,
  stop: Arc<AtomicBool>,
  feeders: Vec<JoinHandle<()>>,
  paused: bool,
  screen: Option<Screen>,
  // The live form's sink, closed ahead of the stop so a recorder blocked
  // on a consumer that stopped pulling can be stopped.
  live: Option<LiveSink>,
  // The recording is standing work: the engine is not at rest while one
  // runs (the app ends it).
  _hold: Hold,
}

struct Inner {
  gui: Rc<super::Gui>,
  recordings: RefCell<HashMap<u64, Entry>>,
  next_id: Cell<u64>,
  // Whether the per-frame rect tracking is registered (once per engine, at
  // the first node recording; an app that never records pays nothing).
  tracking: Cell<bool>,
}

impl Drop for Inner {
  // Engine teardown: stop every recording the app left open; the join (the
  // feeders, the encoder flush, the file's close) is the host's to await,
  // bounded, before the next engine.
  fn drop(&mut self) {
    for (_, mut entry) in self.recordings.get_mut().drain() {
      entry.stop.store(true, Ordering::Relaxed);
      if let Some(live) = entry.live.take() {
        live.close();
      }
      if let Some(screen) = entry.screen.take() {
        self.gui.alloy.close_window_tap(screen.tap);
      }
      let feeders = std::mem::take(&mut entry.feeders);
      let recorder = entry.recorder.take();
      self.gui.teardown.defer(async move {
        let _ = tokio::task::spawn_blocking(move || end(feeders, recorder)).await;
      });
    }
  }
}

/// What a recording takes its picture from.
enum VideoSource {
  /// A camera session, by the texture it streams into.
  Camera(u64),
  /// A render tree node: the window node, or a node whose box is cut out
  /// of the window.
  Node(u64),
}

/// The rect of `node`'s painted box in device pixels (the box in window
/// points times the display scale, outset to whole pixels), or an empty
/// rect when the node has no box now (hidden, or not laid out), which
/// records nothing until it has one.
fn node_rect(tree: &RenderTree, node: u64, scale: f32) -> TapRect {
  let Some(bbox) = tree.bounding_box_viewport(node) else {
    return TapRect { x: 0, y: 0, width: 0, height: 0 };
  };
  let x0 = (bbox.origin.x * scale).floor().max(0.0);
  let y0 = (bbox.origin.y * scale).floor().max(0.0);
  let x1 = ((bbox.origin.x + bbox.size.width) * scale).ceil().max(x0);
  let y1 = ((bbox.origin.y + bbox.size.height) * scale).ceil().max(y0);
  TapRect { x: x0 as u32, y: y0 as u32, width: (x1 - x0) as u32, height: (y1 - y0) as u32 }
}

/// The per-frame tracking of every cut-out recording's box: run in
/// `frame::advance`, so a box that moved last frame moves the cut-out for
/// this frame's readback. Never asks for a frame: a window at rest records
/// nothing, by design.
fn track_rects(ctx: &Ctx<'_>) {
  let state = state(ctx);
  let Some(tree_state) = super::tree::try_state(ctx) else { return };
  let scale = state.0.gui.platform.display_scale();
  let tree = tree_state.tree.borrow();
  for entry in state.0.recordings.borrow_mut().values_mut() {
    let Some(screen) = entry.screen.as_mut() else { continue };
    let Some(node) = screen.node else { continue };
    let rect = Some(node_rect(&tree, node, scale));
    if rect != screen.rect {
      screen.rect = rect;
      state.0.gui.alloy.set_window_tap_rect(screen.tap, rect);
    }
  }
}

#[derive(Clone, JsLifetime)]
struct RecordPluginState(#[qjs(skip_trace)] Rc<Inner>);

/// Store the record plugin state in userdata. Runs at engine init, before
/// any module import; `record` of `flux:video` reads it.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx
    .store_userdata(RecordPluginState(Rc::new(Inner {
      gui: super::gui(ctx),
      recordings: RefCell::new(HashMap::new()),
      next_id: Cell::new(0),
      tracking: Cell::new(false),
    })))
    .expect("store record state");
}

fn state(ctx: &Ctx<'_>) -> RecordPluginState {
  ctx.userdata::<RecordPluginState>().expect("record state").clone()
}

/// Join the feeders and stop the recorder: the blocking end of a
/// recording, run off the JS thread.
fn end(feeders: Vec<JoinHandle<()>>, recorder: Option<Recorder>) -> Result<(), StreamError> {
  for feeder in feeders {
    let _ = feeder.join();
  }
  match recorder {
    Some(recorder) => recorder.stop(),
    None => Ok(()),
  }
}

/// A positive finite number option, or its default.
fn read_rate<'js>(ctx: &Ctx<'js>, opts: &Object<'js>, key: &str, default: u32) -> rquickjs::Result<u32> {
  match opts.get::<_, Option<f64>>(key)? {
    None => Ok(default),
    Some(v) if v.is_finite() && v > 0.0 && v <= u32::MAX as f64 => Ok(v as u32),
    Some(v) => Err(Exception::throw_type(ctx, &format!("record: {key} must be a positive number, got {v}"))),
  }
}

/// Where a recording's bytes go: a file, or the live form's channel (the
/// consumer's end becomes the handle's `readable`).
enum Destination {
  File(String),
  Live,
}

/// record(sources, options): a malformed argument throws; a source that is
/// not open, an encoder this build lacks or a destination that cannot be
/// written rejects with a VideoError.
pub(super) fn record_impl<'js>(
  ctx: Ctx<'js>,
  sources: Object<'js>,
  options: OptArg<Object<'js>>,
) -> rquickjs::Result<Promise<'js>> {
  let video_source = match sources.get::<_, Option<Object>>("video")? {
    Some(obj) => match (obj.get::<_, Option<u64>>("texture")?, obj.get::<_, Option<u64>>("node")?) {
      (Some(texture), None) => Some(VideoSource::Camera(texture)),
      (None, Some(node)) => Some(VideoSource::Node(node)),
      _ => {
        return Err(Exception::throw_type(
          &ctx,
          "record: video must be a camera session (an object with its texture id) or a node ({ node: id })",
        ))
      }
    },
    None => None,
  };
  let audio_id = match sources.get::<_, Option<Object>>("audio")? {
    Some(obj) => Some(obj.get::<_, Option<u64>>("id")?.ok_or_else(|| {
      Exception::throw_type(&ctx, "record: audio must be a microphone session (an object with its id)")
    })?),
    None => None,
  };
  if video_source.is_none() && audio_id.is_none() {
    return Err(Exception::throw_type(&ctx, "record: pass at least one source, { video } or { audio }"));
  }
  let opts = options.0.unwrap_or(Object::new(ctx.clone())?);
  let destination = match opts.get::<_, Option<Object>>("to")? {
    Some(to) => Destination::File(
      to.get::<_, Option<String>>("path")
        .ok()
        .flatten()
        .ok_or_else(|| Exception::throw_type(&ctx, "record: to must be a file() from flux:fs"))?,
    ),
    None => Destination::Live,
  };
  let live = matches!(destination, Destination::Live);
  let frame_rate = read_rate(&ctx, &opts, "frameRate", DEFAULT_FRAME_RATE)?;
  let size = TapSize { width: read_dimension(&ctx, &opts, "width")?, height: read_dimension(&ctx, &opts, "height")? };
  let video_bitrate = opts.get::<_, Option<f64>>("videoBitrate")?;
  let audio_bitrate = read_rate(&ctx, &opts, "audioBitrate", DEFAULT_AUDIO_BITRATE_BPS)?;
  if video_bitrate.is_some_and(|v| !v.is_finite() || v <= 0.0) {
    return Err(Exception::throw_type(&ctx, "record: videoBitrate must be a positive number"));
  }

  let (promise, resolve, reject) = Promise::new(&ctx)?;
  let state = state(&ctx);
  let gui = state.0.gui.clone();
  let refuse = |error: StreamError| -> rquickjs::Result<()> {
    let err = video_error(&ctx, &error, "record: ")?;
    reject.call::<_, ()>((err,))
  };

  // The sources: environmental, so a missing one rejects.
  let video = match video_source {
    Some(VideoSource::Camera(texture)) => match gui.alloy.camera_frame_tap(texture) {
      Some(tap) => match tap.latest() {
        Some(frame) => {
          let asked = (size.width.unwrap_or(frame.width), size.height.unwrap_or(frame.height));
          if asked != (frame.width, frame.height) {
            refuse(StreamError::unsupported(format!(
              "a camera records at its open size ({}x{}); open it at the size wanted",
              frame.width, frame.height
            )))?;
            return Ok(promise);
          }
          Some((tap, frame.width, frame.height, None))
        }
        None => {
          refuse(StreamError::unsupported("the camera has no frame yet"))?;
          return Ok(promise);
        }
      },
      None => {
        refuse(StreamError::unsupported(format!("no open camera streams into texture {texture}")))?;
        return Ok(promise);
      }
    },
    Some(VideoSource::Node(node)) => match open_screen(&ctx, &state, node, size) {
      Ok((tap, screen)) => Some((tap.tap, tap.width, tap.height, Some(screen))),
      Err(e) => {
        refuse(e)?;
        return Ok(promise);
      }
    },
    None => None,
  };
  let audio = match audio_id {
    Some(id) => match gui.alloy.microphone_tap(id) {
      Ok(tap) => Some(tap),
      Err(e) => {
        refuse(StreamError::unsupported(format!("microphone {id}: {e}")))?;
        return Ok(promise);
      }
    },
    None => None,
  };

  let video_config = video.as_ref().map(|(_, width, height, _)| EncoderConfig {
    width: *width,
    height: *height,
    frame_rate,
    bitrate_bps: video_bitrate.map_or_else(|| default_video_bitrate(*width, *height, frame_rate), |v| v as u32),
    keyframe_interval_us: KEYFRAME_INTERVAL_US,
    mode: if live { EncodeMode::Live } else { EncodeMode::Recording },
  });
  let audio_config = audio.as_ref().map(|tap| AudioConfig {
    sample_rate: tap.sample_rate(),
    channels: 1,
    bitrate_bps: audio_bitrate,
    application: if live { OpusApplication::Voip } else { OpusApplication::Audio },
  });

  // The open off the JS thread: the file and the encoders. The live form's
  // output stream is made here and handed to the recording's object.
  let (sink, output): (Option<WebmSink>, Option<(ByteStream, LiveSink)>) = match &destination {
    Destination::File(_) => (None, None),
    Destination::Live => {
      let (sink, stream, live) = live_sink();
      (Some(sink), Some((stream, live)))
    }
  };
  let hold = PendingOps::of(&ctx).in_flight("video record open");
  let task_ctx = ctx.clone();
  ctx.spawn(async move {
    let opened = tokio::task::spawn_blocking(move || -> Result<Recorder, StreamError> {
      let sink = match (destination, sink) {
        (Destination::File(path), _) => {
          WebmSink::File(Box::new(forge::fs::create_file(&path).map_err(StreamError::network)?))
        }
        (Destination::Live, Some(sink)) => sink,
        (Destination::Live, None) => return Err(StreamError::decode("the live sink was not made")),
      };
      Recorder::open(RecorderConfig { video: video_config, audio: audio_config, sink })
    })
    .await
    .unwrap_or_else(|e| Err(StreamError::decode(format!("the open task failed: {e}"))));
    drop(hold);
    let ctx = task_ctx;
    let (video_tap, screen) = match video {
      Some((tap, _, _, screen)) => (Some(tap), screen),
      None => (None, None),
    };
    let settled = match opened {
      Ok(recorder) => match start(ctx.clone(), &state, recorder, video_tap, screen, audio, output) {
        Ok(obj) => resolve.call::<_, ()>((obj,)),
        Err(e) => video_error(&ctx, &e, "record: ").and_then(|err| reject.call::<_, ()>((err,))),
      },
      Err(e) => {
        // The tap opened for this recording has no recording to serve.
        if let Some(screen) = screen {
          state.0.gui.alloy.close_window_tap(screen.tap);
        }
        video_error(&ctx, &e, "record: ").and_then(|err| reject.call::<_, ()>((err,)))
      }
    };
    if let Err(e) = settled {
      log::warn!("[video] settle record: {e}");
    }
  });
  Ok(promise)
}

/// A window tap for `node`: the whole window for the window node, else the
/// node's box cut out, at `size`. Errs (unsupported) for a node not in the
/// tree or without a box, and when the window has no surface.
fn open_screen(
  ctx: &Ctx<'_>,
  state: &RecordPluginState,
  node: u64,
  size: TapSize,
) -> Result<(alloy::WindowTap, Screen), StreamError> {
  let tree_state = super::tree::try_state(ctx).ok_or_else(|| StreamError::unsupported("no render tree to record"))?;
  let tree = tree_state.tree.borrow();
  let element =
    tree.try_node(node).ok_or_else(|| StreamError::unsupported(format!("node {node} is not in the render tree")))?;
  let is_window = matches!(element.kind, ElementKind::Window(_));
  let rect = if is_window {
    None
  } else {
    let rect = node_rect(&tree, node, state.0.gui.platform.display_scale());
    if rect.width == 0 || rect.height == 0 {
      return Err(StreamError::unsupported(format!("node {node} has no painted box (not laid out, or hidden)")));
    }
    Some(rect)
  };
  drop(tree);
  let tap = state.0.gui.alloy.open_window_tap(rect, size).map_err(StreamError::unsupported)?;
  // The clip starts with the screen as it is: one frame demanded, so a
  // window at rest still records its current picture.
  state.0.gui.platform.request_frame();
  if !is_window && !state.0.tracking.replace(true) {
    super::frame::on_advance(ctx, "a screen recording", |ctx, _| {
      track_rects(ctx);
      false
    });
  }
  let screen = Screen { tap: tap.id, node: (!is_window).then_some(node), rect };
  Ok((tap, screen))
}

/// A positive whole-number dimension option, or None.
fn read_dimension<'js>(ctx: &Ctx<'js>, opts: &Object<'js>, key: &str) -> rquickjs::Result<Option<u32>> {
  match opts.get::<_, Option<f64>>(key)? {
    None => Ok(None),
    Some(v) if v.is_finite() && v >= 1.0 && v <= u32::MAX as f64 => Ok(Some(v as u32)),
    Some(v) => Err(Exception::throw_type(ctx, &format!("record: {key} must be a positive number, got {v}"))),
  }
}

/// The feeders over the taps, the entry, and the bound handle.
fn start<'js>(
  ctx: Ctx<'js>,
  state: &RecordPluginState,
  recorder: Recorder,
  video: Option<FrameTap>,
  screen: Option<Screen>,
  audio: Option<MicrophoneTap>,
  output: Option<(ByteStream, LiveSink)>,
) -> Result<Object<'js>, StreamError> {
  let stop = Arc::new(AtomicBool::new(false));
  let mut feeders = Vec::new();
  if let Some(tap) = video {
    let (feeder, stop) = (recorder.feeder(), stop.clone());
    feeders.push(spawn_feeder("sol-record-video", move || video_feeder(tap, feeder, stop))?);
  }
  if let Some(tap) = audio {
    let (feeder, stop) = (recorder.feeder(), stop.clone());
    feeders.push(spawn_feeder("sol-record-audio", move || audio_feeder(tap, feeder, stop))?);
  }
  let shared = recorder.shared();
  let hold = PendingOps::of(&ctx).standing("video record");
  let id = state.0.next_id.get() + 1;
  state.0.next_id.set(id);
  state.0.recordings.borrow_mut().insert(
    id,
    Entry {
      recorder: Some(recorder),
      stop,
      feeders,
      paused: false,
      screen,
      live: output.as_ref().map(|(_, live)| live.clone()),
      _hold: hold,
    },
  );
  build_object(ctx, id, shared, output.map(|(stream, _)| stream))
    .map_err(|e| StreamError::decode(format!("build recording object: {e}")))
}

fn spawn_feeder(name: &str, body: impl FnOnce() + Send + 'static) -> Result<JoinHandle<()>, StreamError> {
  std::thread::Builder::new()
    .name(name.to_string())
    .spawn(body)
    .map_err(|e| StreamError::unsupported(format!("spawn {name}: {e}")))
}

/// The tap's frame handed to the recorder as it is: the pixels stay in the
/// tap's buffer, shared with the raster upload, and the tap allocates the
/// next frame only while the recorder still holds this one (one encode's
/// worth, a few ms; the tap keeps spares for exactly that).
struct TapFrame(Arc<CameraFrame>);

impl AsRef<[u8]> for TapFrame {
  fn as_ref(&self) -> &[u8] {
    &self.0.pixels
  }
}

/// Every frame the tap (a camera's, or the window's) publishes, newest
/// first, until the tap closes or the recording stops, plus the one frame
/// in flight at the stop (a window tap delivers its last readback before
/// it closes; a camera may publish once more), so the clip ends on the
/// last frame drawn. A frame the recorder has no room for is lost, not
/// queued (the clip keeps real time).
fn video_feeder(tap: FrameTap, feeder: Feeder, stop: Arc<AtomicBool>) {
  // Taken and dropped counts, and the newest seq seen.
  struct Counts {
    after: u64,
    taken: u64,
    dropped: u64,
  }
  // Hand a frame to the recorder; false once the recorder is closed.
  fn take(frame: Arc<CameraFrame>, feeder: &Feeder, counts: &mut Counts) -> bool {
    counts.after = frame.seq;
    let (width, height, timestamp_ns) = (frame.width, frame.height, frame.timestamp_ns);
    match feeder.push_video(TapFrame(frame), width, height, timestamp_ns) {
      Push::Queued => counts.taken += 1,
      Push::Dropped => counts.dropped += 1,
      Push::Closed => return false,
    }
    true
  }
  let mut counts = Counts { after: 0, taken: 0, dropped: 0 };
  let mut open = true;
  while open && !stop.load(Ordering::Relaxed) {
    match tap.wait_newer(counts.after, FEEDER_WAIT) {
      TapWait::Frame(frame) => open = take(frame, &feeder, &mut counts),
      TapWait::Timeout => {}
      TapWait::Closed => open = false,
    }
  }
  if open {
    if let TapWait::Frame(frame) = tap.wait_newer(counts.after, STOP_DRAIN) {
      take(frame, &feeder, &mut counts);
    }
  }
  let (taken, dropped) = (counts.taken, counts.dropped);
  if dropped > 0 {
    log::warn!("[video] recording dropped {dropped} of {} frames (the encoder fell behind)", taken + dropped);
  }
}

/// The ring's samples in blocks of one Opus frame, each stamped with its
/// first sample's time on the SDL ticks (the open plus the position).
fn audio_feeder(mut tap: MicrophoneTap, feeder: Feeder, stop: Arc<AtomicBool>) {
  let rate = tap.sample_rate().max(1);
  let block = (rate * AUDIO_BLOCK_MS / 1000) as usize;
  let opened_ns = tap.opened_ns();
  let mut buf = Vec::new();
  while !stop.load(Ordering::Relaxed) {
    match tap.wait(block, FEEDER_WAIT) {
      MicWait::Samples(_) => {
        buf.clear();
        let Ok(read) = tap.read_into(&mut buf) else { break };
        if read.dropped > 0 {
          log::warn!("[video] recording lost {} microphone samples (the recorder fell behind)", read.dropped);
        }
        let timestamp_ns = opened_ns + read.position * NS_PER_S / rate as u64;
        if feeder.push_audio(buf.clone(), timestamp_ns).is_err() {
          break;
        }
      }
      MicWait::Timeout => {}
      MicWait::Closed => break,
    }
  }
}

fn build_object<'js>(
  ctx: Ctx<'js>,
  id: u64,
  shared: Arc<RecorderShared>,
  output: Option<ByteStream>,
) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  if let Some(output) = output {
    // A read waiting on the recorder lasts until the app stops it: standing.
    let pending = PendingOps::of(&ctx);
    obj.set("readable", byte_stream_readable(&ctx, output, move || pending.standing("video record read"))?)?;
  }
  obj.set("recording", Function::new(ctx.clone(), move |ctx: Ctx<'_>| recording_impl(ctx, id))?)?;
  let duration = shared.clone();
  obj.set("duration", Function::new(ctx.clone(), move || duration.duration_us() as f64 / 1_000_000.0)?)?;
  obj.set("pause", Function::new(ctx.clone(), move |ctx: Ctx<'_>| set_paused_impl(ctx, id, true))?)?;
  obj.set("resume", Function::new(ctx.clone(), move |ctx: Ctx<'_>| set_paused_impl(ctx, id, false))?)?;
  obj.set("stop", Function::new(ctx.clone(), promise_builder(move |ctx| stop_impl(ctx, id)))?)?;
  let error = shared;
  obj.set("error", Function::new(ctx.clone(), value_builder(move |ctx| error_impl(ctx, &error)))?)?;
  Ok(obj)
}

/// Force the `for<'js>` HRTB on a capturing closure that returns a
/// `'js`-bound `Value` (see flux/CLAUDE.md "Ctx and the 'js lifetime").
fn value_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>) -> rquickjs::Result<Value<'js>>,
{
  f
}

/// The same coercion for a closure returning a `'js`-bound `Promise`.
fn promise_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>) -> rquickjs::Result<Promise<'js>>,
{
  f
}

fn recording_impl(ctx: Ctx<'_>, id: u64) -> bool {
  let state = state(&ctx);
  let recordings = state.0.recordings.borrow();
  recordings.get(&id).is_some_and(|e| !e.paused && e.recorder.as_ref().is_some_and(|r| r.shared().error().is_none()))
}

fn set_paused_impl(ctx: Ctx<'_>, id: u64, paused: bool) {
  let state = state(&ctx);
  let mut recordings = state.0.recordings.borrow_mut();
  let Some(entry) = recordings.get_mut(&id) else { return };
  if entry.paused == paused {
    return;
  }
  entry.paused = paused;
  if let Some(recorder) = &entry.recorder {
    let now_ns = alloy::sdl_utils::ticks_ns();
    if paused {
      recorder.pause(now_ns);
    } else {
      recorder.resume(now_ns);
    }
  }
}

/// stop(): the feeders end, the recorder flushes and the file is finished,
/// off the JS thread; resolves when the file is final, rejects with the
/// failure that stopped the recording. A second stop resolves at once.
fn stop_impl<'js>(ctx: Ctx<'js>, id: u64) -> rquickjs::Result<Promise<'js>> {
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  let state = state(&ctx);
  let Some(mut entry) = state.0.recordings.borrow_mut().remove(&id) else {
    resolve.call::<_, ()>((Value::new_undefined(ctx.clone()),))?;
    return Ok(promise);
  };
  entry.stop.store(true, Ordering::Relaxed);
  if let Some(live) = entry.live.take() {
    live.close();
  }
  // The tap's frame in flight is delivered before it closes, so the clip
  // ends on the last frame drawn.
  if let Some(screen) = entry.screen.take() {
    state.0.gui.alloy.close_window_tap(screen.tap);
  }
  let feeders = std::mem::take(&mut entry.feeders);
  let recorder = entry.recorder.take();
  drop(entry);
  let hold = PendingOps::of(&ctx).in_flight("video record stop");
  let task_ctx = ctx.clone();
  ctx.spawn(async move {
    let ended = tokio::task::spawn_blocking(move || end(feeders, recorder))
      .await
      .unwrap_or_else(|e| Err(StreamError::decode(format!("the stop task failed: {e}"))));
    drop(hold);
    let ctx = task_ctx;
    let settled = match ended {
      Ok(()) => resolve.call::<_, ()>((Value::new_undefined(ctx.clone()),)),
      Err(e) => video_error(&ctx, &e, "record: ").and_then(|err| reject.call::<_, ()>((err,))),
    };
    if let Err(e) = settled {
      log::warn!("[video] settle stop: {e}");
    }
  });
  Ok(promise)
}

fn error_impl<'js>(ctx: Ctx<'js>, shared: &RecorderShared) -> rquickjs::Result<Value<'js>> {
  match shared.error() {
    Some(error) => video_error(&ctx, &error, ""),
    None => Ok(Value::new_undefined(ctx)),
  }
}
