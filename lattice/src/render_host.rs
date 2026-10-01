// The render host (okf/done/test-harness.md, D35 and D39): `srt render`
// as a host that steps alloy's stepped mode and reads every frame back. One
// engine, built with the wall taken out (performance.now() at 0, the
// calendar on the fixed epoch, Math.random seeded), so two renders of one
// app write the same frames. Once the entry has evaluated and the window has
// its size, the driver runs in a task of the engine: optionally settles the
// app first (the condition of settle.rs, not a sleep), then for each frame
// injects the scripted input due, demands a frame, steps, waits for it to
// have run and reads the window back, handing the pixels to a writer thread
// that encodes the PNGs. Frame k is the app's state after its (k + 1)th
// frame callback, at (k + 1) / fps.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use flux::rquickjs::Ctx;

use crate::settle::{self, Cap, Unsettled};
use crate::stepped::{Stepper, WindowReady};

/// How long a settle may wait on work in flight before the render gives up
/// on it: the in-flight wait passes no app time, so the app-time cap of
/// settle.rs does not bound it, and a render has no test cap above it.
const SETTLE_WALL_CAP: Duration = Duration::from_secs(30);

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

/// One frame read back, for the writer: its index and pixels.
struct Frame {
  index: u64,
  width: u32,
  height: u32,
  pixels: Vec<u8>,
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

  // The PNGs are encoded off this thread, in order; the driver only hands
  // over pixels and waits for the writer at the end.
  let (tx, rx) = mpsc::channel::<Frame>();
  let prefix = run.output_prefix.clone();
  let writer = std::thread::spawn(move || -> Result<u64, String> {
    let mut written = 0;
    for frame in rx {
      let path = PathBuf::from(format!("{}-{:06}.png", prefix, frame.index));
      let png = forge::image::encode_png(&frame.pixels, frame.width, frame.height, false)?;
      std::fs::write(&path, png).map_err(|e| format!("could not write {}: {e}", path.display()))?;
      written += 1;
    }
    Ok(written)
  });

  let mut asked = 0;
  let mut outcome = Ok(());
  for frame in 0..run.frames {
    if stop.load(Ordering::Relaxed) {
      log::info!("[srt] the app ended the render after {frame} frames");
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
    if tx.send(Frame { index: frame, width, height, pixels }).is_err() {
      break;
    }
    asked += 1;
    if asked % run.fps as u64 == 0 {
      log::info!("[srt] rendered {asked} frames");
    }
  }
  drop(tx);
  let written = writer.join().map_err(|_| "the frame writer panicked".to_string())??;
  outcome?;
  log::info!("[srt] render complete ({written} of {} frames)", run.frames);
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
  match tokio::time::timeout(SETTLE_WALL_CAP, settle::settle(ctx, cap, Some(&step))).await {
    Ok(Ok(())) => {
      log::info!("[srt] the app came to rest after {} ms of app time", stepper.time_ms());
      Ok(())
    }
    Ok(Err(left)) => {
      Err(format!("the app did not come to rest within {} ms of app time: {}", settle::DEFAULT_MAX_MS, left.describe()))
    }
    Err(_) => Err(format!(
      "the app did not come to rest within {} s: {}",
      SETTLE_WALL_CAP.as_secs(),
      Unsettled::read(ctx).describe()
    )),
  }
}
