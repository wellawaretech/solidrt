// The render host (okf/done/test-harness.md, D35 and D39): `sol render`
// as a host that steps alloy's stepped mode and reads every frame back. One
// engine, built with the wall taken out (performance.now() at 0, the
// calendar on the fixed epoch, Math.random seeded), so two renders of one
// app write the same frames. Once the entry has evaluated and the window has
// its size, the driver runs in a task of the engine: optionally settles the
// app first (the condition of settle.rs, not a sleep), then for each frame
// injects the scripted input due, demands a frame, steps, waits for it to
// have run and reads the window back, handing the pixels to the frame
// writers (FrameWriters: a few threads on one bounded queue, each encoding
// and writing PNGs in whatever order it takes them; the file name carries
// the index). The queue bound is what holds the render back when encoding
// falls behind, so a long render holds a fixed number of raw frames. Frame
// k is the app's state after its (k + 1)th frame callback, at (k + 1) / fps.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use flux::rquickjs::Ctx;

use crate::settle::{self, Cap, Unsettled};
use crate::stepped::{Stepper, WindowReady};

/// How long a settle may wait on work in flight before the render gives up
/// on it: the in-flight wait passes no app time, so the app-time cap of
/// settle.rs does not bound it, and a render has no test cap above it.
const SETTLE_WALL_CAP: Duration = Duration::from_secs(30);

/// Frames encoded at once: PNG's filter and deflate run on one thread per
/// frame, and at 4K one thread falls behind the stepped render. Fewer on a
/// machine with fewer cores to spare (see `RENDER_THREADS`).
const PNG_WRITERS: usize = 4;

/// Cores the render itself keeps busy while frames are written: the engine
/// thread and the raster thread. The writers take the rest.
const RENDER_THREADS: usize = 2;

/// Frames read back and waiting for a writer: absorbs the jitter between
/// render and encode. The raw frames a render holds are at most this, plus
/// one per writer, plus the one being handed over.
const QUEUED_FRAMES: usize = 2;

/// What a render is started with (see `start_render`).
pub struct RenderRun {
  pub fps: u32,
  /// Frames written.
  pub frames: u64,
  /// `<prefix>-NNNNNN.png`.
  pub output_prefix: String,
  /// Scripted input replayed against the frame clock; empty for a plain
  /// capture.
  pub script: alloy::ScriptPlayer,
  /// Run the app to rest before the first frame (`--settle`): what loads
  /// asynchronously is in the picture, and a transition the mount starts
  /// has played out.
  pub settle: bool,
  /// The seed `Math.random` runs on.
  pub seed: u64,
}

/// The render's outcome, written by the driver and read once the runtime
/// has wound down. None: the engine ended before the driver did.
pub(crate) type RenderOutcome = Arc<Mutex<Option<Result<(), String>>>>;

/// What the driver shares with the runtime: the rate and the seed the
/// engine is built with, the run (taken by `start`), where to report, and
/// the stop the app's own `exit()` asks for (the frames so far are kept and
/// the run ends in order).
#[derive(Clone)]
pub(crate) struct RenderHost {
  pub(crate) fps: u32,
  pub(crate) seed: u64,
  run: Arc<Mutex<Option<RenderRun>>>,
  pub(crate) outcome: RenderOutcome,
  pub(crate) stop: Arc<AtomicBool>,
}

impl RenderHost {
  pub(crate) fn new(run: RenderRun) -> Self {
    Self {
      fps: run.fps,
      seed: run.seed,
      run: Arc::new(Mutex::new(Some(run))),
      outcome: Arc::new(Mutex::new(None)),
      stop: Arc::new(AtomicBool::new(false)),
    }
  }

  /// Start the driver in a task of `ctx`'s engine (the entry has evaluated).
  /// `done` is called with the outcome recorded, from the task.
  pub(crate) fn start(&self, ctx: &Ctx<'_>, done: impl FnOnce() + 'static) {
    let Some(run) = self.run.lock().expect("render run lock poisoned").take() else {
      return;
    };
    let host = self.clone();
    let task_ctx = ctx.clone();
    ctx.spawn(async move {
      let result = drive(&task_ctx, run, &host.stop).await;
      *host.outcome.lock().expect("render outcome lock poisoned") = Some(result);
      done();
    });
  }
}

