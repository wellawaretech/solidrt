mod frame;
#[cfg(feature = "go")]
mod go;
mod overlay;
mod paced_clock;
mod plugins;
mod runtime;
#[cfg(feature = "speech")]
pub mod speech;

#[cfg_attr(not(feature = "go"), allow(dead_code))]
enum EngineCmd {
  Stop,
  Reload(String),
}

use alloy::impellers::ISize;
use alloy::rendertree::{PlatformContext, RenderTree};
use alloy::AlloyEvent;
use flux::gui::AlloyContext;
use flux::{ExecHandle, FluxEngine};
use frame::InputState;
use runtime::UiRuntime;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// --- Start Android entry point ------------------------------

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn SDL_main(argc: i32, argv: *mut *mut i8) -> i32 {
  let dev_server = parse_dev_server_arg(argc, argv);
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
  start(&rt, None, alloy::Mode::Run, (1280, 720), false, dev_server, None);
  0
}

// Pull `--dev-server <addr>` out of the C argv SDL hands SDL_main (populated from
// MainActivity.getArguments). The address is the dev server the go client should
// auto-dial; None when launched without it (e.g. tapping the app icon).
#[cfg(target_os = "android")]
fn parse_dev_server_arg(argc: i32, argv: *mut *mut i8) -> Option<String> {
  if argv.is_null() || argc <= 0 {
    return None;
  }
  let args: Vec<String> = (0..argc as isize)
    .filter_map(|i| {
      let ptr = unsafe { *argv.offset(i) };
      if ptr.is_null() {
        return None;
      }
      // c_char is u8 on Android ARM, i8 elsewhere; cast so this builds on both.
      unsafe { std::ffi::CStr::from_ptr(ptr as *const std::ffi::c_char) }.to_str().ok().map(str::to_owned)
    })
    .collect();
  let mut it = args.iter();
  while let Some(arg) = it.next() {
    if arg == "--dev-server" {
      return it.next().cloned();
    }
  }
  None
}

// Receives the soft-keyboard (IME) inset height in pixels from
// MainActivity.nativeKeyboardInset (Android UI thread) and stores it for the
// event loop to pick up. The export lives in the cdylib so the symbol lands in
// libmain.so; the env/class pointers are unused.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_solidrt_app_MainActivity_nativeKeyboardInset(
  _env: *mut core::ffi::c_void,
  _class: *mut core::ffi::c_void,
  px: core::ffi::c_int,
) {
  alloy::set_keyboard_inset_px(px as i32);
}

// --- End Android entry point ------------------------------

const DEFAULT_SOURCE: &str = include_str!("../default-app/app.srt.js");
const BSOD_SOURCE: &str = include_str!("../default-app/bsod.srt.js");

pub(crate) const VERSION: &str = match option_env!("SOLIDRT_VERSION") {
  Some(v) => v,
  None => "0.0.0-dev",
};

/// QuickJS call-stack soft limit. Sits below the UI thread's native stack (see
/// alloy gl::run_context) so deep recursion throws a clean "Maximum call stack
/// size exceeded" instead of overflowing the OS stack. Tunable down per-app later.
const JS_STACK_SIZE: usize = 64 * 1024 * 1024;

/// The app to run: either JS source (dev/default) or precompiled bytecode (packed binary).
pub enum AppSource {
  Text(String),
  Bytecode(Vec<u8>),
}

/// Where to write a live-recorded input script, driven by `--record <path>`
/// on the go client's normal interactive run. The recording stops (and the
/// script is written) when the window closes.
pub struct RecordInputConfig {
  pub path: std::path::PathBuf,
}

/// What to run, threaded from `start` into the UI thread (kept distinct from the
/// runtime plumbing it travels with: the tokio handle, alloy context, channels).
struct RunOptions {
  app: Option<AppSource>,
  playback_fps: Option<u32>,
  stats: bool,
  // Dev-server address to auto-connect on launch (go client only; see plugins::dev).
  dev_server: Option<String>,
  record_input: Option<RecordInputConfig>,
}

