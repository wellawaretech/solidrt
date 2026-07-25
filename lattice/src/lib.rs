mod frame;
#[cfg(feature = "go")]
mod go;
pub mod manifest;
mod overlay;
mod paced_clock;
mod plugins;
mod runtime;
#[cfg(feature = "speech")]
pub mod speech;
pub mod storage;

#[cfg(test)]
mod tests;

#[cfg_attr(not(feature = "go"), allow(dead_code))]
enum EngineCmd {
  // Return to the launcher; only the dev session sends this.
  #[cfg(feature = "go")]
  Stop,
  // `app_id` names the app a dev push belongs to (from its installed
  // manifest); the runtime re-anchors into that app's data sandbox before the
  // reload applies. None for pushes without a manifest (bytecode one-shots,
  // the BSOD trigger), which keep the current sandbox.
  Reload { code: String, app_id: Option<String> },
}

// What "exit the current app" means, decided by host context (see
// okf/plans/exit-to-launcher.md): with the launcher hosting an app (or the
// BSOD), Stop returns to the launcher; at the launcher root, and always in
// launcher-less runtime builds, the client quits - process exit on desktop,
// backgrounding the activity on Android (the platform's back-at-root
// convention). Backs the srt:app exit() verb, which is core's default action
// for an unprevented `back` event.
#[derive(Clone)]
struct ExitPolicy {
  #[cfg(feature = "go")]
  launcher_active: Arc<std::sync::atomic::AtomicBool>,
  #[cfg(feature = "go")]
  engine_tx: tokio::sync::mpsc::UnboundedSender<EngineCmd>,
  alloy_cmd_tx: std::sync::mpsc::Sender<alloy::AlloyCommand>,
}

impl ExitPolicy {
  fn exit(&self) {
    #[cfg(feature = "go")]
    if !self.launcher_active.load(Ordering::Relaxed) {
      // A failed send means the engine loop is already shutting down; there
      // is nothing left to stop.
      let _ = self.engine_tx.send(EngineCmd::Stop);
      return;
    }
    if cfg!(target_os = "android") {
      let _ = self.alloy_cmd_tx.send(alloy::AlloyCommand::Background);
    } else {
      std::process::exit(0);
    }
  }
}

use alloy::impellers::ISize;
use alloy::rendertree::{FontPayload, PlatformContext, RenderTree};
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
  // Android resolves its own sandboxed root; no flags, no packed identity.
  let storage = storage::StorageSpec { data_root: None, client: None, app_id: None };
  start(&rt, None, alloy::Mode::Run, (1280, 720), false, dev_server, embedded_fonts(), storage);
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

// The launcher is the go client's home; the production runtime never
// shows it (it always boots a provided app source), so only go builds embed it.
#[cfg(feature = "go")]
const LAUNCHER_SOURCE: &str = include_str!("../launcher/launcher.srt.js");
const BSOD_SOURCE: &str = include_str!("../launcher/bsod.srt.js");

/// The dev client's built-in fonts: the three Noto role defaults, matching what
/// a default packed app carries in its trailer. The dev loop (launcher,
/// BSOD, HUD, `srt render` golden frames) needs deterministic text without a
/// packed payload, so these stay compiled in; the production runtime ships no
/// font data and registers whatever the trailer carries.
#[cfg(feature = "go")]
pub fn embedded_fonts() -> Vec<FontPayload> {
  use std::borrow::Cow;
  vec![
    FontPayload {
      alias: Some("sans".to_string()),
      bytes: Cow::Borrowed(include_bytes!("../../alloy/assets/fonts/NotoSans.ttf")),
    },
    FontPayload {
      alias: Some("serif".to_string()),
      bytes: Cow::Borrowed(include_bytes!("../../alloy/assets/fonts/NotoSerif.ttf")),
    },
    FontPayload {
      alias: Some("mono".to_string()),
      bytes: Cow::Borrowed(include_bytes!("../../alloy/assets/fonts/NotoSansMono.ttf")),
    },
  ]
}

pub(crate) const VERSION: &str = match option_env!("SOLIDRT_VERSION") {
  Some(v) => v,
  None => "0.0.0-dev",
};

/// Build profile reported to the dev server (list_clients), so "is this a
/// debug binary" is checkable without inspecting the file.
#[cfg_attr(not(feature = "go"), allow(dead_code))]
pub(crate) const PROFILE: &str = if cfg!(debug_assertions) { "debug" } else { "release" };