/// One frame read back, for the writers: its index and pixels.
pub(crate) struct Frame {
  pub(crate) index: u64,
  pub(crate) width: u32,
  pub(crate) height: u32,
  pub(crate) pixels: Vec<u8>,
}

/// What the writer threads share: the queue they take frames from, and the
/// first write that failed, which stops the driver at its next frame.
struct Shared {
  frames: Mutex<tokio::sync::mpsc::Receiver<Frame>>,
  failed: OnceLock<String>,
}

/// The frame writers of one render: `PNG_WRITERS` threads (fewer on a small
/// machine) on one queue of `QUEUED_FRAMES`. `send` waits while the queue is
/// full, so the driver renders no further ahead than the bound; `finish`
/// drains the queue and reports the frames written or the first failure.
pub(crate) struct FrameWriters {
  queue: tokio::sync::mpsc::Sender<Frame>,
  shared: Arc<Shared>,
  threads: Vec<JoinHandle<u64>>,
}

impl FrameWriters {
  /// Start the writers for `<prefix>-NNNNNN.png`. The prefix's directory is
  /// created first: an output path is a request to write there, and a
  /// render into a directory nobody can create fails here, before a frame
  /// is rendered.
  pub(crate) fn start(prefix: &str) -> Result<Self, String> {
    if let Some(dir) = Path::new(prefix).parent().filter(|dir| !dir.as_os_str().is_empty()) {
      std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let (queue, frames) = tokio::sync::mpsc::channel::<Frame>(QUEUED_FRAMES);
    let shared = Arc::new(Shared { frames: Mutex::new(frames), failed: OnceLock::new() });
    let cores = std::thread::available_parallelism().map(|cores| cores.get()).unwrap_or(PNG_WRITERS);
    let threads = (0..PNG_WRITERS.min(cores.saturating_sub(RENDER_THREADS)).max(1))
      .map(|index| {
        let shared = shared.clone();
        let prefix = prefix.to_string();
        std::thread::Builder::new()
          .name(format!("png-writer-{index}"))
          .spawn(move || write_frames(&shared, &prefix))
          .map_err(|e| format!("could not start the frame writer {index}: {e}"))
      })
      .collect::<Result<Vec<_>, _>>()?;
    Ok(Self { queue, shared, threads })
  }

  /// Hand a frame over, waiting for room in the queue. False when the
  /// writers are gone (every one of them failed or panicked).
  pub(crate) async fn send(&self, frame: Frame) -> bool {
    self.queue.send(frame).await.is_ok()
  }

  /// The first write that failed, once one has.
  pub(crate) fn failed(&self) -> Option<&str> {
    self.shared.failed.get().map(String::as_str)
  }

  /// Close the queue, let the writers drain it and join them: the frames
  /// written, or the first failure.
  pub(crate) fn finish(self) -> Result<u64, String> {
    drop(self.queue);
    let mut written = 0;
    for thread in self.threads {
      written += thread.join().map_err(|_| "a frame writer panicked".to_string())?;
    }
    match self.shared.failed.get() {
      Some(failed) => Err(failed.clone()),
      None => Ok(written),
    }
  }
}

/// One writer thread: encode and write frames until the queue closes or a
/// write fails. Returns the frames it wrote.
fn write_frames(shared: &Shared, prefix: &str) -> u64 {
  let mut written = 0;
  loop {
    // The lock is held for the wait and released at the end of the
    // statement, before the encode, so the other writers take the next
    // frames meanwhile (a `while let` would hold it through the body). A
    // poisoned lock means a writer panicked mid-wait; this one ends too.
    let Ok(next) = shared.frames.lock().map(|mut frames| frames.blocking_recv()) else {
      return written;
    };
    let Some(frame) = next else {
      return written;
    };
    let path = PathBuf::from(format!("{prefix}-{:06}.png", frame.index));
    match forge::image::write_png(&path, &frame.pixels, frame.width, frame.height, false) {
      Ok(()) => written += 1,
      Err(e) => {
        let _ = shared.failed.set(e);
        return written;
      }
    }
  }
}

async fn drive(ctx: &Ctx<'_>, mut run: RenderRun, stop: &AtomicBool) -> Result<(), String> {
  let stepper = ctx.userdata::<Stepper>().ok_or_else(|| "the render engine has no stepper".to_string())?.clone();
  let ready = ctx.userdata::<WindowReady>().ok_or_else(|| "the render engine has no window state".to_string())?.clone();
  let alloy = flux::gui::alloy_context(ctx).ok_or_else(|| "the render engine has no window".to_string())?;
  // The window builds its first frame on its size, which reaches the engine
  // through two threads.
  let _ = ready.wait().await;
  let mounted = flux::gui::tree::with_tree(ctx, |tree| tree.snapshot_from(None, Some(0)).is_some()).unwrap_or(false);
  if !mounted {
    return Err("the app rendered no window; its top level has to call render()".to_string());
  }
  if run.settle {
    settle_first(ctx, &stepper).await?;
  }

  // The PNGs are encoded off this thread; the driver hands over pixels,
  // waits when the writers are behind, and joins them at the end.
  let writers = FrameWriters::start(&run.output_prefix)?;

  let mut asked = 0;
  let mut outcome = Ok(());
  for frame in 0..run.frames {
    if stop.load(Ordering::Relaxed) {
      log::info!("[sol] the app ended the render after {frame} frames");
      break;
    }
    // A failed write ends the render here, one frame on; `finish` reports
    // it.
    if writers.failed().is_some() {
      break;
    }
    // Scripted input due for this frame reaches the UI loop ahead of the
    // frame signal: same channel, so send order is receive order.
    for event in run.script.due((frame + 1) as f64 / run.fps as f64) {
      stepper.inject(event);
    }
    // The host demands the frame, as a display does: the gate passes and
    // the frame draws whether or not the app changed anything.
    flux::gui::request_frame(ctx);
    let ran = settle::next_frame(ctx);
    stepper.step();
    let _ = ran.await;
    let (width, height, pixels) = match alloy.read_window() {
      Ok(read) => read,
      Err(e) => {
        outcome = Err(format!("frame {frame} could not be read back: {e}"));
        break;
      }
    };
    if !writers.send(Frame { index: frame, width, height, pixels }).await {
      break;
    }
    asked += 1;
    if asked % run.fps as u64 == 0 {
      log::info!("[sol] rendered {asked} frames");
    }
  }
  let written = writers.finish()?;
  outcome?;
  log::info!("[sol] render complete ({written} of {} frames)", run.frames);
  // An incomplete render means the app failed to produce some frame; the
  // exit code is what a headless caller gates on, so only a full render
  // reads as success. An app that ended the run itself is the exception:
  // the frame budget is only an upper bound for it.
  if written < run.frames && !stop.load(Ordering::Relaxed) {
    return Err(format!("only {written} of {} frames were written", run.frames));
  }
  Ok(())
}

/// Run the app to rest before the first written frame: demanded frames run
/// (a transition plays out), work in flight is waited for, with the app-time
/// cap of a test and a wall bound on the in-flight wait.
async fn settle_first(ctx: &Ctx<'_>, stepper: &Stepper) -> Result<(), String> {
  let now_ms = || stepper.time_ms();
  let step = || stepper.step();
  let cap = Cap::AppTime { max_ms: settle::DEFAULT_MAX_MS, now_ms: &now_ms };
  match tokio::time::timeout(SETTLE_WALL_CAP, settle::settle(ctx, settle::Until::Rest, cap, Some(&step))).await {
    Ok(Ok(())) => {
      log::info!("[sol] the app came to rest after {} ms of app time", stepper.time_ms());
      Ok(())
    }
    Ok(Err(settle::Stopped::Cap(left))) => {
      Err(format!("the app did not come to rest within {} ms of app time: {}", settle::DEFAULT_MAX_MS, left.describe()))
    }
    // A rest wait reads no condition of its own.
    Ok(Err(settle::Stopped::Read)) => Err("the rest wait failed its read".to_string()),
    Err(_) => Err(format!(
      "the app did not come to rest within {} s: {}",
      SETTLE_WALL_CAP.as_secs(),
      Unsettled::read(ctx).describe()
    )),
  }
}