fn ui_thread(
  handle: tokio::runtime::Handle,
  atx: Arc<alloy::Context>,
  alloy_cmd_tx: std::sync::mpsc::Sender<alloy::AlloyCommand>,
  event_rx: std::sync::mpsc::Receiver<alloy::AlloyEvent>,
  opts: RunOptions,
) {
  let RunOptions { app, playback_fps, stats, dev_server, record_input } = opts;
  // Only the go dev client consumes the launch dev-server address.
  #[cfg(not(feature = "go"))]
  let _ = dev_server;
  // Anchor the process to a writable directory before any app code runs, so
  // relative paths (e.g. a flux:sqlite database) resolve to persistent storage.
  // The launch cwd is unreliable: on Android it is "/" (read-only); on desktop
  // it is wherever the client was spawned. SDL's pref path is writable and
  // persistent on every platform (and on Android is the same internal-storage
  // dir bundled assets are extracted into). The dev client and packed runtime
  // share one directory. The dev server is a separate process and unaffected.
  match alloy::sdl3::filesystem::get_pref_path("SolidRT", "go") {
    Ok(dir) => match std::env::set_current_dir(&dir) {
      Ok(()) => log::info!("[srt] working directory set to {}", dir.display()),
      Err(e) => log::warn!("[srt] could not set working directory to {}: {e}", dir.display()),
    },
    Err(e) => log::warn!("[srt] no writable pref path, leaving working directory unchanged: {e}"),
  }

  let platform = Arc::new(PlatformContext::new());
  // Playback mode renders every frame unconditionally: the lockstep capture
  // loop blocks waiting for each frame's display list, so a frame skipped by
  // the demand-driven gate would deadlock it.
  platform.set_always_render(matches!(playback_fps, Some(rfps) if rfps > 0));
  platform.set_stats_enabled(stats);
  let input_state = Arc::new(InputState::new());
  let mut current_app = app.unwrap_or_else(|| AppSource::Text(DEFAULT_SOURCE.to_string()));
  let mut showing_bsod = false;

  // Bridge the synchronous Alloy event channel onto an async one: a blocking
  // recv on a dedicated thread forwards each event, so the event loop can await
  // events instead of polling on a timer.
  let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<alloy::AlloyEvent>();
  std::thread::spawn(move || {
    while let Ok(event) = event_rx.recv() {
      if ev_tx.send(event).is_err() {
        break;
      }
    }
  });

  handle.block_on(async {
    let local = tokio::task::LocalSet::new();
    let current_exec: Rc<RefCell<Option<ExecHandle>>> = Rc::new(RefCell::new(None));
    let current_exec_events = current_exec.clone();

    let platform_events = platform.clone();
    let input_state_events = input_state.clone();
    // Virtual present counter the playback-mode clock derives time from (frame/fps),
    // published by the frame verb. Unused in run mode.
    let playback_frame = Arc::new(AtomicU64::new(0));
    // Run-mode pacing for the animation timestamps (see paced_clock). None in
    // playback mode, which uses the deterministic frame/fps clock.
    let paced_clock = match playback_fps {
      Some(rfps) if rfps > 0 => None,
      _ => Some(paced_clock::PacedClock::new()),
    };
    let mut ui_runtime =
      runtime::FluxRuntime::new(current_exec_events, playback_frame.clone(), paced_clock, platform.clone());
    local.spawn_local(async move {
      // Live input recording (see `record_input`): wall-clock elapsed time
      // since this task started, and the actions buffered so far. Written out
      // to `record_input.path` when the window closes (Quit).
      let record_start = record_input.as_ref().map(|_| std::time::Instant::now());
      let mut recorded_actions: Vec<alloy::ScriptedAction> = Vec::new();

      // An event popped ahead while coalescing PointerMove below, held for the
      // next iteration so it is not reordered past other event types.
      let mut pending: Option<AlloyEvent> = None;
      loop {
        let mut event = match pending.take() {
          Some(event) => event,
          None => match ev_rx.recv().await {
            Some(event) => event,
            None => break,
          },
        };
        // Coalesce a burst of PointerMove for the same pointer down to the
        // latest position. Each one costs a hit-test plus a JS dispatch, and a
        // fast mouse produces motion events faster than that pipeline can
        // drain them; without this, the unbounded channel backs up and the
        // hover lag grows for as long as the mouse keeps moving (rendering
        // stays on its own thread and is unaffected).
        if let AlloyEvent::PointerMove { pointer_id, pointer_type, .. } = event {
          while let Ok(next) = ev_rx.try_recv() {
            match next {
              AlloyEvent::PointerMove { pointer_id: next_id, pointer_type: next_type, .. }
                if next_id == pointer_id && next_type == pointer_type =>
              {
                event = next;
              }
              other => {
                pending = Some(other);
                break;
              }
            }
          }
        }
        if let Some(start) = record_start {
          if let Some(script_event) = alloy::ScriptEvent::from_alloy_event(&event) {
            recorded_actions.push(alloy::ScriptedAction { at: start.elapsed().as_secs_f64(), event: script_event });
          }
        }
        // Runner bookkeeping: device and window facts that outlive any single
        // engine (the pointer positions hover refresh reads, the platform's
        // window geometry, fps). Everything engine-facing happens behind the
        // UiRuntime verbs below.
        match &event {
          AlloyEvent::Quit => {
            if let Some(input_cfg) = &record_input {
              write_recorded_script(&input_cfg.path, &recorded_actions);
            }
            std::process::exit(0);
          }
          AlloyEvent::KeyDown { modifiers, .. } | AlloyEvent::KeyUp { modifiers, .. } => {
            input_state_events.set_modifiers(*modifiers);
          }
          AlloyEvent::Resize { size, safe_area, display_scale } => {
            platform_events.set_window_size(size.width as f32, size.height as f32);
            platform_events.set_display_scale(*display_scale);
            platform_events.set_safe_area(*safe_area);
          }
          AlloyEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers }
          | AlloyEvent::PointerDown { pointer_id, pointer_type, x, y, modifiers, .. } => {
            input_state_events.set_pointer_pos((*pointer_type, *pointer_id), *x, *y);
            input_state_events.set_modifiers(*modifiers);
          }
          AlloyEvent::PointerUp { pointer_id, pointer_type, x, y, modifiers, .. } => {
            input_state_events.set_pointer_pos((*pointer_type, *pointer_id), *x, *y);
            input_state_events.set_modifiers(*modifiers);
            // Touch pointers end at release; mouse pointers persist.
            if *pointer_type == alloy::PointerType::Touch {
              input_state_events.remove_pointer((*pointer_type, *pointer_id));
            }
          }
          AlloyEvent::Wheel { modifiers, .. } => input_state_events.set_modifiers(*modifiers),
          AlloyEvent::FrameRendered { fps, .. } | AlloyEvent::Tick { fps, .. } => platform_events.set_fps(*fps),
          _ => {}
        }
        match event {
          // FrameRendered reports the frame native just finished drawing. JS
          // uses the "render" event to compute the NEXT frame's state, so
          // shift the field by +1. The JS-side bootstrap owns frame 0;
          // without the shift, playback mode re-runs frame 0 at tick 0 and
          // duplicates a PNG.
          AlloyEvent::FrameRendered { frame, .. } => ui_runtime.frame(frame + 1),
          // Tick's frame is already the next present index (one past the
          // last FrameRendered), so no +1 here.
          AlloyEvent::Tick { frame, .. } => ui_runtime.frame(frame),
          event => ui_runtime.event(&event),
        }
      }
    });

    #[cfg_attr(not(feature = "go"), allow(unused_variables))]
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<EngineCmd>();
    // The dev-server client: connection supervisor, recents, proxy state and the
    // srt.dev surface. None in playback mode (and entirely absent without the
    // `go` feature). This is the runtime's only seam to the dev client.
    #[cfg(feature = "go")]
    let dev_session = go::DevSession::start(
      &handle,
      cmd_tx.clone(),
      playback_fps,
      &local,
      current_exec.clone(),
      platform.stats_handles(),
      dev_server,
    );

    // flux::Clock backs performance.now() (and the run-mode paced clock corrects
    // toward it). Injected into each engine; persists across reloads for continuous
    // time.
    let clock = match playback_fps {
      // Playback mode: derive time from the present counter (frame/fps) so the
      // whole JS time surface is deterministic and recordings reproducible.
      Some(rfps) if rfps > 0 => {
        let playback_frame = playback_frame.clone();
        flux::Clock::new(move || playback_frame.load(Ordering::Relaxed) as f64 * 1000.0 / rfps as f64)
      }
      // Run mode: wall clock built on tokio's Instant so the time surface stays
      // controllable under tokio's test clock (pause / advance).
      _ => {
        let raf_start = tokio::time::Instant::now();
        flux::Clock::new(move || raf_start.elapsed().as_secs_f64() * 1000.0)
      }
    };

    loop {
      let render_tree = RenderTree::new();
      let platform = platform.clone();
      let atx = atx.clone();
      // A reloaded app must not inherit (or leak) the previous app's open
      // capture devices; their JS handles died with the old engine.
      atx.close_all_cameras();
      atx.close_all_microphones();
      let input_state = input_state.clone();

      let draw_platform = platform.clone();
      let draw_atx = atx.clone();
      #[cfg(feature = "speech")]
      let speech_atx = AlloyContext(atx.clone());
      let builder = FluxEngine::builder().stack_size(JS_STACK_SIZE).logger(|level, msg| match level {
        flux::LogLevel::Debug => log::debug!("{msg}"),
        flux::LogLevel::Log => log::info!("{msg}"),
        flux::LogLevel::Warn => log::warn!("{msg}"),
        flux::LogLevel::Error => log::error!("{msg}"),
      });
      // flux owns the gui plugin set and its registration order; it stores the
      // shared render tree in userdata, which the runner's draw bridge
      // (`srt:render`) reads to draw it. lattice only supplies the host
      // instances they bind.
      let builder = flux::gui::install(
        builder,
        flux::gui::GuiHost {
          platform: platform.clone(),
          alloy: atx.clone(),
          render_tree,
          alloy_cmd_tx: alloy_cmd_tx.clone(),
        },
      );
      let builder = builder
        .plugin(move |ctx| plugins::draw::store_state(&ctx, draw_platform, AlloyContext(draw_atx), input_state))
        .plugin(|ctx| plugins::image::init(ctx))
        .module_override("srt:render", plugins::draw::SrtRenderModule)
        .module_override("srt:events", plugins::events::SrtEventsModule)
        .module_override("srt:dev", plugins::dev::SrtDevModule)
        .userdata(clock.clone());
      #[cfg(feature = "speech")]
      let builder = builder.plugin(move |ctx| plugins::speech::init(ctx, speech_atx));
      // Install the dev-server control surface and (when enabled) the proxy.
      #[cfg(feature = "go")]
      let builder = match &dev_session {
        Some(dev) => dev.augment_builder(builder),
        None => builder,
      };
      let engine = builder.build();
      *current_exec.borrow_mut() = Some(engine.exec_handle());
      alloy_cmd_tx.send(alloy::AlloyCommand::EmitInitEvents).ok();
      // Replay the current connection state into this engine so a reload (e.g.
      // a server stop returning to the default app) keeps the right indicator.
      #[cfg(feature = "go")]
      if let Some(dev) = &dev_session {
        dev.replay_state(&engine.exec_handle());
      }

      log::info!("[srt] flux engine start");
      let mut next_app: Option<AppSource> = None;
      local
        .run_until(async {
          tokio::select! {
            _ = async {
              match &current_app {
                AppSource::Text(src) => engine.eval_source(src).await,
                AppSource::Bytecode(bytes) => engine.eval(bytes.clone()).await,
              }
            } => {}
            Some(cmd) = cmd_rx.recv() => {
              match cmd {
                EngineCmd::Reload(src) => { next_app = Some(AppSource::Text(src)); }
                EngineCmd::Stop => { next_app = Some(AppSource::Text(DEFAULT_SOURCE.to_string())); }
              }
            }
          }
        })
        .await;
      if let Some(app) = next_app {
        current_app = app;
        showing_bsod = false;
      } else if !showing_bsod {
        // Engine exited on its own (a module/startup error means render() never
        // ran, so nothing kept it alive). Show the BSOD instead of a frozen
        // frame; it stays live until a fixed app reloads.
        current_app = AppSource::Text(BSOD_SOURCE.to_string());
        showing_bsod = true;
      } else {
        // The BSOD itself exited; wait for a command rather than respinning.
        match local.run_until(cmd_rx.recv()).await {
          Some(EngineCmd::Reload(src)) => {
            current_app = AppSource::Text(src);
            showing_bsod = false;
          }
          Some(EngineCmd::Stop) => {
            current_app = AppSource::Text(DEFAULT_SOURCE.to_string());
            showing_bsod = false;
          }
          None => break,
        }
      }
    }
  });
}