/// QuickJS call-stack soft limit. Sits below the UI thread's native stack (see
/// alloy gl::run_context) so deep recursion throws a clean "Maximum call stack
/// size exceeded" instead of overflowing the OS stack. Tunable down per-app later.
const JS_STACK_SIZE: usize = 64 * 1024 * 1024;

/// The app to run: either JS source (dev/default) or precompiled bytecode (packed binary).
pub enum AppSource {
  Text(String),
  Bytecode(Vec<u8>),
}

/// What to run, threaded from `start` into the UI thread (kept distinct from the
/// runtime plumbing it travels with: the tokio handle, alloy context, channels).
struct RunOptions {
  app: Option<AppSource>,
  playback_fps: Option<u32>,
  stats: bool,
  // Dev-server address to auto-connect on launch (go client only; see plugins::dev).
  dev_server: Option<String>,
  // Fonts to register at startup (see FontPayload): the go client's embedded
  // Notos, or a packed binary's trailer fonts.
  fonts: Vec<FontPayload>,
  // Data-root resolution inputs (see storage::resolve).
  storage: storage::StorageSpec,
}

// Point the assets mount (see forge::fs) at the app's current installed
// version, so reads under assets/ resolve into the immutable version dir while
// the app runs; an app with nothing installed clears it (assets then come over
// the dev proxy or plain cwd). Re-run on every named reload: a fresh install
// of the same app moves the current version dir.
#[cfg(feature = "go")]
fn mount_assets(app_id: &str) {
  forge::fs::set_assets_base(go::store::current_version_dir(app_id).map(forge::fs::AssetsBase::Dir));
}

// Register the font set for `app_id`: the client's base fonts plus the
// current installed version's manifest fonts, replacing whatever the previous
// app registered. Rebuilt from scratch on every app switch so fonts are
// per-app like the assets mount and the data sandbox - nothing accumulates
// across apps, and no alias is ever registered twice into one context.
#[cfg(feature = "go")]
fn apply_app_fonts(app_id: &str, platform: &PlatformContext, base_fonts: &[FontPayload]) {
  let mut fonts = base_fonts.to_vec();
  fonts.extend(go::store::app_fonts(app_id));
  platform.reset_fonts(fonts);
}

// The mechanics of anchoring, separated from storage resolution so tests can
// exercise deleted-cwd recovery without the process-wide storage global.
// Ok(true) means the cwd moved; Ok(false) that it already pointed at a live
// `data_dir`. A cwd whose inode was unlinked makes current_dir() itself fail,
// so that path re-anchors rather than erroring; canonicalize keeps the
// comparison honest when the storage tree sits behind a symlink.
fn anchor_dir(data_dir: &std::path::Path) -> Result<bool, String> {
  if let Ok(cwd) = std::env::current_dir() {
    if data_dir.canonicalize().is_ok_and(|dir| dir == cwd) {
      return Ok(false);
    }
  }
  std::fs::create_dir_all(data_dir).map_err(|e| format!("cannot create {}: {e}", data_dir.display()))?;
  std::env::set_current_dir(data_dir).map_err(|e| format!("cannot enter {}: {e}", data_dir.display()))?;
  Ok(true)
}

// Anchor the process into `app_id`'s data sandbox (see storage: the cwd is
// the app's persistent data dir). The sandbox can vanish while the client
// runs - the launcher removes an app or wipes its cache once it stopped -
// stranding the cwd on an unlinked inode where every relative open fails, so
// anchoring is re-checked before every engine spin instead of done once.
// Quiet when the cwd is already right; without writable storage the cwd is
// left alone (startup warned once).
fn ensure_anchored(app_id: &str) {
  let Some(store) = storage::get() else { return };
  let data_dir = store.app_dir(app_id).join("data");
  match anchor_dir(&data_dir) {
    Ok(true) => log::info!("[srt] working directory set to {}", data_dir.display()),
    Ok(false) => {}
    Err(e) => log::warn!("[srt] could not anchor working directory: {e}"),
  }
}

// Re-anchor into `app_id`'s data sandbox and record it as the app the process
// should be anchored to. `current` is intent, not observed state: if the
// anchor fails here, the loop-top ensure_anchored retries every engine spin
// until it holds.
fn anchor_app(app_id: &str, current: &mut Option<String>) {
  ensure_anchored(app_id);
  *current = Some(app_id.to_string());
}