pub fn start(
  rt: &tokio::runtime::Runtime,
  app_source: Option<AppSource>,
  mode: alloy::Mode,
  size: (u32, u32),
  stats: bool,
  dev_server: Option<String>,
  record_input: Option<RecordInputConfig>,
) {
  alloy::install_logger();
  log::info!("[srt] SolidRT version {VERSION}");

  let handle = rt.handle().clone();
  let playback_fps = match &mode {
    alloy::Mode::Playback(playback) => Some(playback.fps),
    _ => None,
  };
  let app = alloy::setup("SolidRT", ISize::new(size.0 as i64, size.1 as i64), mode);

  let opts = RunOptions { app: app_source, playback_fps, stats, dev_server, record_input };
  app.run(move |atx, alloy_cmd_tx, event_rx| {
    ui_thread(handle, atx, alloy_cmd_tx, event_rx, opts);
  });
}

// Serializes recorded actions to the script JSON schema (`after`-delta steps)
// and writes them to `path`. Requires serde_json, only pulled in by the `go`
// feature (the dev client); the plain packed-app binary never records input.
#[cfg(feature = "go")]
fn write_recorded_script(path: &std::path::Path, actions: &[alloy::ScriptedAction]) {
  #[derive(serde::Serialize)]
  struct ScriptStep {
    after: f64,
    #[serde(rename = "type")]
    kind: &'static str,
    key: String,
  }

  let mut prev_at = 0.0;
  let steps: Vec<ScriptStep> = actions
    .iter()
    .map(|action| {
      let after = action.at - prev_at;
      prev_at = action.at;
      let (kind, keycode) = match action.event {
        alloy::ScriptEvent::KeyDown(k) => ("keydown", k),
        alloy::ScriptEvent::KeyUp(k) => ("keyup", k),
      };
      ScriptStep { after, kind, key: keycode.name() }
    })
    .collect();

  let json = serde_json::to_string_pretty(&steps).expect("serialize recorded script");
  std::fs::write(path, json).unwrap_or_else(|e| panic!("Failed to write '{}': {e}", path.display()));
  log::info!("[srt] wrote {} scripted action(s) to {}", steps.len(), path.display());
}

#[cfg(not(feature = "go"))]
fn write_recorded_script(path: &std::path::Path, actions: &[alloy::ScriptedAction]) {
  let _ = actions;
  log::warn!("[srt] input recording requires the go client build; not writing {}", path.display());
}