fn ui_thread(
  handle: tokio::runtime::Handle,
  atx: Arc<alloy::Context>,
  alloy_cmd_tx: std::sync::mpsc::Sender<alloy::AlloyCommand>,
  event_rx: std::sync::mpsc::Receiver<alloy::AlloyEvent>,
  opts: RunOptions,
) {
  let RunOptions { app, playback_fps, stats, dev_server, fonts, storage: storage_spec } = opts;
  // Only the go dev client consumes the launch dev-server address.
  #[cfg(not(feature = "go"))]
  let _ = dev_server;
  // Resolve the client storage tree, then anchor the process to the app's
  // data sandbox before any app code runs, so relative paths (e.g. a
  // flux:sqlite database) resolve to persistent per-app storage. The launch
  // cwd is unreliable: on Android it is "/" (read-only); on desktop it is
  // wherever the client was spawned. The dev server is a separate process
  // and unaffected.
  storage::init(&storage_spec);
  match storage::get() {
    Some(store) => match std::env::set_current_dir(&store.data_dir) {
      Ok(()) => log::info!("[srt] working directory set to {}", store.data_dir.display()),
      Err(e) => log::warn!("[srt] could not set working directory to {}: {e}", store.data_dir.display()),
    },
    None => log::warn!("[srt] no writable storage, leaving working directory unchanged"),
  }
  // The startup app id: the anchor before any named reload, and what Stop
  // re-anchors to so the launcher never squats in a stopped app's sandbox
  // (removing that app must not fight the cwd, and the loop-top guard must
  // not resurrect its data dir).
  let default_app_id = storage_spec.app_id.clone().unwrap_or_else(|| "default".to_string());
  // The app the process should be anchored to (whose data/ is the cwd). This
  // is intent, not observed state: the loop-top ensure_anchored re-checks it
  // every engine spin, so a sandbox deleted while the client runs (launcher
  // app remove / cache wipe) is rebuilt before the next app run. A dev push
  // naming a different app re-anchors (see anchor_app).
  let mut current_app_id: Option<String> = Some(default_app_id.clone());

  // The client's own fonts (embedded Notos or a packed trailer), kept so app
  // switches can rebuild the font set from scratch (see apply_app_fonts).
  // Embedded font bytes are borrowed Cows, so this clone copies no font data.
  #[cfg(feature = "go")]
  let base_fonts = fonts.clone();
  let platform = Arc::new(PlatformContext::new(fonts));
  // Playback mode renders every frame unconditionally: the lockstep capture
  // loop blocks waiting for each frame's display list, so a frame skipped by
  // the demand-driven gate would deadlock it.
  platform.set_always_render(matches!(playback_fps, Some(rfps) if rfps > 0));
  platform.set_stats_enabled(stats);
  let input_state = Arc::new(InputState::new());
  // The go client's boot rule: no app source means the launcher, always,
  // online or offline. Launched with a dev-server address, the launcher dials
  // it (srt:dev launchAddress) and the server's latched push provides the app;
  // installed apps are launched from the launcher's list, never auto-booted.
  #[cfg(feature = "go")]
  let mut current_app = app.unwrap_or_else(|| AppSource::Text(LAUNCHER_SOURCE.to_string()));
  #[cfg(not(feature = "go"))]
  let mut current_app = app.expect("runtime builds must provide an app source");
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
    // The back watchdog's probe handle and its engine-change guard: bumped on
    // every engine build so a watchdog armed against a dead app never fires
    // into its successor.
    let current_exec_probe = current_exec.clone();
    let engine_generation = Arc::new(AtomicU64::new(0));
    let engine_generation_events = engine_generation.clone();

    let platform_events = platform.clone();
    let input_state_events = input_state.clone();
    let atx_events = atx.clone();
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

    // Live input capture (see `--capture` in dev-server.ts): set from the
    // dev server's `welcome`/`capture` messages (see go::DevSession::start
    // below). Captured events are forwarded to the dev server over the
    // outbound channel, not written locally -- the server decides what to do
    // with them.
    let capture_enabled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let capture_enabled_events = capture_enabled.clone();
    // One outbound text channel to the dev server: capture events, forwarded
    // log lines, and query replies. The connection task drains it into the
    // websocket while connected.
    let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let capture_tx = outbound_tx.clone();
    // True while a dev-server connection is up; gates log forwarding so an
    // offline app never queues log lines (see go::dev_logger).
    let dev_connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Latest stats figures, published by the draw loop every frame; the dev
    // connection answers stats queries from here without touching this thread.
    let stats_snapshot = Arc::new(std::sync::Mutex::new(overlay::StatsSnapshot::default()));
    // Send-safe copy of the live engine's exec handle, refreshed on each engine
    // build; the dev connection uses it to snapshot the render tree on the JS
    // thread for tree queries.
    let query_exec: Arc<std::sync::Mutex<Option<ExecHandle>>> = Arc::new(std::sync::Mutex::new(None));
    #[cfg(not(feature = "go"))]
    let _ = (capture_enabled, outbound_rx, &dev_connected, &outbound_tx);

    local.spawn_local(async move {
      loop {
        // One batch per cycle: block for the first event, then drain whatever
        // else is queued, with two coalescing rules.
        // - PointerMove collapses to the latest position per pointer. Each
        //   move costs a hit-test plus a JS dispatch, and a fast mouse
        //   produces motion faster than that pipeline drains it.
        // - Frame signals (FrameRendered / Tick) collapse to the newest one,
        //   dispatched after the batch's input. A frame signal triggers a
        //   full paint + present on this thread; when presents stall (driver
        //   throttling, an occluded window) the signals pile up in the
        //   unbounded queue, and each stale one replays another paint into
        //   the saturated swapchain - the loop falls arbitrarily far behind,
        //   re-animating old hover states and starving input and dev
        //   queries. Only the newest signal matters; the dropped ones are
        //   exactly the catch-up frames a browser skips too. Playback mode
        //   is lockstep (one FrameRendered in flight, no Ticks), so its
        //   captures never see a collapse.
        let Some(first) = ev_rx.recv().await else { break };
        let mut events: Vec<AlloyEvent> = Vec::new();
        let mut frame_signal: Option<AlloyEvent> = None;
        let mut incoming = Some(first);
        loop {
          let event = match incoming.take() {
            Some(event) => event,
            None => match ev_rx.try_recv() {
              Ok(event) => event,
              Err(_) => break,
            },
          };
          match event {
            signal @ (AlloyEvent::FrameRendered { .. } | AlloyEvent::Tick { .. }) => frame_signal = Some(signal),
            event => events.push(event),
          }
        }
        // Walk from the newest: the first move seen per pointer is its latest
        // position and stays; earlier ones drop.
        let mut seen_moves: Vec<(alloy::PointerType, u64)> = Vec::new();
        let mut keep = vec![true; events.len()];
        for (i, event) in events.iter().enumerate().rev() {
          if let AlloyEvent::PointerMove { pointer_id, pointer_type, .. } = event {
            let key = (*pointer_type, *pointer_id);
            if seen_moves.contains(&key) {
              keep[i] = false;
            } else {
              seen_moves.push(key);
            }
          }
        }
        let batch = events.into_iter().zip(keep).filter_map(|(event, keep)| keep.then_some(event)).chain(frame_signal);
        for event in batch {
          if capture_enabled_events.load(Ordering::Relaxed) {
            if let Some(script_event) = alloy::ScriptEvent::from_alloy_event(&event) {
              let kind = if script_event.down { "keydown" } else { "keyup" };
              // W3C key values include printables like `"` and `\`; escape them
              // so the hand-built JSON line stays valid.
              let key = script_event.key.replace('\\', "\\\\").replace('"', "\\\"");
              let _ = capture_tx.send(format!(r#"{{"type":"capture","kind":"{kind}","key":"{key}"}}"#));
            }
          }
          // Runner bookkeeping: device and window facts that outlive any single
          // engine (the pointer positions hover refresh reads, the platform's
          // window geometry, fps). Everything engine-facing happens behind the
          // UiRuntime verbs below.
          match &event {
            AlloyEvent::Quit => std::process::exit(0),
            AlloyEvent::Key { modifiers, .. } => {
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
            // The back intent dispatches to JS like any window event, backed
            // by a liveness watchdog: the emit just queued runs synchronously
            // on the JS executor, so a probe queued behind it proves the
            // engine processed the dispatch (a handler prevented it, or
            // exit() already ran). No probe by the deadline means the engine
            // is wedged - and a blocked JS thread also blocks EngineCmd
            // handling here, so returning to the launcher is impossible; quit
            // the process so the user is never trapped. Skipped when the
            // engine changed meanwhile: that request belonged to an app that
            // is already gone.
            AlloyEvent::Back => {
              ui_runtime.event(&AlloyEvent::Back);
              let alive = Arc::new(std::sync::atomic::AtomicBool::new(false));
              if let Some(exec) = current_exec_probe.borrow().as_ref() {
                let probe = alive.clone();
                exec.exec(move |_| probe.store(true, Ordering::Relaxed));
              }
              let generation = engine_generation_events.load(Ordering::Relaxed);
              let generations = engine_generation_events.clone();
              tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if !alive.load(Ordering::Relaxed) && generations.load(Ordering::Relaxed) == generation {
                  log::warn!("[srt] app did not respond to back; exiting");
                  std::process::exit(1);
                }
              });
            }
            // Repaint on damage and on return to visibility: the demand gate
            // otherwise leaves the recreated/undefined surface unpresented
            // forever (Android destroys the EGL surface on background; an
            // idle app never repaints on its own). The latch is cheap and
            // the cached display list makes the frame present-only, so
            // over-triggering is harmless. Visibility does not reach JS yet
            // (that is the lifecycle plan's stage 3).
            AlloyEvent::Exposed => platform_events.request_frame(),
            AlloyEvent::Visibility { visible } => {
              // Rare lifecycle transitions; logged so device traces show
              // whether and when the platform reported them (the resume
              // repaint pipeline depends on it).
              log::info!("[srt] visibility: {}", if visible { "visible" } else { "hidden" });
              if visible {
                // Rebind first: the command channel is ordered, so the raster
                // thread picks up the recreated EGL surface before the
                // repaint's frame arrives (device-verified failure mode:
                // EGL_BAD_SURFACE on the first post-resume present).
                atx_events.rebind_window_surface();
                platform_events.request_frame();
              }
              // Forward to the engine as the sticky `visibility` event.
              // Same-state repeats are normal here (app + window paths, plus
              // the Android background watch); core's env signal dedupes.
              ui_runtime.event(&AlloyEvent::Visibility { visible });
            }
            event => ui_runtime.event(&event),
          }
        }
      }
    });

    #[cfg_attr(not(feature = "go"), allow(unused_variables))]
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<EngineCmd>();
    // True while the current engine runs the launcher itself (set at each
    // engine build); exit() at the launcher root quits instead of Stop-ing.
    #[cfg(feature = "go")]
    let launcher_active = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let exit_policy = ExitPolicy {
      #[cfg(feature = "go")]
      launcher_active: launcher_active.clone(),
      #[cfg(feature = "go")]
      engine_tx: cmd_tx.clone(),
      alloy_cmd_tx: alloy_cmd_tx.clone(),
    };
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
      capture_enabled,
      dev_connected.clone(),
      outbound_rx,
      go::QueryHandles { stats: stats_snapshot.clone(), exec: query_exec.clone(), outbound_tx: outbound_tx.clone() },
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

    if storage::get().is_none() {
      log::warn!("No fetch cache dir; fetch caching disabled");
    }

    loop {
      // Re-anchor before anything in this spin touches the sandbox: the data
      // dir may have been deleted since the last spin, and a reload naming
      // the SAME app would otherwise keep the stranded cwd forever.
      if let Some(app_id) = &current_app_id {
        ensure_anchored(app_id);
      }
      // Fetch disk cache: per app, so cached assets are browsable and
      // clearable per app (and die with it on remove). Resolved per engine:
      // the anchored app changes across reloads.
      let fetch_cache_dir = storage::get()
        .map(|store| store.cache_dir(current_app_id.as_deref().unwrap_or("default")));
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
      let builder = FluxEngine::builder().stack_size(JS_STACK_SIZE).user_agent(format!("SolidRT/{VERSION}"));
      let builder = match &fetch_cache_dir {
        Some(dir) => builder.cache_dir(dir.clone()),
        None => builder,
      };
      // The go client's logger also forwards lines to a connected dev server;
      // other builds log locally only.
      #[cfg(feature = "go")]
      let builder = builder.logger(go::dev_logger(outbound_tx.clone(), dev_connected.clone()));
      #[cfg(not(feature = "go"))]
      let builder = builder.logger(|level, msg| match level {
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
      let draw_stats = stats_snapshot.clone();
      let builder = builder
        .plugin(move |ctx| {
          plugins::draw::store_state(&ctx, draw_platform, AlloyContext(draw_atx), input_state, draw_stats)
        })
        .plugin(|ctx| plugins::image::init(ctx))
        .module_override("srt:render", plugins::draw::SrtRenderModule)
        .module_override("srt:events", plugins::events::SrtEventsModule)
        .module_override("srt:dev", plugins::dev::SrtDevModule)
        .module_override("srt:apps", plugins::apps::SrtAppsModule)
        .module_override("srt:app", plugins::app::SrtAppModule)
        .userdata(clock.clone());
      // The running app's own surface (exit()), in every build: the
      // production runtime exits too, it just always quits.
      let builder = {
        let policy = exit_policy.clone();
        builder.plugin(move |ctx| {
          plugins::app::install(
            &ctx,
            plugins::app::AppControl::new(plugins::app::AppControlInner { exit: Box::new(move || policy.exit()) }),
          )
        })
      };
      #[cfg(feature = "speech")]
      let builder = builder.plugin(move |ctx| plugins::speech::init(ctx, speech_atx));
      // The launcher's app-management surface over the version store; the
      // launch closure feeds the same reload path a dev push uses.
      #[cfg(feature = "go")]
      let builder = {
        let apps_tx = cmd_tx.clone();
        builder.plugin(move |ctx| go::control::install_apps_control(ctx, apps_tx))
      };
      // Install the dev-server control surface and (when enabled) the proxy.
      #[cfg(feature = "go")]
      let builder = match &dev_session {
        Some(dev) => dev.augment_builder(builder),
        None => builder,
      };
      let engine = builder.build();
      *current_exec.borrow_mut() = Some(engine.exec_handle());
      engine_generation.fetch_add(1, Ordering::Relaxed);
      #[cfg(feature = "go")]
      launcher_active.store(
        matches!(&current_app, AppSource::Text(src) if src.as_str() == LAUNCHER_SOURCE),
        Ordering::Relaxed,
      );
      *query_exec.lock().expect("query exec lock poisoned") = Some(engine.exec_handle());
      alloy_cmd_tx.send(alloy::AlloyCommand::EmitInitEvents).ok();
      // Replay the current connection state into this engine so a reload (e.g.
      // a server stop returning to the launcher) keeps the right indicator.
      #[cfg(feature = "go")]
      if let Some(dev) = &dev_session {
        dev.replay_state(&engine.exec_handle());
      }

      log::info!("[srt] flux engine start");
      let mut next_app: Option<AppSource> = None;
      let mut next_app_id: Option<String> = None;
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
                EngineCmd::Reload { code, app_id } => {
                  next_app = Some(AppSource::Text(code));
                  next_app_id = app_id;
                }
                #[cfg(feature = "go")]
                EngineCmd::Stop => {
                  next_app = Some(AppSource::Text(LAUNCHER_SOURCE.to_string()));
                  // Back to the launcher: release the stopped app's sandbox
                  // by re-anchoring to the startup default, so the launcher
                  // can remove the app without the cwd (or the loop-top
                  // guard) holding its data dir alive. Its fonts go with it -
                  // the launcher runs on the base set alone.
                  current_app_id = Some(default_app_id.clone());
                  platform.reset_fonts(base_fonts.clone());
                }
              }
            }
          }
        })
        .await;
      if let Some(app) = next_app {
        if let Some(app_id) = &next_app_id {
          anchor_app(app_id, &mut current_app_id);
          #[cfg(feature = "go")]
          mount_assets(app_id);
          #[cfg(feature = "go")]
          apply_app_fonts(app_id, &platform, &base_fonts);
        }
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
          Some(EngineCmd::Reload { code, app_id }) => {
            if let Some(app_id) = &app_id {
              anchor_app(app_id, &mut current_app_id);
              #[cfg(feature = "go")]
              mount_assets(app_id);
              #[cfg(feature = "go")]
              apply_app_fonts(app_id, &platform, &base_fonts);
            }
            current_app = AppSource::Text(code);
            showing_bsod = false;
          }
          #[cfg(feature = "go")]
          Some(EngineCmd::Stop) => {
            current_app = AppSource::Text(LAUNCHER_SOURCE.to_string());
            showing_bsod = false;
            // Same sandbox and font release as the in-loop Stop arm above.
            current_app_id = Some(default_app_id.clone());
            platform.reset_fonts(base_fonts.clone());
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
  fonts: Vec<FontPayload>,
  storage: storage::StorageSpec,
) {
  alloy::install_logger();
  log::info!("[srt] SolidRT version {VERSION}");

  let handle = rt.handle().clone();
  let playback_fps = match &mode {
    alloy::Mode::Playback(playback) => Some(playback.fps),
    _ => None,
  };
  let app = alloy::setup("SolidRT", ISize::new(size.0 as i64, size.1 as i64), mode);

  let opts = RunOptions { app: app_source, playback_fps, stats, dev_server, fonts, storage };
  app.run(move |atx, alloy_cmd_tx, event_rx| {
    ui_thread(handle, atx, alloy_cmd_tx, event_rx, opts);
  });
}
